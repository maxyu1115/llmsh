use log;
use simplelog::*;
use nix::fcntl::{open, fcntl, FcntlArg, OFlag};
use nix::libc::{ioctl, TIOCSWINSZ, TIOCGWINSZ, winsize};
use nix::pty::*;
use nix::sys::termios::{self, SetArg, Termios};
use nix::unistd::*;
use nix::sys::wait::*;
use mio::{Events, Interest, Poll, Token};
use mio::unix::SourceFd;
use std::env;
use std::fs::{OpenOptions, File};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::os::fd::AsFd;
use std::path::Path;
use std::ffi::CString;
use tempfile::NamedTempFile;

mod io;
mod messages;
mod shell;

const MASTER: Token = Token(0);
const STDIN: Token = Token(1);


fn set_raw_mode<Fd: AsFd>(fd: &Fd) -> Termios {
    let original_termios = termios::tcgetattr(fd).expect("Failed to get terminal attributes");
    let mut raw_termios = original_termios.clone();
    // raw_termios.input_flags &= !(InputFlags::ICRNL | InputFlags::IXON | InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP | InputFlags::IXANY);
    // raw_termios.output_flags &= !termios::OutputFlags::OPOST;
    // raw_termios.control_flags |= termios::ControlFlags::CS8;
    // raw_termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG);
    termios::cfmakeraw(&mut raw_termios);
    termios::tcsetattr(fd, SetArg::TCSANOW, &raw_termios).expect("Failed to set terminal to raw mode");
    original_termios
}

fn restore_terminal<Fd: AsFd>(fd: Fd, termios: &Termios) {
    termios::tcsetattr(fd, SetArg::TCSANOW, termios).expect("Failed to restore terminal attributes");
}

// Function to get the terminal window size
fn get_terminal_size(fd: i32) -> winsize {
    let mut ws: winsize = winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        if ioctl(fd, TIOCGWINSZ, &mut ws) == -1 {
            panic!("Failed to get terminal window size");
        }
    }
    return ws;
}

// Function to set the terminal window size
fn set_terminal_size(fd: i32, ws: &winsize) {
    unsafe {
        if ioctl(fd, TIOCSWINSZ, ws) == -1 {
            panic!("Failed to set terminal window size");
        }
    }
}

fn touch(path: &Path) -> std::io::Result<()> {
    match OpenOptions::new().create(true).write(true).open(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

fn main() {
    let home_dir = env::var("HOME").expect("Could not get home directory");
    touch(&Path::new(&home_dir).join(".llmshrc")).expect("Failed to touch ~/.llmshrc");
    // Initialize the logger
    WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
        File::create("my_log.log").unwrap()
    ).expect("Logger Initialization failed");

    // TODO: enhance error handling
    let mut shell = shell::get_shell().expect("$SHELL is not set");

    // Open a new PTY master and get the file descriptor
    let master_fd = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("Failed to open PTY master");

    // Grant access to the slave PTY
    grantpt(&master_fd).expect("Failed to grant PTY access");

    // Unlock the slave PTY
    unlockpt(&master_fd).expect("Failed to unlock PTY");

    // Get the name of the slave PTY
    // FIXME: ptsname_r does not work on windows/mac
    let slave_name = ptsname_r(&master_fd).expect("Failed to get slave PTY name");

    // Fork the process
    match unsafe { fork().expect("Failed to fork process") } {
        ForkResult::Parent { child } => {
            // Parent process: Set up non-blocking I/O and polling
            let mut poll = Poll::new().expect("Failed to create Poll instance");
            let mut events = Events::with_capacity(1024);

            let raw_master_fd = master_fd.as_raw_fd();
            let stdin_fd = std::io::stdin();
            let raw_stdin_fd = stdin_fd.as_raw_fd();

            fcntl(raw_master_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");
            fcntl(raw_stdin_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");

            // Register the master PTY and stdin file descriptors with the poll instance
            poll.registry().register(&mut SourceFd(&raw_master_fd), MASTER, Interest::READABLE).expect("Failed to register master_fd");
            poll.registry().register(&mut SourceFd(&raw_stdin_fd), STDIN, Interest::READABLE).expect("Failed to register stdin_fd");

            // Set terminal to raw mode
            let original_termios = set_raw_mode(&stdin_fd);

            // Get the current terminal size
            let ws = get_terminal_size(raw_stdin_fd);

            // Set the terminal size of the PTY
            set_terminal_size(raw_master_fd, &ws);

            let mut input_buffer: [u8; 4096] = [0; 4096];
            let mut child_exited = false;

            loop {
                poll.poll(&mut events, None).expect("Failed to poll events");

                for event in events.iter() {
                    match event.token() {
                        MASTER => {
                            let n = read(raw_master_fd, &mut input_buffer);
                            match n {
                                Ok(n) if n > 0 => {
                                    log::debug!("{:?}", &input_buffer[..n]);
                                    // std::io::stdout().write_all(&input_buffer[..n]).expect("Failed to write to stdout");
                                    let parsed_output = shell.parse_output(&input_buffer[..n]);
                                    for (_, out) in parsed_output {
                                        // Write data from master PTY to stdout
                                        std::io::stdout().write_all(&out).expect("Failed to write to stdout");
                                    }
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
                            let n = read(raw_stdin_fd, &mut input_buffer);
                            match n {
                                Ok(n) if n > 0 => {
                                    // Write data from stdin to master PTY
                                    write(&master_fd, &input_buffer[..n]).expect("Failed to write to master_fd");
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

            // TODO: use /bin/sh when no SHELL set
            let shell_path: String = env::var("SHELL").expect("$SHELL is not set");
            let shell_path: CString = CString::new(shell_path).unwrap();

            // Collect the current environment variables
            let env_vars: Vec<CString> = env::vars().map(
                |(key, value)| {
                    CString::new(format!("{}={}", key, value)).unwrap()
                }
            ).collect();

            // Create a temporary rc file, so that we use both 
            let mut temp_rc = NamedTempFile::new().expect("Failed to create NamedTempFile");
            let _ = temp_rc.write_all(&format!("source {}\n",shell.get_rcfile()).into_bytes());
            let _ = writeln!(temp_rc, "source ~/.llmshrc");

            shell.inject_markers(&temp_rc);

            let temp_filename = temp_rc.path().as_os_str().to_str().unwrap();
            let args: [_; 3] = [
                shell_path.clone(), 
                CString::new("--rcfile").unwrap(), 
                CString::new(temp_filename).unwrap()
            ];

            // Convert to the right format, then pass into the shell
            execvpe(&shell_path, &args, &env_vars).expect("Failed to execute bash shell");
        }
    }
}
