pub struct Psuedoterminal {
    pub child: nix::unistd::Pid,
    pub master_fd: nix::pty::PtyMaster,
}

impl Psuedoterminal {
    pub fn create() -> nix::Result<Psuedoterminal> {
        use nix::fcntl::OFlag;

        // Open a new PTY master
        let master_fd = nix::pty::posix_openpt(OFlag::O_RDWR)?;

        // Allow a slave to be generated for it
        nix::pty::grantpt(&master_fd)?;
        nix::pty::unlockpt(&master_fd)?;

        // Get the name of the slave
        let slave_name = unsafe { nix::pty::ptsname(&master_fd) }?;

        // Try to open the slave
        let slave_path = std::path::Path::new(&slave_name);
        let slave_fd = nix::fcntl::open(slave_path, OFlag::O_RDWR, nix::sys::stat::Mode::empty())?;

        // Spawn the child process
        match unsafe { nix::unistd::fork()? } {
            nix::unistd::ForkResult::Child => {
                drop(master_fd);
                nix::unistd::setsid()?;

                // Overwrite the old stdio
                nix::unistd::dup2(slave_fd, 0)?;
                nix::unistd::dup2(slave_fd, 1)?;
                nix::unistd::dup2(slave_fd, 2)?;

                unsafe {
                    nix::ioctl_write_int_bad!(set_controlling_terminal, nix::libc::TIOCSCTTY);
                    set_controlling_terminal(slave_fd, 0)?;
                }

                nix::unistd::close(slave_fd)?;

                fn c_str(text: &[u8]) -> &std::ffi::CStr {
                    std::ffi::CStr::from_bytes_with_nul(text).unwrap()
                }

                // Launch a shell
                let program = c_str(b"/bin/sh\0");
                let args: &[&std::ffi::CStr] = &[];

                let result = nix::unistd::execv(program, args)?;
                match result {}
            }
            nix::unistd::ForkResult::Parent { child } => {
                nix::unistd::close(slave_fd)?;

                Ok(Psuedoterminal { child, master_fd })
            }
        }
    }
}

pub struct Terminal {
    master_fd: nix::pty::PtyMaster,
    input: flume::Sender<TerminalInput>,
    output: flume::Receiver<TerminalOutput>,
}

pub type TerminalInput = char;
pub type TerminalOutput = Vec<TerminalCode>;

#[derive(Debug, Clone)]
pub enum TerminalCode {
    Char(char),

    /// Makes an audible bell
    Bell,
    /// Move cursor to next column that is an multiple of 8
    Tab,

    /// Move cursor to the left, might wrap to previous line
    Backspace,

    /// `\r`
    CarriageReturn,
    /// `\n`
    LineFeed,

    /// Clear from cursor to the end of the line
    ClearLineToEnd,
    /// Clear from start of the line to the cursor
    ClearLineToStart,
    /// Clear entire line
    ClearLine,
}

#[derive(Debug, Copy, Clone)]
pub enum TryReadError {
    Empty,
    Closed,
}

impl Terminal {
    pub fn connect(pty: Psuedoterminal, waker: crate::window::EventLoopWaker) -> Terminal {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        let (input, receiver) = flume::bounded(32);
        let (sender, output) = flume::bounded(32);

        let reader = unsafe {
            std::io::BufReader::new(std::fs::File::from_raw_fd(
                nix::unistd::dup(pty.master_fd.as_raw_fd()).unwrap(),
            ))
        };
        let writer = unsafe {
            std::io::BufWriter::new(std::fs::File::from_raw_fd(
                nix::unistd::dup(pty.master_fd.as_raw_fd()).unwrap(),
            ))
        };

        std::thread::spawn(move || Self::handle_terminal_input(receiver, writer).unwrap());
        std::thread::spawn(move || Self::handle_terminal_output(sender, reader, waker).unwrap());

        Terminal {
            master_fd: pty.master_fd,
            input,
            output,
        }
    }

