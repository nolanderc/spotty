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
                let program = c_str(b"/bin/zsh\0");
                let args: &[&std::ffi::CStr] = &[c_str(b"-i\0")];

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

enum TerminalInput {
    Char(char),
    Bytes(Box<[u8]>),
}

type TerminalOutput = Vec<TerminalCode>;

#[derive(Debug, Clone)]
pub enum TerminalCode {
    Unknown(Box<str>),
    Ignored(Box<str>),

    Char(char),
    Text(Box<str>),

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

    MoveCursor {
        direction: Direction,
        steps: u16,
    },

    /// Erase some number of characters
    Erase {
        count: u16
    },

    SetCursorPos([u16; 2]),

    /// Clear from cursor to the end of the screen
    ClearScreenToEnd,
    /// Clear from cursor to the start of the screen
    ClearScreenToStart,
    /// Clear entire screen
    ClearScreen,
    /// Clear entire screen and scrollback buffer
    ClearScreenAndScrollback,

    /// Clear from cursor to the end of the line
    ClearLineToEnd,
    /// Clear from start of the line to the cursor
    ClearLineToStart,
    /// Clear entire line
    ClearLine,

    /// If enabled: surround text pasted into terminal with `ESC [200~` and `ESC [201~`
    SetBracketedPaste(bool),

    /// If enabled: arrow keys should send application codes instead of ANSI codes
    SetApplicationCursor(bool),

    /// Sets the title of the window
    SetWindowTitle(Box<str>),
}

