import textwrap
from hermitd.llm import LLM
from hermitd.context import Context
import hermitd.messages as messages


def find_all(a_str: str, sub: str, overlap: bool = False) -> list[int]:
    if not sub:
        raise ValueError("Empty String is not supported")
    start = 0
    jump = 1 if overlap else len(sub)
    out = []
    while True:
        start = a_str.find(sub, start)
        if start == -1:
            return out
        out.append(start)
        start += jump


MD_CODE_ID = "```"


def parse_code_md(response: str) -> list[str]:
    indexes = find_all(response, MD_CODE_ID)
    if len(indexes) % 2 != 0:
        raise ValueError("Malformed response from LLM, detected mismatching ```s. ")

    commands = []
    for i in range(0, len(indexes), 2):
        # we want to parse out everything from ``` to newline. Because this often times has things like
        #   "```bash", "```python" or other headers
        new_line_pos = response.find("\n", indexes[i] + len(MD_CODE_ID), indexes[i + 1])
        commands.append(response[new_line_pos + 1 : indexes[i + 1]].strip())

    return commands


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

    def generate_command(self, user_request: str) -> tuple[str, list[str]]:
        prompt = Bot.GEN_CMD_PROMPT + self.context.get_context_prompt()
        # TODO: explore using structed generation instead of parsing
        response = self.llm.generate(user_request, header=prompt)
        commands = parse_code_md(response)
        return (response, commands)

    def save_context(self, context_type: messages.ShellOutputType, context: str):
        self.context.save_shell_context(context_type, context)
