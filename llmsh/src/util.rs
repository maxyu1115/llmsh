use std::fmt;
use std::fs::OpenOptions;
use std::path::Path;

pub enum Error {
    Failed(String),
    HermitFailed(String),
    HermitBusy,
    HermitDead,
    IllegalState(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Failed(msg) => write!(fmt, "{}", msg),
            Error::HermitBusy => write!(fmt, "hermitd is Busy"),
            Error::HermitDead => write!(
                fmt,
                "hermitd is unresponsive. Please check if you have hermitd started"
            ),
            Error::HermitFailed(msg) => write!(fmt, "hermitd failed with error: {}", msg),
            Error::IllegalState(msg) => write!(fmt, "Illegal State Exception: {}", msg),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self)
    }
}

#[macro_export]
macro_rules! illegal_state {
    ($msg:expr) => {{
        log::error!("Illegal State Exception: {}", $msg);
        log::debug!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
        return Err(util::Error::IllegalState($msg.to_string()));
    }};
}

#[macro_export]
macro_rules! map_err {
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| {
            log::error!("{}: {:?}", $msg, e);
            log::debug!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
            util::Error::Failed(format!("{}: {:?}", $msg, e))
        })
    };
}

#[macro_export]
macro_rules! expect {
    ($result:expr, $msg:expr) => {
        match $result {
            Ok(out) => out,
            Err(e) => {
                log::error!("{}: {:?}", $msg, e);
                print!("{}: {:?}\r\n", $msg, e);
                std::process::exit(1);
            }
        }
    };
}

pub fn touch(path: &Path) -> std::io::Result<()> {
    match OpenOptions::new().create(true).write(true).open(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn fix_newlines(input: Vec<u8>) -> Vec<u8> {
    input
        .into_iter()
        .flat_map(|c| {
            if c == b'\n' {
                vec![b'\r', b'\n'].into_iter()
            } else {
                vec![c].into_iter()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    // Import the parent module's items
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("\n", "\r\n")]
    #[case("H\ni\r?", "H\r\ni\r?")]
    #[case("Hi.", "Hi.")]
    fn test_fix_newlines(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(
            fix_newlines(input.as_bytes().to_vec()),
            expected.as_bytes().to_vec()
        );
    }
}