#[derive(Debug, Clone)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
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
                ws_row: size[0],
                ws_col: size[1],
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

    pub fn send_char(&self, ch: char) {
        let _ = self.input.send(TerminalInput::Char(ch));
    }

    pub fn write_fmt(&self, args: std::fmt::Arguments) {
        let bytes = args.to_string().into_bytes().into_boxed_slice();
        let _ = self.input.send(TerminalInput::Bytes(bytes));
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
                Ok(TerminalInput::Char(ch)) => {
                    let mut buffer = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut buffer).as_bytes();
                    writer.write_all(encoded)?;
                    next_item = receiver.try_recv();
                }
                Ok(TerminalInput::Bytes(bytes)) => {
                    writer.write_all(&bytes)?;
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

type ParseResult<T> = Result<T, ParseError>;

enum ParseError {
    Incomplete,
    Invalid,
    Ignored,
}

impl TerminalCode {
    fn parse(text: &str, mut emit: impl FnMut(TerminalCode)) -> &str {
        let mut chars = text.chars();

        let mut unprocessed = chars.as_str();

        loop {
            match chars.next() {
                None => {
                    if unprocessed.len() > 0 {
                        emit(TerminalCode::Text(unprocessed.to_owned().into_boxed_str()));
                    }
                    return "";
                }
                Some(ch) if ch.is_ascii_control() => {
                    let remaining_bytes = 1 + chars.as_str().len();
                    let text_len = unprocessed.len() - remaining_bytes;
                    if text_len > 0 {
                        let (text, rest) = unprocessed.split_at(text_len);
                        emit(TerminalCode::Text(text.to_owned().into_boxed_str()));
                        unprocessed = rest;
                    }

                    let parse_result = match ch {
                        '\x07' => Ok(TerminalCode::Bell),
                        '\x08' => Ok(TerminalCode::Backspace),
                        '\x09' => Ok(TerminalCode::Tab),
                        '\r' => Ok(TerminalCode::CarriageReturn),
                        '\n' => Ok(TerminalCode::LineFeed),
                        '\x1b' => match chars.next() {
                            // Control Sequence
                            Some('[') => Self::parse_ansi_control_sequence(&mut chars),

                            // Operating System Command
                            Some(']') => Self::parse_operating_system_command(&mut chars),

                            // sets the character set: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
                            Some('(') => match chars.next() {
                                None => Err(ParseError::Incomplete),
                                Some(_) => Err(ParseError::Ignored),
                            },

                            // Enter alternate keypad mode
                            Some('=') => Err(ParseError::Ignored),

                            // Exit alternate keypad mode
                            Some('>') => Err(ParseError::Ignored),

                            Some(_) => Err(ParseError::Invalid),
                            None => Err(ParseError::Incomplete),
                        },
                        _ => Err(ParseError::Invalid),
                    };

                    match parse_result {
                        Ok(code) => emit(code),
                        Err(ParseError::Incomplete) => return unprocessed,
                        Err(ParseError::Invalid) => {
                            let len = chars.as_str().len();
                            let sequence = &unprocessed[..unprocessed.len() - len];
                            emit(TerminalCode::Unknown(sequence.to_owned().into_boxed_str()));
                        }
                        Err(ParseError::Ignored) => {
                            let len = chars.as_str().len();
                            let sequence = &unprocessed[..unprocessed.len() - len];
                            emit(TerminalCode::Ignored(sequence.to_owned().into_boxed_str()));
                        }
                    }

                    unprocessed = chars.as_str();
                }
                _ => continue,
            }
        }
    }

    fn parse_ansi_control_sequence(chars: &mut std::str::Chars) -> ParseResult<TerminalCode> {
        let (parameters, intermediate, terminator) = Self::parse_control_sequence_parts(chars)?;

        match (parameters, intermediate, terminator) {
            ("?2004", "", b'h') => Ok(TerminalCode::SetBracketedPaste(true)),
            ("?2004", "", b'l') => Ok(TerminalCode::SetBracketedPaste(false)),

            ("?1", "", b'h') => Ok(TerminalCode::SetApplicationCursor(true)),
            ("?1", "", b'l') => Ok(TerminalCode::SetApplicationCursor(false)),

            _ => {
                // TODO: figure out how to handle intermediate bytes
                if !intermediate.is_empty() {
                    return Err(ParseError::Invalid);
                }

                let arguments = Self::parse_control_sequence_arguments(parameters.as_bytes())?;
                Self::parse_standard_terminator(arguments, terminator)
            }
        }
    }

    fn parse_control_sequence_arguments(input: &[u8]) -> ParseResult<ControlSequenceArguments> {
        let mut arguments = ControlSequenceArguments { values: [0; 5] };
        let mut argument_index = 0;

        for &byte in input {
            match byte {
                b'0'..=b'9' => {
                    arguments.values[argument_index] *= 10;
                    arguments.values[argument_index] += u16::from(byte - b'0');
                }
                b':' | b';' => {
                    argument_index += 1;
                    if argument_index >= arguments.values.len() {
                        return Err(ParseError::Invalid);
                    }
                }

                // reserved for private use
                _ => return Err(ParseError::Invalid),
            }
        }

        Ok(arguments)
    }

    fn parse_control_sequence_parts<'a>(
        chars: &mut std::str::Chars<'a>,
    ) -> ParseResult<(&'a str, &'a str, u8)> {
        let parameters = take_while_in_range(chars, 0x30..=0x3f)?;
        let intermediate = take_while_in_range(chars, 0x20..=0x2f)?;
        let terminator = chars.next().ok_or(ParseError::Incomplete)?;

        if ('\x40'..='\x7f').contains(&terminator) {
            Ok((parameters, intermediate, terminator as u8))
        } else {
            Err(ParseError::Invalid)
        }
    }

    fn parse_standard_terminator(
        arguments: ControlSequenceArguments,
        ch: u8,
    ) -> ParseResult<TerminalCode> {
        fn move_cursor(direction: Direction, arguments: ControlSequenceArguments) -> TerminalCode {
            TerminalCode::MoveCursor {
                direction,
                steps: arguments.get_default(0, 1),
            }
        }

        match ch {
            b'A' => Ok(move_cursor(Direction::Up, arguments)),
            b'B' => Ok(move_cursor(Direction::Down, arguments)),
            b'C' => Ok(move_cursor(Direction::Right, arguments)),
            b'D' => Ok(move_cursor(Direction::Left, arguments)),

            b'H' => Ok(TerminalCode::SetCursorPos([
                arguments.get_default(0, 1) - 1,
                arguments.get_default(0, 1) - 1,
            ])),

            b'J' => match arguments.get(0) {
                0 => Ok(TerminalCode::ClearScreenToEnd),
                1 => Ok(TerminalCode::ClearScreenToStart),
                2 => Ok(TerminalCode::ClearScreen),
                3 => Ok(TerminalCode::ClearScreenAndScrollback),
                _ => Err(ParseError::Invalid),
            },

            b'K' => match arguments.values[0] {
                0 => Ok(TerminalCode::ClearLineToEnd),
                1 => Ok(TerminalCode::ClearLineToStart),
                2 => Ok(TerminalCode::ClearLine),
                _ => Err(ParseError::Invalid),
            },

            b'X' => Ok(TerminalCode::Erase { count: arguments.get_default(0, 1) }),

            _ => Err(ParseError::Invalid),
        }
    }

    fn parse_operating_system_command(chars: &mut std::str::Chars) -> ParseResult<TerminalCode> {
        let numbers = take_while_in_range(chars, b'0'..=b'9')?;
        let parameters = take_while_in_range(chars, 0x20..=0x7e)?;
        let terminator = chars.next().ok_or(ParseError::Incomplete)?;

        if !matches!(terminator, '\x07') {
            return Err(ParseError::Invalid);
        }

        let parameters = parameters.strip_prefix(';').ok_or(ParseError::Invalid)?;

        match numbers {
            // Change "icon name" and window title. The former does not apply.
            "0" => Ok(TerminalCode::SetWindowTitle(
                parameters.to_owned().into_boxed_str(),
            )),

            // Change "icon name" (does not apply)
            "1" => Err(ParseError::Ignored),

            // Change window title.
            "2" => Ok(TerminalCode::SetWindowTitle(
                parameters.to_owned().into_boxed_str(),
            )),

            // Set X-property on top-level window (does not apply)
            "3" => Err(ParseError::Ignored),

            _ => Err(ParseError::Invalid),
        }
    }
}

fn take_while<'a>(
    chars: &mut std::str::Chars<'a>,
    mut predicate: impl FnMut(u8) -> bool,
) -> ParseResult<&'a str> {
    let text = chars.as_str();
    let bytes = text.as_bytes();

    if bytes.is_empty() {
        return Err(ParseError::Incomplete);
    }

    let mut i = 0;
    loop {
        match bytes.get(i) {
            Some(byte) if predicate(*byte) => i += 1,
            None => return Err(ParseError::Incomplete),
            _ => break,
        }
    }

    if text.is_char_boundary(i) {
        let (matching, rest) = text.split_at(i);
        *chars = rest.chars();
        Ok(matching)
    } else {
        Err(ParseError::Invalid)
    }
}

fn take_while_in_range<'a, R>(chars: &mut std::str::Chars<'a>, range: R) -> ParseResult<&'a str>
where
    R: std::ops::RangeBounds<u8>,
{
    take_while(chars, |byte| range.contains(&byte))
}

struct ControlSequenceArguments {
    values: [u16; 5],
}

impl ControlSequenceArguments {
    pub fn get_default(&self, index: usize, default: u16) -> u16 {
        match self.values[index] {
            0 => default,
            value => value,
        }
    }

    pub fn get(&self, index: usize) -> u16 {
        self.values[index]
    }
}
