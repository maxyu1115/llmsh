use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::io;
use crate::io::TransitionCondition::StringID;
use crate::map_err;

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShellOutputType {
    Header,
    Input,
    InputAborted,
    Output,
}

pub enum ParsedOutput {
    // InProgress(&'a [u8]),
    InProgress(Vec<u8>),
    Output {
        output_type: ShellOutputType,
        step: Vec<u8>,
        aggregated: Vec<u8>,
    },
}

pub trait ShellParser {
    fn get_path(&self) -> CString;
    fn get_rcfile(&self) -> String;
    fn inject_markers(&self, temp_rc: &NamedTempFile);
    fn parse_output(&mut self, input: &[u8]) -> Vec<ParsedOutput>;
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

pub fn get_shell() -> Result<Box<dyn ShellParser>, String> {
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
        return Err("the SHELL path terminates in '..'".to_string());
    }
}

fn make_string_id(s: &str) -> String {
    String::from(s.replace("\n", "\r\n"))
}

/*********************************** BASH ***********************************/

const BASH_PROMPT_INPUT_START: &str = "🐚";

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
enum BashState {
    Idle,     // State starts when ##LLMSH-OUT-END##, ends with ##LLMSH-CMD-START##
    CmdInput, // State starts when ##LLMSH-CMD-START##
    Output,   // State starts when ##LLMSH-CMD-END##, State ends when ##LLMSH-OUT-END##
}

struct Bash {
    shell_name: String,
    shell_path: String,
    parser: io::BufferParser<BashState, ShellOutputType>,
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
            parser: io::BufferParser::new(
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
                        vec![(
                            // Recieved proper user cmd
                            StringID(make_string_id(&output_end_marker), false),
                            BashState::Idle,
                            ShellOutputType::Output,
                        )],
                    ),
                ]),
            ),
            input_end_marker: input_end_marker,
            output_end_marker: output_end_marker,
        };
    }
}

impl ShellParser for Bash {
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

    fn parse_output(&mut self, input: &[u8]) -> Vec<ParsedOutput> {
        let mut ret = Vec::new();
        self.parser.buffer(input);
        loop {
            match self.parser.step() {
                io::StepResults::Done => break,
                io::StepResults::Echo(out) => {
                    ret.push(ParsedOutput::InProgress(out.to_vec()));
                    break;
                }
                io::StepResults::StateChange {
                    event,
                    step,
                    aggregated,
                } => {
                    ret.push(ParsedOutput::Output {
                        output_type: event,
                        step,
                        aggregated,
                    });
                }
            }
        }
        return ret;
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
