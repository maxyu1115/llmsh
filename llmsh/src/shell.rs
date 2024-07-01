use std::env;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

pub trait ShellParser {
    fn get_rcfile(&self) -> String;
    fn inject_markers(&self, temp_rc: &NamedTempFile);
    fn parse(&self);
}

pub enum Shell {
    BASH,
    ZSH,
    CSH,
    MISC(String),
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

pub fn get_shell() -> Option<Shell> {
    let shell_pathname: String = env::var("SHELL").expect("$SHELL is not set");
    if let Some(file_name) = PathBuf::from(shell_pathname).file_name() {
        let file_name_str = file_name.to_string_lossy();
        match file_name_str.as_ref() {
            "bash" => return Some(Shell::BASH),
            "zsh" => return Some(Shell::ZSH),
            "csh" => return Some(Shell::CSH),
            other => return Some(Shell::MISC(String::from(other))),
        }
    } else {
        return None;
    }
}


impl ShellParser for Shell {
    fn get_rcfile(&self) -> String {
        match self {
            Shell::BASH => return String::from("~/.bashrc"),
            Shell::ZSH => return String::from("~/.zshrc"),
            Shell::CSH => return String::from("~/.cshrc"),
            Shell::MISC(s) => return format!("~/.{}rc", s),
        }
    }
    fn inject_markers(&self, temp_rc: &NamedTempFile) {
        match self {
            Shell::BASH => bash_inject_markers(temp_rc),
            Shell::ZSH => bash_inject_markers(temp_rc),
            Shell::CSH => todo!(),
            Shell::MISC(_) => todo!(),
        }
    }
    fn parse(&self) {
        todo!()
    }
}


/*********************************** BASH ***********************************/

const BASH_PROMPT_INPUT_START: &str = "##LLMSH-CMD-START##\n";
const BASH_PROMPT_INPUT_END: &str = "##LLMSH-CMD-END##\n";
const BASH_PROMPT_OUTPUT_END: &str = "##LLMSH-OUT-END##\n";


fn bash_inject_markers(mut temp_rc: &NamedTempFile) {
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

