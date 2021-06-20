pub mod control_code;

use crate::inline::InlineBytes;

pub struct Psuedoterminal {
    master_fd: nix::pty::PtyMaster,
    input: flume::Sender<InlineBytes>,
    output: flume::Receiver<InlineBytes>,
}

#[derive(Debug, Copy, Clone)]
pub enum TryReadError {
    Empty,
    Closed,
}

impl Psuedoterminal {
    pub fn connect(waker: crate::window::EventLoopWaker) -> nix::Result<Psuedoterminal> {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        let link = PsuedoterminalLink::create()?;

        let (input, receiver) = flume::bounded(256);
        let (sender, output) = flume::bounded(256);

        let reader = unsafe {
            std::fs::File::from_raw_fd(nix::unistd::dup(link.master_fd.as_raw_fd()).unwrap())
        };
        let writer = unsafe {
            std::io::BufWriter::new(std::fs::File::from_raw_fd(
                nix::unistd::dup(link.master_fd.as_raw_fd()).unwrap(),
            ))
        };

        std::thread::spawn(move || Self::handle_terminal_input(receiver, writer).unwrap());
        std::thread::spawn(move || Self::handle_terminal_output(sender, reader, waker).unwrap());

        Ok(Psuedoterminal {
            master_fd: link.master_fd,
            input,
            output,
        })
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

    pub fn read_timeout(&self, timeout: std::time::Duration) -> Result<InlineBytes, TryReadError> {
        self.output.recv_timeout(timeout).map_err(|err| match err {
            flume::RecvTimeoutError::Timeout => TryReadError::Empty,
            flume::RecvTimeoutError::Disconnected => TryReadError::Closed,
        })
    }

    pub fn send(&self, bytes: impl Into<InlineBytes>) {
        let _ = self.input.send(bytes.into());
    }

    fn handle_terminal_input(
        receiver: flume::Receiver<InlineBytes>,
        mut writer: std::io::BufWriter<std::fs::File>,
    ) -> std::io::Result<()> {
        use std::io::Write;

        let into_try_recv_error = |error| match error {
            flume::RecvError::Disconnected => flume::TryRecvError::Disconnected,
        };

        let mut next_item = receiver.recv().map_err(into_try_recv_error);

        loop {
            match next_item {
                Ok(bytes) => {
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
        sender: flume::Sender<InlineBytes>,
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
            sender.send(InlineBytes::new(&bytes)).unwrap();
            waker.wake();
        }

        Ok(())
    }
}

struct PsuedoterminalLink {
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
