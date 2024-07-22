use log;
use messages::HermitdClient;
use mio::{Events, Poll};
use nix::pty::PtyMaster;
use nix::sys::wait::*;
use nix::unistd::*;
use shell::ShellParser;
use simplelog::*;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::{Stdin, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use tempfile::NamedTempFile;

mod io;
mod messages;
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
                File::create("my_log.log").unwrap(),
            ),
            "Logger Initialization failed"
        );
    }

    // TODO: enhance error handling
    let shell = expect!(
        shell::get_shell(),
        "Failed to identify shell, bad $SHELL path"
    );

    // Open a new PTY parent and get the file descriptor
    let (parent_fd, child_name) = pty::open_pty();

    // Fork the process
    match unsafe { expect!(fork(), "Failed to fork process") } {
        ForkResult::Parent { child } => {
            let client = expect!(
                HermitdClient::init_client(),
                "hermitd client initialization failed"
            );

            let stdin_fd = std::io::stdin();

            let (poll, events) = pty::setup_parent_pty(&parent_fd, &stdin_fd);

            // Set terminal to raw mode
            let original_termios = pty::set_raw_mode(&stdin_fd);

            let exit_result =
                safe_handle_terminal(shell, client, parent_fd, &stdin_fd, child, poll, events);
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
            pty::restore_terminal(stdin_fd, &original_termios);
            std::process::exit(exit_code);
        }
        ForkResult::Child => {
            // setup the child pty to properly redirect everything to the parent
            pty::setup_child_pty(child_name);

            // TODO: use /bin/sh when no SHELL set
            let shell_path: CString = shell.get_path();

            // Collect the current environment variables
            let env_vars: Vec<CString> = env::vars()
                .map(|(key, value)| CString::new(format!("{}={}", key, value)).unwrap())
                .collect();

            // Create a temporary rc file, so that we use both
            let mut temp_rc = expect!(NamedTempFile::new(), "Failed to create NamedTempFile");
            let _ = temp_rc.write_all(&format!("source {}\n", shell.get_rcfile()).into_bytes());
            let _ = writeln!(temp_rc, "source ~/.llmshrc");

            shell.inject_markers(&temp_rc);

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

// This function should never panic
fn safe_handle_terminal(
    mut shell: Box<dyn ShellParser>,
    client: messages::HermitdClient,
    parent_fd: PtyMaster,
    stdin_fd: &Stdin,
    child: Pid,
    mut poll: Poll,
    mut events: Events,
) -> Result<(), util::Error> {
    let raw_parent_fd = parent_fd.as_raw_fd();
    let raw_stdin_fd = stdin_fd.as_raw_fd();

    let mut input_buffer: [u8; 4096] = [0; 4096];
    let mut child_exited = false;

    loop {
        map_err!(poll.poll(&mut events, None), "Failed to poll events")?;

        for event in events.iter() {
            match event.token() {
                pty::PARENT_TOK => {
                    let n = read(raw_parent_fd, &mut input_buffer);
                    match n {
                        Ok(n) if n > 0 => {
                            log::debug!("{:?}", &input_buffer[..n]);
                            let parsed_outputs = shell.parse_output(&input_buffer[..n]);
                            for out in parsed_outputs {
                                match out {
                                    shell::ParsedOutput::InProgress(s) => {
                                        // Write data from parent PTY to stdout
                                        map_err!(
                                            std::io::stdout().write_all(&s),
                                            "Failed to write to stdout"
                                        )?;
                                    }
                                    shell::ParsedOutput::Output {
                                        output_type,
                                        step,
                                        aggregated,
                                    } => {
                                        map_err!(
                                            std::io::stdout().write_all(&step),
                                            "Failed to write to stdout"
                                        )?;
                                        match output_type {
                                            shell::ShellOutputType::Input => {}
                                            shell::ShellOutputType::Header => {}
                                            _ => {
                                                let context = map_err!(
                                                    String::from_utf8(aggregated),
                                                    "Inputted string is not utf8"
                                                )?;
                                                match client.save_context(output_type, context) {
                                                    Ok(_) => {}
                                                    Err(err) => {
                                                        log::error!(
                                                            "Failed to write to hermitd: {}",
                                                            err
                                                        );
                                                        break;
                                                    }
                                                }
                                            }
                                        };
                                    }
                                }
                            }
                            map_err!(std::io::stdout().flush(), "Failed to flush stdout")?;
                        }
                        Ok(_) => {}
                        Err(nix::errno::Errno::EIO) => {
                            // EIO indicates the child process has exited
                            child_exited = true;
                            break;
                        }
                        Err(e) => panic!("Failed to read from parent_fd: {}", e),
                    }
                }
                pty::STDIN_TOK => {
                    let n = read(raw_stdin_fd, &mut input_buffer);
                    match n {
                        Ok(n) if n > 0 => {
                            // Write data from stdin to parent PTY
                            map_err!(
                                write(&parent_fd, &input_buffer[..n]),
                                "Failed to write to parent_fd"
                            )?;
                        }
                        Ok(_) => {}
                        Err(e) => panic!("Failed to read from stdin: {}", e),
                    }
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
