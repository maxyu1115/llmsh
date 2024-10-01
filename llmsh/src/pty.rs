use anyhow::{Context, Result};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use nix::fcntl::{fcntl, open, FcntlArg, OFlag};
use nix::libc::{ioctl, winsize, TIOCGWINSZ, TIOCSWINSZ};
use nix::pty::*;
use nix::sys::signal::Signal;
use nix::sys::termios::{self, LocalFlags, SetArg, Termios};
use nix::unistd::*;
use procfs::process::Process;
use signal_hook::consts::signal;
use signal_hook::iterator::Signals;
use std::io::Stdin;
use std::os::fd::{AsFd, AsRawFd};
use std::thread;

pub fn open_pty() -> (PtyMaster, String) {
    // Open a new PTY master and get the file descriptor
    let master_fd =
        posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("Failed to open PTY master");

    // Grant access to the slave PTY
    grantpt(&master_fd).expect("Failed to grant PTY access");

    // Unlock the slave PTY
    unlockpt(&master_fd).expect("Failed to unlock PTY");

    // Get the name of the slave PTY
    // FIXME: ptsname_r does not work on windows/mac
    let child_name = ptsname_r(&master_fd).expect("Failed to get slave PTY name");

    return (master_fd, child_name);
}

pub fn setup_child_pty(child_name: String) {
    // Child process: Start a new session and set the slave PTY as the controlling terminal
    setsid().expect("Failed to create new session");

    let child_fd = open(
        child_name.as_str(),
        OFlag::O_RDWR,
        nix::sys::stat::Mode::empty(),
    )
    .expect("Failed to open slave PTY");

    // Set the slave PTY as stdin, stdout, and stderr
    dup2(child_fd, 0).expect("Failed to duplicate slave PTY to stdin");
    dup2(child_fd, 1).expect("Failed to duplicate slave PTY to stdout");
    dup2(child_fd, 2).expect("Failed to duplicate slave PTY to stderr");

    // Close the slave PTY file descriptor
    close(child_fd).expect("Failed to close slave PTY file descriptor");
}

pub const PARENT_TOK: Token = Token(0);
pub const STDIN_TOK: Token = Token(1);

pub fn setup_parent_pty(parent_fd: &PtyMaster, stdin_fd: &Stdin) -> (Poll, Events) {
    // Parent process: Set up non-blocking I/O and polling
    let poll = Poll::new().expect("Failed to create Poll instance");
    let events = Events::with_capacity(1024);

    let raw_parent_fd = parent_fd.as_raw_fd();
    let raw_stdin_fd = stdin_fd.as_raw_fd();

    fcntl(raw_parent_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");
    fcntl(raw_stdin_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("Failed to set non-blocking");

    // Register the parent PTY and stdin file descriptors with the poll instance
    poll.registry()
        .register(
            &mut SourceFd(&raw_parent_fd),
            PARENT_TOK,
            Interest::READABLE,
        )
        .expect("Failed to register parent_fd");
    poll.registry()
        .register(&mut SourceFd(&raw_stdin_fd), STDIN_TOK, Interest::READABLE)
        .expect("Failed to register stdin_fd");

    // Sync the current terminal size
    sync_terminal_size(raw_parent_fd, raw_stdin_fd);

    return (poll, events);
}

fn get_tpgid(pid: i32) -> Result<i32> {
    // Open the process information using the PID
    let process =
        Process::new(pid).with_context(|| "Failed to read proc fs, this only works on linux")?;

    // Retrieve the process status
    let stat = process
        .stat()
        .with_context(|| "Failed to retrieve the process status")?;
    return Ok(stat.tpgid);
}

fn pass_signal(child_pid: i32, sig: Signal) -> Result<()> {
    // NOTE that this assumes the tgpid of the child (shell) process is the pid/pgid of the foreground process.
    let pgid = get_tpgid(child_pid)?;

    // Send SIGINT to the child process
    nix::sys::signal::killpg(Pid::from_raw(pgid), sig)
        .with_context(|| "Failed to send signal to child")?;

    return Ok(());
}

pub fn setup_signal_handlers(
    parent_fd: i32,
    child_fd: i32,
    child_pid: i32,
) -> thread::JoinHandle<()> {
    let mut signals = Signals::new(&[
        signal::SIGINT,
        signal::SIGTSTP,
        signal::SIGQUIT,
        signal::SIGWINCH,
    ])
    .expect("Failed to create signal handler");

    let handler_thread = thread::spawn(move || {
        for sig in signals.forever() {
            log::info!("Received and handling signal {}", sig);
            match sig {
                // Control + C
                signal::SIGINT => {
                    let _ = pass_signal(child_pid, Signal::SIGINT);
                }
                // Control + Z
                signal::SIGTSTP => {
                    let _ = pass_signal(child_pid, Signal::SIGTSTP);
                }
                // Control + \
                signal::SIGQUIT => {
                    let _ = pass_signal(child_pid, Signal::SIGQUIT);
                }
                // terminal size adjustment
                signal::SIGWINCH => {
                    sync_terminal_size(parent_fd, child_fd);
                }
                _ => unreachable!(),
            }
        }
    });
    return handler_thread;
}

pub fn set_raw_mode<Fd: AsFd>(fd: &Fd) -> Termios {
    let original_termios = termios::tcgetattr(fd).expect("Failed to get terminal attributes");
    let mut raw_termios = original_termios.clone();
    // raw_termios.input_flags &= !(InputFlags::ICRNL | InputFlags::IXON | InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP | InputFlags::IXANY);
    // raw_termios.output_flags &= !termios::OutputFlags::OPOST;
    // raw_termios.control_flags |= termios::ControlFlags::CS8;
    // raw_termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG);
    termios::cfmakeraw(&mut raw_termios);

    // Enable the ISIG flag to allow signal generation (e.g., Control-C for SIGINT)
    // We handle and pass in those signals manually, to ensure they aren't effected by io load
    raw_termios.local_flags.insert(LocalFlags::ISIG);

    termios::tcsetattr(fd, SetArg::TCSANOW, &raw_termios)
        .expect("Failed to set terminal to raw mode");
    original_termios
}

pub fn restore_terminal<Fd: AsFd>(fd: Fd, termios: &Termios) {
    termios::tcsetattr(fd, SetArg::TCSANOW, termios)
        .expect("Failed to restore terminal attributes");
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

fn sync_terminal_size(parent_fd: i32, child_fd: i32) {
    // Get the current terminal size
    let ws = get_terminal_size(child_fd);

    // Set the terminal size of the PTY
    set_terminal_size(parent_fd, &ws);
}
