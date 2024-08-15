use log;
use serde::{Deserialize, Serialize};
use std::env;
use zmq;

use crate::util;
use crate::{illegal_state, map_err};

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum ShellOutputType {
    Header,
    Input,
    InputAborted,
    Output,
}

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
        context_type: ShellOutputType,
        context: String,
    },
    Exit {
        session_id: u32,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum Response {
    SetupSuccess { session_id: u32, motd: String },
    CommandResponse { command: String },
    Error { status: String },
    // generic success message, used for apis we only care about success vs failure
    Success,
}

const HERMITD_ENDPOINT: &str = "ipc:///tmp/hermitd-ipc";

const ALIVE_MSG: &str = "";
const ALIVE_RESP: &str = "Ack";
const BUSY_RESP: &str = "Busy";

pub struct HermitdClient {
    socket: zmq::Socket,
    session_id: u32,
}

impl HermitdClient {
    pub fn init_client() -> Result<(HermitdClient, String), util::Error> {
        let context = zmq::Context::new();
        let socket = map_err!(context.socket(zmq::REQ), "Failed to create zmq socket")?;
        // Set linger so we can close
        map_err!(socket.set_linger(1000), "Failed to set zmq_linger")?;
        map_err!(socket
            .connect(HERMITD_ENDPOINT),
            "Failed to connect to hermitd ipc endpoint [/tmp/hermitd-ipc], please check your file system permissions")?;

        let (session_id, motd) = HermitdClient::setup_session(&socket)?;

        Ok((HermitdClient { socket, session_id }, motd))
    }

    fn _send_str(socket: &zmq::Socket, msg: &str, timeout: i32) -> Result<String, util::Error> {
        log::info!("Sending request: {}", msg);
        map_err!(socket.send(msg, 0), "Failed to send message to hermitd")?;

        // Set timeout for the alive receive operation
        map_err!(
            socket.set_rcvtimeo(timeout),
            "Failed to set receive zmq_timeout"
        )?;
        let resp = socket.recv_string(0);

        let resp = match resp {
            Err(zmq::Error::EAGAIN) => {
                return Err(util::Error::HermitDead);
            }
            other => other,
        };
        let resp = map_err!(resp, "Receive string failed for ALIVE response")?;
        let resp_str = map_err!(resp, "Failed Vec<u8> to utf8 conversion, received: {:?}")?;
        return Ok(resp_str);
    }

    fn is_alive(socket: &zmq::Socket) -> Result<(), util::Error> {
        let resp_str = HermitdClient::_send_str(socket, ALIVE_MSG, 500)?;
        return match resp_str.as_str() {
            ALIVE_RESP => Ok(()),
            BUSY_RESP => Err(util::Error::HermitBusy),
            other => illegal_state!(format!(
                "Illegal State: Unexpected Response Message Type {:?}",
                other
            )),
        };
    }

    fn send_msg(socket: &zmq::Socket, msg: Request, timeout: i32) -> Result<Response, util::Error> {
        // Check first that hermitd is alive
        HermitdClient::is_alive(socket)?;

        let request_json = map_err!(
            serde_json::to_string(&msg),
            "Failed to convert request object to json"
        )?;
        let reply_json = HermitdClient::_send_str(socket, &request_json, timeout)?;
        let reply: Response = map_err!(
            serde_json::from_str(&reply_json),
            "Failed to convert received json string to response object"
        )?;
        return Ok(reply);
    }

    fn setup_session(socket: &zmq::Socket) -> Result<(u32, String), util::Error> {
        let user: String = map_err!(env::var("USER"), "$USER is not set")?;
        let setup_request = Request::Setup { user };
        let reply = HermitdClient::send_msg(socket, setup_request, 1000)?;
        match reply {
            Response::SetupSuccess { session_id, motd } => Ok((session_id, motd)),
            Response::Error { status } => Err(util::Error::HermitFailed(format!(
                "Hermitd returned error with status {}",
                status
            ))),
            other => illegal_state!(format!(
                "Illegal State: Unexpected Response Message Type {:?}",
                other
            )),
        }
    }

    pub fn save_context(
        &self,
        context_type: ShellOutputType,
        context: String,
    ) -> Result<(), util::Error> {
        let save_request = Request::SaveContext {
            session_id: self.session_id,
            context_type,
            context,
        };
        let reply = HermitdClient::send_msg(&self.socket, save_request, 1000)?;
        match reply {
            Response::Success => return Ok(()),
            Response::Error { status } => {
                return Err(util::Error::HermitFailed(status));
            }
            other => illegal_state!(format!(
                "Illegal State: Unexpected Response Message Type {:?}",
                other
            )),
        }
    }

    pub fn generate_command(&self, prompt: String) -> Result<String, util::Error> {
        let gen_request = Request::GenerateCommand {
            session_id: self.session_id,
            prompt,
        };
        let reply = HermitdClient::send_msg(&self.socket, gen_request, 10000)?;
        match reply {
            Response::CommandResponse { command } => {
                return Ok(command);
            }
            Response::Error { status } => {
                return Err(util::Error::HermitFailed(status));
            }
            other => illegal_state!(format!(
                "Illegal State: Unexpected Response Message Type {:?}",
                other
            )),
        }
    }

    pub fn exit(&self) {
        let exit_request = Request::Exit {
            session_id: self.session_id,
        };

        // For exit we don't care if we succeed or not, as we're already exiting anyways,
        //  and it's only courtesy to let hermitd know
        let _ = HermitdClient::send_msg(&self.socket, exit_request, 2000);
    }
}
