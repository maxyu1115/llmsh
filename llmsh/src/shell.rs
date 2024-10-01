use anyhow::{anyhow, Context, Result};
use core::panic;
use nix::pty;
use nix::unistd;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::io::{Read, Stdin, Stdout, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::messages::{HermitdClient, ShellOutputType};
use crate::parsing;
use crate::parsing::TransitionCondition::StringID;
use crate::util;
use crate::{illegal_state, map_err};

const SHELL_PROMPT_INPUT_START: &str = "🐚";

// Other candidates: 🌊,📶,📨,📡,🎤
const HERMITD_PROMPT_HEADER: &str = "🌊";
const HERMITD_RESP_HEADER: &str = "🦀〉";

pub enum ParsedOutput {
    InProgress {
        step: Vec<u8>,
        aggregate_locally: bool,
    },
    Output {
        output_type: ShellOutputType,
        step: Vec<u8>,
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

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum ShellInputState {
    Undetermined, // Start of the input when it can be both
    HermitInput,  // State for interacting with hermitd, not managed through this state machine
    ShellPrompt,
    Idle, // when not in the input flow
}

pub struct ShellProxy {
    hermit_client: HermitdClient,
    parent_fd: pty::PtyMaster,
    stdin_fd: Stdin,
    stdout_fd: Stdout,
    io_buffer: [u8; 4096],
    input_parser: ShellInputStateMachine,
    output_parser: Box<dyn ShellOutputParser>,
    output_aggregation: Vec<u8>,
    input_rl: Reedline,
    rl_prompt: DefaultPrompt,
}

struct ShellInputStateMachine {
    state: ShellInputState,
}

enum ShellInputTarget {
    #[allow(dead_code)]
    // For input echoing since we aren't actually writing to the child shell
    Stdout,
    Pty,
    // For communication with hermitd
    Hermit,
    // Marker so we just pass through the input without extra copies
    PassThrough,
}

type ShellInputActions = Vec<(ShellInputTarget, Vec<u8>)>;

impl ShellInputStateMachine {
    fn new() -> ShellInputStateMachine {
        ShellInputStateMachine {
            state: ShellInputState::Idle,
        }
    }

    fn activate(&mut self) -> Result<(), util::Error> {
        if self.state != ShellInputState::Idle {
            illegal_state!(format!(
                "Tried calling activate in state {:?} instead",
                self.state
            ));
        }
        self.state = ShellInputState::Undetermined;
        Ok(())
    }

    fn finish_shell_prompt(&mut self) -> Result<(), util::Error> {
        if self.state != ShellInputState::ShellPrompt && self.state != ShellInputState::Undetermined
        {
            illegal_state!(format!(
                "Tried calling finish_shell_prompt in state {:?} instead",
                self.state
            ));
        }
        self.state = ShellInputState::Idle;
        Ok(())
    }

    fn finish_hermit_inputs(&mut self) -> Result<(), util::Error> {
        if self.state != ShellInputState::HermitInput {
            illegal_state!(format!(
                "Tried calling finish_hermit_inputs in state {:?} instead",
                self.state
            ));
        }
        self.state = ShellInputState::ShellPrompt;
        Ok(())
    }

    fn _handle_shell_prompt(&self, input: &[u8]) -> ShellInputActions {
        return vec![(ShellInputTarget::Pty, input.to_vec())];
    }

    fn _handle_undetermined(&mut self, input: &[u8]) -> ShellInputActions {
        if input.contains(&b':') {
            self.state = ShellInputState::HermitInput;
            return vec![(ShellInputTarget::Hermit, Vec::new())];
        } else {
            self.state = ShellInputState::ShellPrompt;
            return self._handle_shell_prompt(input);
        }
    }

    fn handle_input(&mut self, input: &[u8]) -> ShellInputActions {
        match self.state {
            ShellInputState::Undetermined => self._handle_undetermined(input),
            ShellInputState::HermitInput => panic!(),
            ShellInputState::ShellPrompt => self._handle_shell_prompt(input),
            ShellInputState::Idle => vec![(ShellInputTarget::PassThrough, Vec::new())],
        }
    }
}

pub fn hermit_print(stdout_fd: &mut Stdout, message: &str) -> Result<()> {
    let wrapped_message = format!("{}{}\n", HERMITD_RESP_HEADER, message).into_bytes();

    stdout_fd
        .write_all(&util::fix_newlines(wrapped_message))
        .with_context(|| "Failed to write to stdout")?;
    stdout_fd
        .flush()
        .with_context(|| "Failed to flush stdout")?;
    Ok(())
}

impl ShellProxy {
    fn _hermit_print(&mut self, message: &str) -> Result<()> {
        return hermit_print(&mut self.stdout_fd, message);
    }

    fn _hermit_map_err<T, E>(&mut self, res: Result<T, E>, msg: &str) -> Option<T> {
        match res {
            Ok(value) => {
                return Some(value);
            }
            Err(_) => {
                let _ = self._hermit_print(msg);
                return None;
            }
        }
    }

    fn _handle_hermit_prompt(&mut self) -> Result<Option<String>> {
        let result = self.input_rl.read_line(&self.rl_prompt);
        let signal = result.with_context(|| "Unknown error during handling hermitd prompt")?;
        match signal {
            Signal::Success(input) => {
                return Ok(Some(input));
            }
            Signal::CtrlD | Signal::CtrlC => return Ok(None),
        }
    }

    fn _handle_hermit_selection(&mut self, commands: &Vec<String>) -> Result<Option<String>> {
        if commands.len() == 0 {
            return Ok(None);
        }
        let command_choices_message: String = commands
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[{}] `{}`", i, s))
            .collect::<Vec<String>>()
            .join("\n");
        self._hermit_print(&format!(
            "To run a suggested command from hermitd, type one of the following: \n{}",
            command_choices_message
        ))?;
        let result = self.input_rl.read_line(&self.rl_prompt);
        let signal = result.with_context(|| "Unknown error during handling hermitd prompt")?;
        match signal {
            Signal::Success(input) => {
                let trimmed_input = input.trim();
                let mut selection: usize = 0;
                if trimmed_input != "" {
                    let selection_raw: Result<usize, _> = trimmed_input.parse();
                    selection = match selection_raw {
                        Ok(value) => value,
                        Err(_) => {
                            self._hermit_print("Please input a valid selection")?;
                            return Ok(None);
                        }
                    };
                }
                if selection >= commands.len() {
                    self._hermit_print("Please input a valid selection")?;
                    return Ok(None);
                }
                return Ok(Some(commands[selection].clone()));
            }
            Signal::CtrlD | Signal::CtrlC => return Ok(None),
        }
    }

    fn handle_hermit(&mut self) -> Result<String> {
        let prompt_option = self._handle_hermit_prompt()?;
        if prompt_option.is_none() {
            // input a new line so that we get a new shell line (and header)
            return Ok("\r".to_string());
        }
        let prompt = prompt_option.unwrap();

        let (response, commands) = self.hermit_client.generate_command(prompt)?;
        self._hermit_print(&response)?;

        let selection_option = self._handle_hermit_selection(&commands)?;
        if selection_option.is_none() {
            return Ok("\r".to_string());
        }
        let selection = selection_option.unwrap();
        return Ok(format!("\r{}\r", selection));
    }

    fn handle_input_actions(&mut self, actions: ShellInputActions, n: usize) -> Result<()> {
        for (target, input) in actions {
            match target {
                ShellInputTarget::Stdout => {
                    log::debug!("Attempting to write to stdout: {:?}", input);
                    let mapped_input: Vec<u8> = util::fix_newlines(input);
                    self.stdout_fd
                        .write_all(&mapped_input)
                        .with_context(|| "Failed to write to stdout")?;
                    self.stdout_fd
                        .flush()
                        .with_context(|| "Failed to flush stdout")?;
                }
                ShellInputTarget::Hermit => {
                    let hermit_pty_input = self.handle_hermit()?;
                    self.input_parser.finish_hermit_inputs()?;
                    self.parent_fd
                        .write_all(hermit_pty_input.as_bytes())
                        .with_context(|| "Failed to write to parent_fd")?;
                }
                ShellInputTarget::Pty => {
                    self.parent_fd
                        .write_all(&input)
                        .with_context(|| "Failed to write to parent_fd")?;
                }
                ShellInputTarget::PassThrough => {
                    self.parent_fd
                        .write_all(&self.io_buffer[..n])
                        .with_context(|| "Failed to write to parent_fd")?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_input(&mut self) -> Result<()> {
        let n = self.stdin_fd.read(&mut self.io_buffer);
        match n {
            Ok(n) if n > 0 => {
                let actions = self.input_parser.handle_input(&self.io_buffer[..n]);
                return self.handle_input_actions(actions, n);
            }
            Ok(_) => {
                log::debug!("Nothing to read");
            }
            Err(e) => return Err(e).with_context(|| "Failed to read from stdin"),
        }
        Ok(())
    }

    pub fn handle_output(&mut self) -> Result<bool> {
        // use unistd::read instead, in order to have nix::errno::Errno::EIO
        let n = unistd::read(self.parent_fd.as_raw_fd(), &mut self.io_buffer);
        match n {
            Ok(n) if n > 0 => {
                log::debug!("Input: {:?}", &self.io_buffer[..n]);
                let parsed_outputs = self.output_parser.parse_output(&self.io_buffer[..n]);
                for out in parsed_outputs {
                    match out {
                        ParsedOutput::InProgress {
                            step,
                            aggregate_locally,
                        } => {
                            // Write data from parent PTY to stdout
                            self.stdout_fd
                                .write_all(&step)
                                .with_context(|| "Failed to write to stdout")?;
                            if aggregate_locally {
                                self.output_aggregation.extend(step);
                            } else {
                                let context = String::from_utf8_lossy(&step).to_string();
                                log::debug!("Saving context, raw output: {}", context);
                                let context = parsing::strip_ansi_escape_sequences(&context);
                                self.hermit_client.save_context(None, context.to_string())?;
                            }
                        }
                        ParsedOutput::Output { output_type, step } => {
                            map_err!(self.stdout_fd.write_all(&step), "Failed to write to stdout")?;
                            match output_type {
                                ShellOutputType::Header => {
                                    self.input_parser.activate()?;
                                }
                                ShellOutputType::Input | ShellOutputType::InputAborted => {
                                    self.input_parser.finish_shell_prompt()?;
                                }
                                _ => {}
                            }
                            let mut ctx_str: &[u8] = &step;
                            if !self.output_aggregation.is_empty() {
                                self.output_aggregation.extend(step);
                                ctx_str = &self.output_aggregation;
                            }
                            let context = String::from_utf8_lossy(&ctx_str).to_string();
                            if !self.output_aggregation.is_empty() {
                                self.output_aggregation.clear();
                            }
                            log::debug!("Saving context, raw output: {}", context);
                            let context = parsing::strip_ansi_escape_sequences(&context);
                            self.hermit_client
                                .save_context(Some(output_type), context.to_string())?;
                        }
                    }
                }
                self.stdout_fd
                    .flush()
                    .with_context(|| "Failed to flush stdout")?;
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EIO) => {
                // EIO indicates the child process has exited
                return Ok(false);
            }
            Err(e) => return Err(e).with_context(|| "Failed to read from parent_fd"),
        }
        return Ok(true);
    }

    pub fn exit(self) {
        self.hermit_client.exit();
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

pub fn get_shell(shell_name: Option<String>) -> Result<Box<dyn ShellCreator>> {
    let shell_pathname: String = match shell_name {
        Some(name) => name,
        None => env::var("SHELL").with_context(|| "$SHELL is not set")?,
    };
    if let Some(file_name) = PathBuf::from(&shell_pathname).file_name() {
        let file_name_str = file_name.to_string_lossy();
        match file_name_str.as_ref() {
            "bash" => return Ok(Box::new(Bash::new(shell_pathname, "bash".to_string()))),
            "zsh" => return Ok(Box::new(Bash::new(shell_pathname, "zsh".to_string()))),
            "csh" => todo!(),
            other => return Ok(Box::new(Bash::new(shell_pathname, other.to_string()))),
        }
    } else {
        return Err(anyhow!(util::Error::Failed(
            "the SHELL path terminates in '..'".to_string(),
        )));
    }
}

fn make_string_id(s: &str) -> String {
    String::from(s.replace("\n", "\r\n"))
}

/*********************************** BASH ***********************************/

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum BashState {
    Idle,     // State starts when ##LLMSH-OUT-END##, ends with ##LLMSH-CMD-START##
    CmdInput, // State starts when ##LLMSH-CMD-START##
    Output,   // State starts when ##LLMSH-CMD-END##, State ends when ##LLMSH-OUT-END##
}

struct Bash {
    shell_name: String,
    shell_pathname: String,
    input_end_marker: String,
    output_end_marker: String,
}

impl Bash {
    fn new(shell_pathname: String, shell_name: String) -> Bash {
        let input_end_marker: String = Uuid::new_v4().to_string() + "\n";
        let output_end_marker: String = Uuid::new_v4().to_string() + "\n";

        log::debug!(
            "Bash Input End Marker: [{}], Output End Marker: [{}]",
            input_end_marker,
            output_end_marker
        );

        return Bash {
            shell_name: shell_name,
            shell_pathname,
            input_end_marker: input_end_marker,
            output_end_marker: output_end_marker,
        };
    }
}

impl ShellCreator for Bash {
    fn get_path(&self) -> CString {
        return CString::new(self.shell_pathname.clone()).unwrap();
    }

    fn get_rcfile(&self) -> String {
        return format!("~/.{}rc", self.shell_name);
    }

    fn inject_markers(&self, mut temp_rc: &NamedTempFile) {
        // Inject our prompt markers
        let orig_ps0 = get_shell_variable(&self.shell_pathname, "PS0");
        let orig_ps1 = get_shell_variable(&self.shell_pathname, "PS1");
        let _ = temp_rc.write_all(
            &format!("export PS0=\"{}{}\"\n", self.input_end_marker, &orig_ps0).into_bytes(),
        );

        // If current ps1 uses $ as the ending, replace with our crab identifier
        if let Some(_dollar_idx) = orig_ps1.rfind("\\$") {
            let new_ps1 = replace_last(&orig_ps1, "\\$", SHELL_PROMPT_INPUT_START);

            let _ = temp_rc.write_all(
                &format!("export PS1=\"{}{}\"\n", self.output_end_marker, new_ps1).into_bytes(),
            );
        } else {
            let _ = temp_rc.write_all(
                &format!(
                    "export PS1=\"{}{}{}\"\n",
                    self.output_end_marker,
                    &orig_ps1,
                    String::from(SHELL_PROMPT_INPUT_START)
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
        let input_parser = ShellInputStateMachine::new();
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
            input_parser,
            output_parser,
            output_aggregation: Vec::new(),
            input_rl: Reedline::create(),
            rl_prompt: DefaultPrompt::new(
                DefaultPromptSegment::Basic(HERMITD_PROMPT_HEADER.to_string()),
                DefaultPromptSegment::Empty,
            ),
        }
    }
}
struct BashParser {
    parser: parsing::BufferParser<BashState, ShellOutputType, bool>,
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
                            StringID(make_string_id(SHELL_PROMPT_INPUT_START), true),
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
                HashMap::from([
                    (BashState::Idle, false),
                    (BashState::CmdInput, true),
                    (BashState::Output, false),
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
                parsing::StepResults::Echo { event, step } => ParsedOutput::InProgress {
                    step,
                    aggregate_locally: event,
                },
                parsing::StepResults::StateChange { event, step } => ParsedOutput::Output {
                    output_type: event,
                    step,
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

fn get_shell_variable(shell_pathname: &str, variable_name: &str) -> String {
    // prioritize getting the variable value from environment
    let env_res = env::var(variable_name);
    if let Ok(value) = env_res {
        return value;
    }

    // Spawning a shell and executing `echo $variable_name`
    let echo_output = Command::new(shell_pathname)
        .arg("-ic")
        .arg(format!("printf '%s' \"${}\"", variable_name))
        .output();

    match echo_output {
        Ok(output) => {
            // Checking if the command was successful
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).to_string();
            }
        }
        _ => {}
    }
    return String::from("");
}
