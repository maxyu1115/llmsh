use log::debug;
use nix::pty;
use nix::unistd;
use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::io::{Read, Stdin, Stdout, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::map_err;
use crate::messages::{HermitdClient, ShellOutputType};
use crate::parsing;
use crate::parsing::TransitionCondition::StringID;
use crate::util;

pub enum ParsedOutput {
    // InProgress(&'a [u8]),
    InProgress(Vec<u8>),
    Output {
        output_type: ShellOutputType,
        step: Vec<u8>,
        aggregated: Vec<u8>,
    },
}

pub trait ShellCreator {
    fn get_path(&self) -> CString;
    fn get_rcfile(&self) -> String;
    fn inject_markers(&self, temp_rc: &NamedTempFile);
    fn create_proxy(
        &self,
        hermit_client: HermitdClient,
        parent_fd: pty::PtyMaster,
        stdin_fd: Stdin,
        stdout_fd: Stdout,
    ) -> ShellProxy;
}

trait ShellOutputParser {
    fn parse_output(&mut self, input: &[u8]) -> Vec<ParsedOutput>;
}

pub struct ShellProxy {
    hermit_client: HermitdClient,
    parent_fd: pty::PtyMaster,
    stdin_fd: Stdin,
    stdout_fd: Stdout,
    io_buffer: [u8; 4096],
    output_parser: Box<dyn ShellOutputParser>,
}

impl ShellProxy {
    pub fn handle_input(&mut self) -> Result<(), util::Error> {
        let n = self.stdin_fd.read(&mut self.io_buffer);
        match n {
            Ok(n) if n > 0 => {
                // Write data from stdin to parent PTY
                map_err!(
                    self.parent_fd.write_all(&self.io_buffer[..n]),
                    "Failed to write to parent_fd"
                )?;
            }
            Ok(_) => {}
            Err(e) => return map_err!(Err(e), "Failed to read from stdin"),
        }
        Ok(())
    }
    pub fn handle_output(&mut self) -> Result<bool, util::Error> {
        // use unistd::read instead, in order to have nix::errno::Errno::EIO
        let n = unistd::read(self.parent_fd.as_raw_fd(), &mut self.io_buffer);
        match n {
            Ok(n) if n > 0 => {
                log::debug!("Input: {:?}", &self.io_buffer[..n]);
                let parsed_outputs = self.output_parser.parse_output(&self.io_buffer[..n]);
                for out in parsed_outputs {
                    match out {
                        ParsedOutput::InProgress(s) => {
                            // Write data from parent PTY to stdout
                            map_err!(self.stdout_fd.write_all(&s), "Failed to write to stdout")?;
                        }
                        ParsedOutput::Output {
                            output_type,
                            step,
                            aggregated,
                        } => {
                            map_err!(self.stdout_fd.write_all(&step), "Failed to write to stdout")?;
                            match output_type {
                                // shell::ShellOutputType::InputAborted => {}
                                // shell::ShellOutputType::Header => {}
                                _ => {
                                    let context = map_err!(
                                        String::from_utf8(aggregated),
                                        "Shell output string is not utf8"
                                    )?;
                                    log::debug!("Saving context, raw output: {}", context);
                                    let context = parsing::strip_ansi_escape_sequences(&context);
                                    match self
                                        .hermit_client
                                        .save_context(output_type, context.to_string())
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
                map_err!(self.stdout_fd.flush(), "Failed to flush stdout")?;
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EIO) => {
                // EIO indicates the child process has exited
                return Ok(false);
            }
            Err(e) => return map_err!(Err(e), "Failed to read from parent_fd"),
        }
        return Ok(true);
    }
}

// TODO: symlink handling, especially for /bin/sh?
// fn resolve_symlink(path: PathBuf) -> std::io::Result<PathBuf> {
//     let mut current_path = path;
//     loop {
//         if current_path.is_symlink() {
//             current_path = read_link(&current_path)?;
//         } else {
//             break;
//         }
//     }
//     Ok(current_path)
// }

pub fn get_shell() -> Result<Box<dyn ShellCreator>, util::Error> {
    let shell_pathname: String = map_err!(env::var("SHELL"), "$SHELL is not set")?;
    if let Some(file_name) = PathBuf::from(&shell_pathname).file_name() {
        let file_name_str = file_name.to_string_lossy();
        match file_name_str.as_ref() {
            "bash" => return Ok(Box::new(Bash::new(shell_pathname, "bash".to_string()))),
            "zsh" => return Ok(Box::new(Bash::new(shell_pathname, "zsh".to_string()))),
            "csh" => todo!(),
            other => return Ok(Box::new(Bash::new(shell_pathname, other.to_string()))),
        }
    } else {
        return Err(util::Error::Failed(
            "the SHELL path terminates in '..'".to_string(),
        ));
    }
}

fn make_string_id(s: &str) -> String {
    String::from(s.replace("\n", "\r\n"))
}

/*********************************** BASH ***********************************/

const BASH_PROMPT_INPUT_START: &str = "🐚";

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum BashState {
    Idle,     // State starts when ##LLMSH-OUT-END##, ends with ##LLMSH-CMD-START##
    CmdInput, // State starts when ##LLMSH-CMD-START##
    Output,   // State starts when ##LLMSH-CMD-END##, State ends when ##LLMSH-OUT-END##
}

struct Bash {
    shell_name: String,
    shell_path: String,
    input_end_marker: String,
    output_end_marker: String,
}

impl Bash {
    fn new(shell_pathname: String, shell_name: String) -> Bash {
        let input_end_marker: String = Uuid::new_v4().to_string() + "\n";
        let output_end_marker: String = Uuid::new_v4().to_string() + "\n";

        debug!(
            "Bash Input End Marker: [{}], Output End Marker: [{}]",
            input_end_marker, output_end_marker
        );

        return Bash {
            shell_name: shell_name,
            shell_path: shell_pathname,
            input_end_marker: input_end_marker,
            output_end_marker: output_end_marker,
        };
    }
}

impl ShellCreator for Bash {
    fn get_path(&self) -> CString {
        return CString::new(self.shell_path.clone()).unwrap();
    }

    fn get_rcfile(&self) -> String {
        return format!("~/.{}rc", self.shell_name);
    }

    fn inject_markers(&self, mut temp_rc: &NamedTempFile) {
        // Inject our prompt markers
        let orig_ps0 = env::var("PS0").unwrap_or_else(|_| String::from(""));
        let orig_ps1 = env::var("PS1").unwrap_or_else(|_| String::from(""));
        let _ = temp_rc.write_all(
            &format!("export PS0=\"{}{}\"\n", self.input_end_marker, &orig_ps0).into_bytes(),
        );

        // If current ps1 uses $ as the ending, replace with our crab identifier
        if let Some(_dollar_idx) = orig_ps1.rfind("\\$") {
            let new_ps1 = replace_last(&orig_ps1, "\\$", BASH_PROMPT_INPUT_START);

            let _ = temp_rc.write_all(
                &format!("export PS1=\"{}{}\"\n", self.output_end_marker, new_ps1).into_bytes(),
            );
        } else {
            let _ = temp_rc.write_all(
                &format!(
                    "export PS1=\"{}{}{}\"\n",
                    self.output_end_marker,
                    &orig_ps1,
                    String::from(BASH_PROMPT_INPUT_START)
                )
                .into_bytes(),
            );
        }
    }

    fn create_proxy(
        &self,
        hermit_client: HermitdClient,
        parent_fd: pty::PtyMaster,
        stdin_fd: Stdin,
        stdout_fd: Stdout,
    ) -> ShellProxy {
        let output_parser = Box::new(BashParser::new(
            &self.input_end_marker,
            &self.output_end_marker,
        ));
        ShellProxy {
            hermit_client,
            parent_fd,
            stdin_fd,
            stdout_fd,
            io_buffer: [0; 4096],
            output_parser,
        }
    }
}
struct BashParser {
    parser: parsing::BufferParser<BashState, ShellOutputType>,
}

impl BashParser {
    fn new(input_end_marker: &str, output_end_marker: &str) -> BashParser {
        BashParser {
            parser: parsing::BufferParser::new(
                BashState::Output, // Start with output state, since it instantly transitions to idle
                HashMap::from([
                    (
                        BashState::Idle,
                        vec![(
                            // transition from end of output to pending new input
                            StringID(make_string_id(BASH_PROMPT_INPUT_START), true),
                            BashState::CmdInput,
                            ShellOutputType::Header,
                        )],
                    ),
                    (
                        BashState::CmdInput,
                        vec![
                            (
                                // Recieved proper user cmd
                                StringID(make_string_id(&input_end_marker), false),
                                BashState::Output,
                                ShellOutputType::Input,
                            ),
                            (
                                // User aborted cmd input (control+c or empty enter)
                                StringID(make_string_id(&output_end_marker), false),
                                BashState::Idle,
                                ShellOutputType::InputAborted,
                            ),
                        ],
                    ),
                    (
                        BashState::Output,
                        vec![
                            (
                                // User inputed multiple commands at once, in which case there 
                                // will be multiple output blocks separated by input end markers
                                StringID(make_string_id(&input_end_marker), false),
                                BashState::Output,
                                ShellOutputType::Output,
                            ),
                            (
                                // Recieved proper user cmd
                                StringID(make_string_id(&output_end_marker), false),
                                BashState::Idle,
                                ShellOutputType::Output,
                            ),
                        ],
                    ),
                ]),
            ),
        }
    }
}

impl ShellOutputParser for BashParser {
    fn parse_output(&mut self, output: &[u8]) -> Vec<ParsedOutput> {
        let results = self.parser.parse(output);
        return results
            .into_iter()
            .map(|ret| match ret {
                parsing::StepResults::Echo(out) => ParsedOutput::InProgress(out),
                parsing::StepResults::StateChange {
                    event,
                    step,
                    aggregated,
                } => ParsedOutput::Output {
                    output_type: event,
                    step,
                    aggregated,
                },
            })
            .collect();
    }
}

fn replace_last(haystack: &str, needle: &str, replacement: &str) -> String {
    if let Some(pos) = haystack.rfind(needle) {
        let mut result = String::with_capacity(haystack.len() - needle.len() + replacement.len());
        result.push_str(&haystack[..pos]);
        result.push_str(replacement);
        result.push_str(&haystack[pos + needle.len()..]);
        result
    } else {
        haystack.to_string() // If the needle is not found, return the original string
    }
}
