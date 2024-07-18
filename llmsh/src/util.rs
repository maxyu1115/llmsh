use std::fs::OpenOptions;
use std::path::Path;

#[macro_export]
macro_rules! map_err {
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| {
            log::error!("{}: {:?}", $msg, e);
            log::debug!("{:?}", std::backtrace::Backtrace::capture());
            format!("{}: {}", $msg, e.to_string())
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
                print!("{}\r\n", $msg);
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
