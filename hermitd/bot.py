import textwrap
from hermitd.llm import LLM
from hermitd.llm.llama3 import Llama3
from hermitd.context import Context
import hermitd.messages as messages


class Bot:
    GEN_CMD_PROMPT = textwrap.dedent(
        """You are an assistant for our user using a posix shell. 
        Your job is to generate a shell command satisfying the USER's prompt. \n
        """
    )

    def __init__(self, user: str, session_id: int) -> None:
        self.user: str = user
        self.session_id: int = session_id
        self.llm: LLM = Llama3()
        self.context: Context = Context()

    def generate_command(
        self, request: messages.GenerateCommand
    ) -> messages.CommandResponse:
        prompt = Bot.GEN_CMD_PROMPT + self.context.get_context_prompt()
        command = self.llm.generate(request.prompt, header=prompt)
        return messages.CommandResponse(type="CommandResponse", command=command)

    def save_context(self, context: messages.SaveContext):
        self.context.save_shell_context(context.context_type, context.context)
