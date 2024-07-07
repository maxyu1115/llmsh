use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

use crate::io;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum OutputType {
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


/*********************************** BASH ***********************************/

const BASH_PROMPT_INPUT_START: &str = "##LLMSH-CMD-START##\n";
const BASH_PROMPT_INPUT_END: &str = "##LLMSH-CMD-END##\n";
const BASH_PROMPT_OUTPUT_END: &str = "##LLMSH-OUT-END##\n";


#[derive(Copy, Clone, PartialEq, Eq, Hash)]
enum BashState {
    Idle, // State starts when ##LLMSH-OUT-END##, ends with ##LLMSH-CMD-START##
    CmdInput, // State starts when ##LLMSH-CMD-START##
    Output, // State starts when ##LLMSH-CMD-END##, State ends when ##LLMSH-OUT-END##
}

struct Bash {
    shell_name: String,
    parser: io::BufferParser<BashState>,
}

impl Bash {
    fn new(shell_name: String) -> Bash {
        return Bash {
            shell_name: shell_name,
            parser: io::BufferParser::new(
                BashState::Output, // Start with output state, since it instantly transitions to idle
            HashMap::from([
                    (BashState::Idle, vec![
                        (String::from(BASH_PROMPT_INPUT_START.replace("\n", "\r\n")), BashState::CmdInput),
                    ]),
                    (BashState::CmdInput, vec![
                        (String::from(BASH_PROMPT_INPUT_END.replace("\n", "\r\n")), BashState::Output),
                        (String::from(BASH_PROMPT_OUTPUT_END.replace("\n", "\r\n")), BashState::Idle),
                    ]),
                    (BashState::Output, vec![
                        (String::from(BASH_PROMPT_OUTPUT_END.replace("\n", "\r\n")), BashState::Idle),
                    ]),
                ])
            ),
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
            &format!("export PS0=\"{}{}\"\n", String::from(BASH_PROMPT_INPUT_END), &orig_ps0).into_bytes()
        );
        let _ = temp_rc.write_all(
            &format!("export PS1=\"{}{}{}\"\n", String::from(BASH_PROMPT_OUTPUT_END), &orig_ps1, String::from(BASH_PROMPT_INPUT_START)).into_bytes()
        );
    }

    fn parse_output(&mut self, input: &[u8]) -> Vec<(OutputType, Vec<u8>)> {
        let mut ret = Vec::new();
        self.parser.buffer(input);
        loop {
            match self.parser.step() {
                None => break,
                Some((s, out)) => match s {
                    BashState::Idle => ret.push((OutputType::Header, out)),
                    BashState::CmdInput => ret.push((OutputType::Input, out)),
                    BashState::Output => ret.push((OutputType::Output, out)),
                }
            }
        }
        return ret;
    }
}
