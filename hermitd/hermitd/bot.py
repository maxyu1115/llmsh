import textwrap
from hermitd.llm import LLM
from hermitd.context import Context
import hermitd.messages as messages


class Bot:
    GEN_CMD_PROMPT = textwrap.dedent(
        """You are an assistant for our user using a posix shell.
        Your job is to generate a shell command satisfying the USER's prompt. \n
        """
    )

    def __init__(self, user: str, session_id: int, llm: LLM) -> None:
        self.user: str = user
        self.session_id: int = session_id
        self.llm: LLM = llm
        self.context: Context = Context()

    def generate_command(self, user_request: str) -> str:
        prompt = Bot.GEN_CMD_PROMPT + self.context.get_context_prompt()
        return self.llm.generate(user_request, header=prompt)

    def save_context(self, context_type: messages.ShellOutputType, context: str):
        self.context.save_shell_context(context_type, context)