    pub fn set_grid_size(&self, size: [u16; 2]) {
        use std::os::unix::io::AsRawFd;

        unsafe {
            nix::ioctl_write_ptr_bad!(set_window_size, nix::libc::TIOCSWINSZ, nix::libc::winsize);
            let new_size = nix::libc::winsize {
                ws_row: size[1],
                ws_col: size[0],
                ws_xpixel: 0,
                ws_ypixel: 0,
            };

            set_window_size(self.master_fd.as_raw_fd(), &new_size as *const _).unwrap();
        }
    }

    pub fn try_read(&self) -> Result<Vec<TerminalCode>, TryReadError> {
        self.output.try_recv().map_err(|err| match err {
            flume::TryRecvError::Empty => TryReadError::Empty,
            flume::TryRecvError::Disconnected => TryReadError::Closed,
        })
    }

    pub fn send(&self, ch: char) {
        let _ = self.input.send(ch);
    }

    fn handle_terminal_input(
        receiver: flume::Receiver<TerminalInput>,
        mut writer: std::io::BufWriter<std::fs::File>,
    ) -> std::io::Result<()> {
        use std::io::Write;

        let into_try_recv_error = |error| match error {
            flume::RecvError::Disconnected => flume::TryRecvError::Disconnected,
        };

        let mut next_item = receiver.recv().map_err(into_try_recv_error);

        loop {
            match next_item {
                Ok(message) => {
                    let mut buffer = [0u8; 4];
                    let encoded = message.encode_utf8(&mut buffer).as_bytes();
                    writer.write_all(encoded)?;
                    next_item = receiver.try_recv();
                }
                Err(flume::TryRecvError::Empty) => {
                    writer.flush()?;
                    next_item = receiver.recv().map_err(into_try_recv_error);
                }
                Err(flume::TryRecvError::Disconnected) => break,
            }
        }

        Ok(())
    }

    fn handle_terminal_output(
        sender: flume::Sender<TerminalOutput>,
        mut reader: std::io::BufReader<std::fs::File>,
        waker: crate::window::EventLoopWaker,
    ) -> std::io::Result<()> {
        use std::io::Read;

        const BUFFER_SIZE: usize = 8 * 1024;

        let mut buffer = [0; BUFFER_SIZE];
        let mut valid_until = 0;

        let mut codes = Vec::with_capacity(BUFFER_SIZE);

        loop {
            let count = reader.read(&mut buffer[valid_until..])?;
            if count == 0 {
                break;
            }

            let buffer_end = valid_until + count;
            let mut bytes = &buffer[..buffer_end];

            loop {
                match std::str::from_utf8(bytes) {
                    Ok(text) => {
                        // Could be all valid input, or an incomplete escape sequence
                        bytes = TerminalCode::parse(text, |code| codes.push(code)).as_bytes();
                        break;
                    }
                    Err(error) => {
                        let (valid, invalid) = bytes.split_at(error.valid_up_to());

                        // SAFETY: we know that everything up to `error.valid_up_to` is valid UTF-8
                        let text = unsafe { std::str::from_utf8_unchecked(valid) };

                        let rest = TerminalCode::parse(text, |code| codes.push(code));

                        if invalid.len() < 4 {
                            if !rest.is_empty() {
                                // Invalid unicode following an invalid escape sequence
                                codes.push(TerminalCode::Char(char::REPLACEMENT_CHARACTER));
                            }

                            // Could be a valid unicode code point that has not loaded fully yet
                            bytes = invalid;
                            break;
                        } else {
                            // Invalid unicode, potentially following an invalid escape sequence
                            codes.push(TerminalCode::Char(char::REPLACEMENT_CHARACTER));
                            bytes = &invalid[1..];
                        }
                    }
                }
            }

            valid_until = bytes.len();
            let rest_start = buffer_end - bytes.len();
            buffer.copy_within(rest_start..buffer_end, 0);

            sender.send(codes.clone()).unwrap();
            codes.clear();
            waker.wake();
        }

        Ok(())
    }
}

