use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use log::debug;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::io;
use crate::io::TransitionCondition::StringID;

#[derive(Copy, Clone)]
pub enum OutputType {
    InProgress,
    Header,
    Input,
    Output,
}

pub trait ShellParser {
    fn get_rcfile(&self) -> String;
    fn inject_markers(&self, temp_rc: &NamedTempFile);
    fn parse_output(&mut self, input: &[u8]) -> Vec<(OutputType, Vec<u8>)>;
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

pub fn get_shell() -> Option<Box<dyn ShellParser>> {
    let shell_pathname: String = env::var("SHELL").expect("$SHELL is not set");
    if let Some(file_name) = PathBuf::from(shell_pathname).file_name() {
        let file_name_str = file_name.to_string_lossy();
        match file_name_str.as_ref() {
            "bash" => return Some(Box::new(Bash::new("bash".to_string()))),
            "zsh" => return Some(Box::new(Bash::new("zsh".to_string()))),
            "csh" => todo!(),
            other => return Some(Box::new(Bash::new(other.to_string()))),
        }
    } else {
        return None;
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
    parser: io::BufferParser<BashState>,
    input_end_marker: String,
    output_end_marker: String,
}

impl Bash {
    fn new(shell_name: String) -> Bash {
        let input_end_marker: String = Uuid::new_v4().to_string() + "\n";
        let output_end_marker: String = Uuid::new_v4().to_string() + "\n";

        debug!("Bash Input End Marker: [{}], Output End Marker: [{}]", input_end_marker, output_end_marker);

        return Bash {
            shell_name: shell_name,
            parser: io::BufferParser::new(
                BashState::Output, // Start with output state, since it instantly transitions to idle
                HashMap::from([
                    (
                        BashState::Idle,
                        vec![(
                            StringID(make_string_id(BASH_PROMPT_INPUT_START), true),
                            BashState::CmdInput,
                        )],
                    ),
                    (
                        BashState::CmdInput,
                        vec![
                            (
                                StringID(make_string_id(&input_end_marker), false),
                                BashState::Output,
                            ),
                            (
                                StringID(make_string_id(&output_end_marker), false),
                                BashState::Idle,
                            ),
                        ],
                    ),
                    (
                        BashState::Output,
                        vec![(
                            StringID(make_string_id(&output_end_marker), false),
                            BashState::Idle,
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

    fn parse_output(&mut self, input: &[u8]) -> Vec<(OutputType, Vec<u8>)> {
        let mut ret = Vec::new();
        self.parser.buffer(input);
        loop {
            match self.parser.step() {
                io::StepResults::Done => break,
                io::StepResults::Echo(out) => {
                    ret.push((OutputType::InProgress, out.to_vec()));
                    break;
                }
                io::StepResults::StateChange {
                    state,
                    step,
                    aggregated,
                } => match state {
                    BashState::Idle => ret.push((OutputType::Header, step)),
                    BashState::CmdInput => ret.push((OutputType::Input, step)),
                    BashState::Output => ret.push((OutputType::Output, step)),
                },
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
