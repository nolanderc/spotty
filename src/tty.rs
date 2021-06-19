pub mod control_code;

pub struct PsuedoterminalLink {
    pub child: nix::unistd::Pid,
    pub master_fd: nix::pty::PtyMaster,
}

impl PsuedoterminalLink {
    pub fn create() -> nix::Result<PsuedoterminalLink> {
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

                Ok(PsuedoterminalLink { child, master_fd })
            }
        }
    }
}

pub struct Psuedoterminal {
    master_fd: nix::pty::PtyMaster,
    input: flume::Sender<TerminalInput>,
    output: flume::Receiver<TerminalOutput>,
}

enum TerminalInput {
    Char(char),
    Bytes(Box<[u8]>),
}

pub type TerminalOutput = Box<[u8]>;

#[derive(Debug, Copy, Clone)]
pub enum TryReadError {
    Empty,
    Closed,
}

impl Psuedoterminal {
    pub fn connect(pty: PsuedoterminalLink, waker: crate::window::EventLoopWaker) -> Psuedoterminal {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        let (input, receiver) = flume::bounded(32);
        let (sender, output) = flume::bounded(32);

        let reader = unsafe {
            std::fs::File::from_raw_fd(nix::unistd::dup(pty.master_fd.as_raw_fd()).unwrap())
        };
        let writer = unsafe {
            std::io::BufWriter::new(std::fs::File::from_raw_fd(
                nix::unistd::dup(pty.master_fd.as_raw_fd()).unwrap(),
            ))
        };

        std::thread::spawn(move || Self::handle_terminal_input(receiver, writer).unwrap());
        std::thread::spawn(move || Self::handle_terminal_output(sender, reader, waker).unwrap());

        Psuedoterminal {
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

    pub fn try_read(&self) -> Result<TerminalOutput, TryReadError> {
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
        mut reader: std::fs::File,
        waker: crate::window::EventLoopWaker,
    ) -> std::io::Result<()> {
        use std::io::Read;

        const BUFFER_SIZE: usize = 8 * 1024;

        let mut buffer = [0; BUFFER_SIZE];

        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }

            let bytes = &buffer[..count];
            sender.send(bytes.to_vec().into_boxed_slice()).unwrap();
            waker.wake();
        }

        Ok(())
    }
}
