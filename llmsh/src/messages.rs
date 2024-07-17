use log;
use serde::{Deserialize, Serialize};
use std::env;
use zmq;

use crate::shell;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Request {
    Setup {
        user: String,
    },
    GenerateCommand {
        session_id: u32,
        prompt: String,
    },
    SaveContext {
        session_id: u32,
        context_type: shell::ShellOutputType,
        context: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Response {
    SetupSuccess { session_id: u32 },
    CommandResponse { status: String, command: String },
    Error { status: String },
    // generic success message, used for apis we only care about success vs failure
    Success,
}

const HERMITD_ENDPOINT: &str = "ipc:///tmp/hermitd-ipc";

pub struct HermitdClient {
    socket: zmq::Socket,
    session_id: u32,
}

impl HermitdClient {
    pub fn init_client() -> Result<HermitdClient, String> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::REQ).unwrap();
        socket
            .connect(HERMITD_ENDPOINT)
            .expect("Failed to connect to hermitd, please ensure hermitd is running");

        let session_id = HermitdClient::setup_session(&socket)?;

        Ok(HermitdClient { socket, session_id })
    }

    fn setup_session(socket: &zmq::Socket) -> Result<u32, String> {
        let user: String = env::var("USER").expect("$USER is not set");
        let setup_request = Request::Setup { user };
        let request_json = serde_json::to_string(&setup_request).unwrap();
        log::info!("Sending request: {}", request_json);
        socket
            .send(&request_json, 0)
            .map_err(|_| "Failed to send SETUP request to hermitd")?;
        let reply_json = socket.recv_string(0).unwrap().unwrap();
        let reply: Response = serde_json::from_str(&reply_json).unwrap();
        match reply {
            Response::SetupSuccess { session_id } => Ok(session_id),
            Response::Error { status } => {
                Err(format!("Hermitd returned error with status {}", status))
            }
            _ => panic!("Illegal State: Unexpected Response Message Type"),
        }
    }

    pub fn save_context(
        &self,
        context_type: shell::ShellOutputType,
        context: String,
    ) -> Result<(), String> {
        let save_request = Request::SaveContext {
            session_id: self.session_id,
            context_type,
            context,
        };
        let request_json = serde_json::to_string(&save_request).unwrap();
        log::info!("Sending request: {}", request_json);
        self.socket
            .send(&request_json, 0)
            .map_err(|_| "Failed to send SAVE_CONTEXT request to hermitd")?;
        let reply_json = self.socket.recv_string(0).unwrap().unwrap();
        let reply: Response = serde_json::from_str(&reply_json).unwrap();
        match reply {
            Response::Success => return Ok(()),
            Response::Error { status } => {
                return Err(format!("Hermitd returned error with status {}", status))
            }
            _ => panic!("Illegal State: Unexpected Response Message Type"),
        }
    }
}
