use log;
use messages::HermitdClient;
use mio::{Events, Poll};
use nix::sys::wait::*;
use nix::unistd::*;
use simplelog::*;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::NamedTempFile;

mod messages;
mod parsing;
mod pty;
mod shell;
mod util;

fn main() {
    let home_dir = expect!(env::var("HOME"), "Could not get home directory");
    expect!(
        util::touch(&Path::new(&home_dir).join(".llmshrc")),
        "Failed to touch ~/.llmshrc"
    );

    // Initialize the logger
    #[cfg(debug_assertions)]
    {
        let _ = expect!(
            WriteLogger::init(
                LevelFilter::Debug,
                Config::default(),
                File::create("llmsh_debug.log").unwrap(),
            ),
            "Logger Initialization failed"
        );
    }

    // TODO: enhance error handling
    let shell_creator = expect!(
        shell::get_shell(),
        "Failed to identify shell, bad $SHELL path"
    );

    // Open a new PTY parent and get the file descriptor
    let (parent_fd, child_name) = pty::open_pty();

    // Fork the process
    match unsafe { expect!(fork(), "Failed to fork process") } {
        ForkResult::Parent { child } => {
            let mut stdout_fd = std::io::stdout();
            let stdin_fd = std::io::stdin();

            let (client, motd) = expect!(
                HermitdClient::init_client(),
                "hermitd client initialization failed"
            );
            expect!(
                shell::hermit_print(&mut stdout_fd, &motd),
                "Failed to print motd"
            );

            let (poll, events) = pty::setup_parent_pty(&parent_fd, &stdin_fd);

            // Set terminal to raw mode
            let original_termios = pty::set_raw_mode(&stdin_fd);

            let shell_proxy = shell_creator.create_proxy(client, parent_fd, stdin_fd, stdout_fd);

            let exit_result = safe_handle_terminal(shell_proxy, child, poll, events);
            let mut exit_code = 0;
            match exit_result {
                Ok(_) => {
                    log::info!("Parent process natural exiting");
                }
                Err(err_msg) => {
                    print!("Exiting due to:\r\n  {}\r\n", err_msg);
                    exit_code = 1;
                }
            }

            // Restore terminal to original state
            pty::restore_terminal(std::io::stdin(), &original_termios);
            std::process::exit(exit_code);
        }
        ForkResult::Child => {
            // setup the child pty to properly redirect everything to the parent
            pty::setup_child_pty(child_name);

            // TODO: use /bin/sh when no SHELL set
            let shell_path: CString = shell_creator.get_path();

            // Collect the current environment variables
            let env_vars: Vec<CString> = env::vars()
                .map(|(key, value)| CString::new(format!("{}={}", key, value)).unwrap())
                .collect();

            // Create a temporary rc file, so that we use both
            let mut temp_rc = expect!(NamedTempFile::new(), "Failed to create NamedTempFile");
            let _ =
                temp_rc.write_all(&format!("source {}\n", shell_creator.get_rcfile()).into_bytes());
            let _ = writeln!(temp_rc, "source ~/.llmshrc");

            shell_creator.inject_markers(&temp_rc);

            // Set the temporary rc file to user read write only
            let rc_metadata = std::fs::metadata(temp_rc.path()).unwrap();
            let mut rc_permission = rc_metadata.permissions();
            rc_permission.set_mode(0o600);

            let temp_filename = temp_rc.path().as_os_str().to_str().unwrap();
            let args: [_; 3] = [
                shell_path.clone(),
                CString::new("--rcfile").unwrap(),
                CString::new(temp_filename).unwrap(),
            ];

            // Convert to the right format, then pass into the shell
            expect!(
                execvpe(&shell_path, &args, &env_vars),
                "Failed to execute shell"
            );
        }
    }
}

const MAX_EINTR_RETRY: u32 = 2;

// This function should never panic
fn safe_handle_terminal(
    mut shell_proxy: shell::ShellProxy,
    child: Pid,
    mut poll: Poll,
    mut events: Events,
) -> Result<(), util::Error> {
    let mut child_exited = false;
    let mut retry_counter = 0;

    loop {
        match poll.poll(&mut events, None) {
            Ok(()) => {
                retry_counter = 0;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                // we seem to be only seeing this error after integrating reedline...
                // TODO: properly debug this issue
                if retry_counter < MAX_EINTR_RETRY {
                    log::warn!(
                        "Failed to poll event due to io::ErrorKind::Interrupted, retry counter {}",
                        retry_counter
                    );
                    retry_counter += 1;
                    continue;
                } else {
                    return map_err!(
                        Err(e),
                        "Repeatedly failed to poll events due to io::ErrorKind::Interrupted"
                    );
                }
            }
            Err(e) => {
                return map_err!(Err(e), "Failed to poll events");
            }
        };

        for event in events.iter() {
            match event.token() {
                pty::PARENT_TOK => {
                    child_exited = shell_proxy.handle_output()?;
                    if child_exited {
                        break;
                    }
                }
                pty::STDIN_TOK => {
                    shell_proxy.handle_input()?;
                }
                _ => unreachable!(),
            }
        }

        // Check if child process has exited
        if child_exited {
            match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) => {}
                _ => return Ok(()),
            };
        }
    }
}
