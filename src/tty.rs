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

                // TODO: ioctl(pty->slave, TIOCSCTTY, NULL)
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
