use nix::fcntl::{open, fcntl, FcntlArg, OFlag};
use nix::pty::*;
use nix::sys::termios::{self, Termios, InputFlags, LocalFlags, SetArg};
use nix::unistd::*;
use nix::sys::wait::*;
use mio::{Events, Interest, Poll, Token};
use mio::unix::SourceFd;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::ffi::CString;

const MASTER: Token = Token(0);
const STDIN: Token = Token(1);

fn set_raw_mode(fd: i32) -> Termios {
    let original_termios = termios::tcgetattr(fd).expect("Failed to get terminal attributes");
    let mut raw_termios = original_termios.clone();
    // Modify input and local flags to disable canonical mode, echoing, signal handling, etc.
    raw_termios.input_flags &= !(InputFlags::ICRNL | InputFlags::IXON);
    raw_termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG | LocalFlags::IEXTEN);
    termios::tcsetattr(fd, SetArg::TCSANOW, &raw_termios).expect("Failed to set terminal to raw mode");
    original_termios
}

fn restore_terminal(fd: i32, termios: &Termios) {
    termios::tcsetattr(fd, SetArg::TCSANOW, termios).expect("Failed to restore terminal attributes");
}

fn main() {
    // Open a new PTY master and get the file descriptor
    let master_fd = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("Failed to open PTY master");

    // Grant access to the slave PTY
    grantpt(&master_fd).expect("Failed to grant PTY access");

    // Unlock the slave PTY
    unlockpt(&master_fd).expect("Failed to unlock PTY");

    // Get the name of the slave PTY
    let slave_name = ptsname_r(&master_fd).expect("Failed to get slave PTY name");

    // Fork the process
    match unsafe { fork().expect("Failed to fork process") } {
        ForkResult::Parent { child } => {
            // Parent process: Set up non-blocking I/O and polling
            let mut poll = Poll::new().expect("Failed to create Poll instance");
            let mut events = Events::with_capacity(1024);

            let master_fd = master_fd.as_raw_fd();
            let stdin_fd = std::io::stdin().as_raw_fd();

            fcntl(master_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");
            fcntl(stdin_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");

            // Register the master PTY and stdin file descriptors with the poll instance
            poll.registry().register(&mut SourceFd(&master_fd), MASTER, Interest::READABLE).expect("Failed to register master_fd");
            poll.registry().register(&mut SourceFd(&stdin_fd), STDIN, Interest::READABLE).expect("Failed to register stdin_fd");

            // Set terminal to raw mode
            let original_termios = set_raw_mode(stdin_fd);

            let mut buffer = [0; 1024];
            let mut child_exited = false;

            loop {
                poll.poll(&mut events, None).expect("Failed to poll events");

                for event in events.iter() {
                    match event.token() {
                        MASTER => {
                            let n = read(master_fd, &mut buffer);
                            match n {
                                Ok(n) if n > 0 => {
                                    // Write data from master PTY to stdout
                                    std::io::stdout().write_all(&buffer[..n]).expect("Failed to write to stdout");
                                    std::io::stdout().flush().expect("Failed to flush stdout");
                                },
                                Ok(_) => {},
                                Err(nix::errno::Errno::EIO) => {
                                    // EIO indicates the child process has exited
                                    child_exited = true;
                                    break;
                                },
                                Err(e) => panic!("Failed to read from master_fd: {}", e),
                            }
                        },
                        STDIN => {
                            let n = read(stdin_fd, &mut buffer);
                            match n {
                                Ok(n) if n > 0 => {
                                    // Write data from stdin to master PTY
                                    write(master_fd, &buffer[..n]).expect("Failed to write to master_fd");
                                },
                                Ok(_) => {},
                                Err(e) => panic!("Failed to read from stdin: {}", e),
                            }
                        },
                        _ => unreachable!(),
                    }
                }

                // Check if child process has exited
                if child_exited {
                    match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {},
                        _ => break,
                    }
                }
            }

            // Restore terminal to original state
            restore_terminal(stdin_fd, &original_termios);
        },
        ForkResult::Child => {
            // Child process: Start a new session and set the slave PTY as the controlling terminal
            setsid().expect("Failed to create new session");

            let slave_fd = open(
                slave_name.as_str(),
                OFlag::O_RDWR,
                nix::sys::stat::Mode::empty()
            ).expect("Failed to open slave PTY");

            // Set the slave PTY as stdin, stdout, and stderr
            dup2(slave_fd, 0).expect("Failed to duplicate slave PTY to stdin");
            dup2(slave_fd, 1).expect("Failed to duplicate slave PTY to stdout");
            dup2(slave_fd, 2).expect("Failed to duplicate slave PTY to stderr");

            // Close the slave PTY file descriptor
            close(slave_fd).expect("Failed to close slave PTY file descriptor");

            // Execute the bash shell
            let shell = CString::new("/bin/bash").expect("Failed to create CString");
            // let shell = CString::new("/bin/sh").expect("Failed to create CString");
            execvp(&shell, &[shell.clone()]).expect("Failed to execute bash shell");
        }
    }
}
