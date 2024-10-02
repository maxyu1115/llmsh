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

use crate::map_err;
use crate::util;

pub fn open_pty() -> Result<(PtyMaster, String), util::Error> {
    // Open a new PTY master and get the file descriptor
    let master_fd = map_err!(
        posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY),
        "Failed to open PTY master"
    )?;

    // Grant access to the slave PTY
    map_err!(grantpt(&master_fd), "Failed to grant PTY access")?;

    // Unlock the slave PTY
    map_err!(unlockpt(&master_fd), "Failed to unlock PTY")?;

    // Get the name of the slave PTY
    // FIXME: ptsname_r does not work on windows/mac
    let child_name = map_err!(ptsname_r(&master_fd), "Failed to get slave PTY name")?;

    return Ok((master_fd, child_name));
}

pub fn setup_child_pty(child_name: String) -> Result<(), util::Error> {
    // Child process: Start a new session and set the slave PTY as the controlling terminal
    map_err!(setsid(), "Failed to create new session")?;

    let child_fd = map_err!(
        open(
            child_name.as_str(),
            OFlag::O_RDWR,
            nix::sys::stat::Mode::empty(),
        ),
        "Failed to open slave PTY"
    )?;

    // Set the slave PTY as stdin, stdout, and stderr
    map_err!(dup2(child_fd, 0), "Failed to duplicate slave PTY to stdin")?;
    map_err!(dup2(child_fd, 1), "Failed to duplicate slave PTY to stdout")?;
    map_err!(dup2(child_fd, 2), "Failed to duplicate slave PTY to stderr")?;

    // Close the slave PTY file descriptor
    map_err!(close(child_fd), "Failed to close slave PTY file descriptor")?;
    Ok(())
}

pub const PARENT_TOK: Token = Token(0);
pub const STDIN_TOK: Token = Token(1);

pub fn setup_parent_pty(
    parent_fd: &PtyMaster,
    stdin_fd: &Stdin,
) -> Result<(Poll, Events), util::Error> {
    // Parent process: Set up non-blocking I/O and polling
    let poll = map_err!(Poll::new(), "Failed to create Poll instance")?;
    let events = Events::with_capacity(1024);

    let raw_parent_fd = parent_fd.as_raw_fd();
    let raw_stdin_fd = stdin_fd.as_raw_fd();

    map_err!(
        fcntl(raw_parent_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
        "Failed to set non-blocking"
    )?;
    map_err!(
        fcntl(raw_stdin_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
        "Failed to set non-blocking"
    )?;

    // Register the parent PTY and stdin file descriptors with the poll instance
    map_err!(
        poll.registry().register(
            &mut SourceFd(&raw_parent_fd),
            PARENT_TOK,
            Interest::READABLE,
        ),
        "Failed to register parent_fd"
    )?;
    map_err!(
        poll.registry()
            .register(&mut SourceFd(&raw_stdin_fd), STDIN_TOK, Interest::READABLE),
        "Failed to register stdin_fd"
    )?;

    // Sync the current terminal size
    sync_terminal_size(raw_parent_fd, raw_stdin_fd);

    return Ok((poll, events));
}

fn get_tpgid(pid: i32) -> Result<i32, util::Error> {
    // Open the process information using the PID
    let process = map_err!(
        Process::new(pid),
        "Failed to read proc fs, this only works on linux"
    )?;

    // Retrieve the process status
    let stat = map_err!(process.stat(), "")?;
    return Ok(stat.tpgid);
}

fn pass_signal(child_pid: i32, sig: Signal) -> Result<(), util::Error> {
    // NOTE that this assumes the tgpid of the child (shell) process is the pid/pgid of the foreground process.
    let pgid = get_tpgid(child_pid)?;

    // Send SIGINT to the child process
    map_err!(
        nix::sys::signal::killpg(Pid::from_raw(pgid), sig),
        "Failed to send signal to child"
    )?;

    return Ok(());
}

pub fn setup_signal_handlers(
    parent_fd: i32,
    child_fd: i32,
    child_pid: i32,
) -> Result<thread::JoinHandle<()>, util::Error> {
    let mut signals = map_err!(
        Signals::new(&[
            signal::SIGINT,
            signal::SIGTSTP,
            signal::SIGQUIT,
            signal::SIGWINCH,
        ]),
        "Failed to create signal handler"
    )?;

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
    return Ok(handler_thread);
}

pub fn set_raw_mode<Fd: AsFd>(fd: &Fd) -> Result<Termios, util::Error> {
    let original_termios = map_err!(termios::tcgetattr(fd), "Failed to get terminal attributes")?;
    let mut raw_termios = original_termios.clone();
    // raw_termios.input_flags &= !(InputFlags::ICRNL | InputFlags::IXON | InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP | InputFlags::IXANY);
    // raw_termios.output_flags &= !termios::OutputFlags::OPOST;
    // raw_termios.control_flags |= termios::ControlFlags::CS8;
    // raw_termios.local_flags &= !(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG);
    termios::cfmakeraw(&mut raw_termios);

    // Enable the ISIG flag to allow signal generation (e.g., Control-C for SIGINT)
    // We handle and pass in those signals manually, to ensure they aren't effected by io load
    raw_termios.local_flags.insert(LocalFlags::ISIG);

    map_err!(
        termios::tcsetattr(fd, SetArg::TCSANOW, &raw_termios),
        "Failed to set terminal to raw mode"
    )?;
    Ok(original_termios)
}

pub fn restore_terminal<Fd: AsFd>(fd: Fd, termios: &Termios) -> Result<(), util::Error> {
    map_err!(
        termios::tcsetattr(fd, SetArg::TCSANOW, termios),
        "Failed to restore terminal attributes"
    )?;
    Ok(())
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
