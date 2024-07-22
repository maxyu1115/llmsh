use std::fmt;
use std::fs::OpenOptions;
use std::path::Path;

pub enum Error {
    Failed(String),
    HermitFailed(String),
    HermitBusy,
    HermitDead,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Failed(s) => write!(fmt, "{}", s),
            Error::HermitBusy => write!(fmt, "hermitd is Busy"),
            Error::HermitDead => write!(
                fmt,
                "hermitd is unresponsive. Please check if you have hermitd started"
            ),
            Error::HermitFailed(s) => write!(fmt, "hermitd failed with error: {}", s),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self)
    }
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