impl TerminalCode {
    fn parse(text: &str, mut emit: impl FnMut(TerminalCode)) -> &str {
        let mut chars = text.chars();

        let mut unprocessed;

        'outer: loop {
            unprocessed = chars.as_str();
            let ch = match chars.next() {
                None => break 'outer,
                Some(ch) => ch,
            };

            match ch {
                '\x07' => emit(TerminalCode::Bell),
                '\x08' => emit(TerminalCode::Backspace),
                '\x09' => emit(TerminalCode::Tab),
                '\r' => emit(TerminalCode::CarriageReturn),
                '\n' => emit(TerminalCode::LineFeed),
                '\x1b' => match chars.next() {
                    // Control Sequence
                    Some('[') => {
                        let bytes = chars.as_str().as_bytes();
                        match Self::parse_control_sequence_arguments(bytes) {
                            Err(rest) => {
                                eprintln!(
                                    "invalid control sequence arguments: {:?}",
                                    &text[text.len() - bytes.len()..text.len() - rest.len()]
                                );

                                chars = text[text.len() - rest.len()..].chars();
                                emit(TerminalCode::Char(char::REPLACEMENT_CHARACTER))
                            }
                            Ok((arguments, private, rest)) => {
                                chars = text[text.len() - rest.len()..].chars();

                                match chars.next() {
                                    None => break 'outer,
                                    Some(ch) if !private => match ch {
                                        'K' => match arguments[0] {
                                            0 => emit(TerminalCode::ClearLineToEnd),
                                            1 => emit(TerminalCode::ClearLineToStart),
                                            2 | _ => emit(TerminalCode::ClearLine),
                                        },
                                        _ => {
                                            let sequence_start = text.len() - unprocessed.len();
                                            let sequence_end = text.len() - chars.as_str().len();
                                            let sequence = &text[sequence_start..sequence_end];
                                            eprintln!("unknown control sequence: {:?}", sequence);

                                            emit(TerminalCode::Char(char::REPLACEMENT_CHARACTER));
                                        }
                                    },
                                    Some(ch) => match ch {
                                        _ => {
                                            let sequence_start = text.len() - unprocessed.len();
                                            let sequence_end = text.len() - chars.as_str().len();
                                            let sequence = &text[sequence_start..sequence_end];
                                            eprintln!("unknown control sequence: {:?}", sequence);

                                            emit(TerminalCode::Char(char::REPLACEMENT_CHARACTER));
                                        }
                                    },
                                }
                            }
                        }
                    }
                    _ => {
                        eprintln!("unknown escape sequnce: {:?}", ch);
                        emit(TerminalCode::Char(char::REPLACEMENT_CHARACTER));
                    }
                },
                _ => emit(TerminalCode::Char(ch)),
            }
        }

        unprocessed
    }

    fn parse_control_sequence_arguments(input: &[u8]) -> Result<([u16; 5], bool, &[u8]), &[u8]> {
        let mut arguments = [0; 5];
        let mut argument_index = 0;

        let mut private = false;

        let mut bytes = input.iter();

        while let Some(byte) = bytes.next() {
            match byte {
                b'0'..=b'9' => {
                    arguments[argument_index] *= 10;
                    arguments[argument_index] += u16::from(byte - b'0');
                }
                b':' | b';' => {
                    argument_index += 1;
                    if argument_index > arguments.len() {
                        return Err(bytes.as_slice());
                    }
                }

                // reserved for private use
                b'?' if !private => private = true,
                b'<'..=b'?' => return Err(bytes.as_slice()),

                _ => {
                    let remaining_bytes = bytes.len() + 1;
                    return Ok((arguments, private, &input[input.len() - remaining_bytes..]));
                }
            }
        }

        Err(&[])
    }
}
