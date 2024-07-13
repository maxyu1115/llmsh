use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Request {
    GenerateCommand {
        prompt: String,
    },
    SaveContext {
        command: String,
        output: String,
        exit_code: u32,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Response {
    CommandResponse { status: String, command: String },
    Error { status: String },
}
