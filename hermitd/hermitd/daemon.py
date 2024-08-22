import json
import traceback
import zmq
from hermitd.bot import Bot
import hermitd.config as config
import hermitd.messages as messages
from hermitd.llm import LLMFactory, SingletonLLMFactory, SupportedLLMs


HERMITD_IPC_ENDPOINT = "ipc:///tmp/hermitd-ipc"
MAX_SESSIONS = 16

ILLEGAL_IPC_ERROR = messages.Error(
    type="Error",
    status="Malformed ipc message, suspect illegal access to hermitd",
)


MOTD = """Welcome to llmsh! I am the llm-powered hermit living in your shell, here to assist you. 
llmsh is simply a wrapper around your favorite shell specified in $SHELL, and is intended to work just like your shell.
If you want to ask for my help, type `:` as the first character on the prompt line. 
(Currently due to technical difficulties, clear out a line and then typing `:` will not work)
"""


class Hermitd:
    def __init__(self, llm_provider: LLMFactory) -> None:
        self.zmq_context: zmq.Context = zmq.Context()
        self.zmq_socket: zmq.SyncSocket = self.zmq_context.socket(zmq.REP)
        self.llm_provider: LLMFactory = llm_provider
        self.sessions: dict[int, Bot] = dict()
        self.available_session_ids: list[int] = list(range(MAX_SESSIONS))

    def start(self):
        self.zmq_socket.bind(HERMITD_IPC_ENDPOINT)

    def create_session(self, user: str) -> int:
        session_id = self.available_session_ids.pop(0)
        llm = self.llm_provider.get_llm()
        self.sessions[session_id] = Bot(user, session_id, llm)
        return session_id

    def handle_message(self, data) -> messages.HermitResponse:
        message_type = data["type"]

        if message_type == "Setup":
            msg = messages.Setup(**data)
            session_id = self.create_session(msg.user)
            return messages.SetupSuccess(
                type="SetupSuccess", session_id=session_id, motd=MOTD
            )

        if "session_id" not in data:
            return ILLEGAL_IPC_ERROR
        session_id = data["session_id"]

        if session_id not in self.sessions:
            return ILLEGAL_IPC_ERROR
        session: Bot = self.sessions[session_id]

        if message_type == "GenerateCommand":
            msg = messages.GenerateCommand(**data)
            cmd = session.generate_command(msg.prompt)
            return messages.CommandResponse(type="CommandResponse", command=cmd)
        elif message_type == "SaveContext":
            msg = messages.SaveContext(**data)
            session.save_context(msg.context_type, msg.context)
            return messages.SUCCESS
        elif message_type == "Exit":
            del self.sessions[session_id]
            self.available_session_ids.append(session_id)
            return messages.SUCCESS
        else:
            return messages.Error(type="Error", status="Illegal message type")

    def _run(self):
        message = self.zmq_socket.recv_string()
        if message == messages.ALIVE_REQ:
            self.zmq_socket.send_string(messages.ALIVE_RESP)
            return
        print("Recieved:" + message)
        try:
            data = json.loads(message)
            reply = self.handle_message(data)
        except json.JSONDecodeError as err:
            reply = ILLEGAL_IPC_ERROR
        except Exception as err:
            traceback.print_exception(err)
            reply = messages.Error(type="Error", status=str(err))

        reply_json = reply.model_dump_json()
        print("Reply: " + reply_json)
        self.zmq_socket.send_string(reply_json)

    def run(self):
        print("Python server is running...")
        while True:
            self._run()


def get_llm_provider(cfg: config.Config, secrets: config.Secrets) -> LLMFactory:
    # Only import based on which model is configured
    if (not cfg.llm) or (cfg.llm == SupportedLLMs.Llama3):
        from hermitd.llm.llama3 import Llama3

        return SingletonLLMFactory(Llama3())

    elif cfg.llm == SupportedLLMs.Claude:
        if not secrets.anthropic:
            raise ValueError("Specified claude but did not set an Anthropic API key")

        from hermitd.llm.claude import Claude35Sonnet

        return SingletonLLMFactory(Claude35Sonnet(secrets.anthropic))

    elif cfg.llm == SupportedLLMs.GPT:
        if not secrets.openai:
            raise ValueError("Specified gpt but did not set an OpenAI API key")

        from hermitd.llm.gpt import GPT4oMini
        import openai

        openai.api_key = secrets.openai
        return SingletonLLMFactory(GPT4oMini())


def run_daemon(config_path):
    cfg = config.read_config(config_path)
    secrets = config.read_api_keys()

    llm_provider = get_llm_provider(cfg, secrets)

    hermitd = Hermitd(llm_provider)
    hermitd.start()
    hermitd.run()
