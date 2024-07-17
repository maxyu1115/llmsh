use log;
use nix::sys::wait::*;
use nix::unistd::*;
use simplelog::*;
use std::env;
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use tempfile::NamedTempFile;

mod io;
mod messages;
mod pty;
mod shell;

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
    #[cfg(debug_assertions)]
    {
        WriteLogger::init(
            LevelFilter::Debug,
            Config::default(),
            File::create("my_log.log").unwrap(),
        )
        .expect("Logger Initialization failed");
    }

    // TODO: enhance error handling
    let mut shell = shell::get_shell().expect("$SHELL is not set");

    // Open a new PTY parent and get the file descriptor
    let (parent_fd, child_name) = pty::open_pty();

    // Fork the process
    match unsafe { fork().expect("Failed to fork process") } {
        ForkResult::Parent { child } => {
            let client = match messages::HermitdClient::init_client() {
                Ok(client) => client,
                Err(error_msg) => {
                    log::error!("init_client failed with: {}", error_msg);
                    panic!()
                }
            };

            let stdin_fd = std::io::stdin();
            let raw_parent_fd = parent_fd.as_raw_fd();
            let raw_stdin_fd = stdin_fd.as_raw_fd();

            let (mut poll, mut events) = pty::setup_parent_pty(&parent_fd, &stdin_fd);

            // Set terminal to raw mode
            let original_termios = pty::set_raw_mode(&stdin_fd);

            let mut input_buffer: [u8; 4096] = [0; 4096];
            let mut child_exited = false;

            // TODO: clean up error handling
            loop {
                poll.poll(&mut events, None).expect("Failed to poll events");

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
                                                std::io::stdout()
                                                    .write_all(&s)
                                                    .expect("Failed to write to stdout");
                                            }
                                            shell::ParsedOutput::Output {
                                                output_type,
                                                step,
                                                aggregated,
                                            } => {
                                                std::io::stdout()
                                                    .write_all(&step)
                                                    .expect("Failed to write to stdout");
                                                match output_type {
                                                    shell::ShellOutputType::Input => {}
                                                    shell::ShellOutputType::Header => {}
                                                    _ => {
                                                        let context =
                                                            String::from_utf8(aggregated).unwrap();
                                                        match client
                                                            .save_context(output_type, context)
                                                        {
                                                            Ok(_) => {}
                                                            Err(err) => {
                                                                log::error!("Failed to write to hermitd: {}", err);
                                                                break;
                                                            }
                                                        }
                                                    }
                                                };
                                            }
                                        }
                                    }
                                    std::io::stdout().flush().expect("Failed to flush stdout");
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
                                    write(&parent_fd, &input_buffer[..n])
                                        .expect("Failed to write to parent_fd");
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
                        _ => break,
                    }
                }
            }

            // Restore terminal to original state
            pty::restore_terminal(stdin_fd, &original_termios);
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
            let mut temp_rc = NamedTempFile::new().expect("Failed to create NamedTempFile");
            let _ = temp_rc.write_all(&format!("source {}\n", shell.get_rcfile()).into_bytes());
            let _ = writeln!(temp_rc, "source ~/.llmshrc");

            shell.inject_markers(&temp_rc);

            let temp_filename = temp_rc.path().as_os_str().to_str().unwrap();
            let args: [_; 3] = [
                shell_path.clone(),
                CString::new("--rcfile").unwrap(),
                CString::new(temp_filename).unwrap(),
            ];

            // Convert to the right format, then pass into the shell
            execvpe(&shell_path, &args, &env_vars).expect("Failed to execute bash shell");
        }
    }
}
