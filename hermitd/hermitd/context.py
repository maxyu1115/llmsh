from hermitd.messages import ShellOutputType
import textwrap
from typing import Optional


IGNORED_TYPES = {ShellOutputType.Header}
TRUNCATED_MARKER = "##TRUNCATED##"


class Context:
    PROMPT_HEADER = textwrap.dedent(
        """For context, here are some of the user's shell usage history. The user's inputs to the shell is after
        \"User Input:\", and the shell's output in response to user input is after \"Shell Output:\". Sometimes
        the user will abort their shell prompt using control C, and that likely means their aborted command is
        relevant their intentions but is not exactly what they want. Now here are the user's shell history:\n
        """
    )
    PROMPT_MAP = {
        ShellOutputType.Input: 'User Input: \\"{}\\"',
        ShellOutputType.InputAborted: 'User Aborted Input: \\"{}\\"',
        ShellOutputType.Output: 'Shell Output: \\"{}\\"',
    }

    def __init__(self, max_chunk_length: int = 4096) -> None:
        self.shell_ctx: list[list[(ShellOutputType, str)]] = []
        self.current_dialogue: list[(ShellOutputType, str)] = []
        self.undecided_stack: list[str] = []
        self.undecided_stack_len: int = 0
        self.max_chunk_length: int = max_chunk_length

    def _add_truncate_marker(self):
        self.undecided_stack.append(TRUNCATED_MARKER)
        self.undecided_stack_len += len(TRUNCATED_MARKER)

    def save_shell_context(self, data_type: Optional[ShellOutputType], data: str):
        if data_type in IGNORED_TYPES:
            self.undecided_stack = []
            return

        if data_type is None:
            if self.max_chunk_length < self.undecided_stack_len:
                # when exceeded, don't save anything
                return
            elif self.max_chunk_length < self.undecided_stack_len:
                self._add_truncate_marker()
                return
            saving_len = min(
                self.max_chunk_length - self.undecided_stack_len, len(data)
            )
            self.undecided_stack.append(data[:saving_len])
            self.undecided_stack_len += saving_len
            if saving_len < len(data):
                self._add_truncate_marker()
            return

        self.undecided_stack.append(data)
        self.current_dialogue.append((data_type, "".join(self.undecided_stack)))
        self.undecided_stack = []
        self.undecided_stack_len = 0

        if data_type == ShellOutputType.Output:
            # each time we have an output, that marks the end of a "dialogue"
            self.shell_ctx.append(self.current_dialogue)
            self.current_dialogue = []

    def _get_shell_ctx(self) -> list[list[(ShellOutputType, str)]]:
        if self.current_dialogue:
            return self.shell_ctx + [self.current_dialogue]
        else:
            return self.shell_ctx

    def get_context_prompt(self, context_window: int = 3) -> str:
        # only take the last few dialogues
        context_dialogue = self._get_shell_ctx()[-context_window:]

        # if there is no shell history don't add the context prompt
        if not context_dialogue:
            return ""

        items = []
        for diag in context_dialogue:
            for data_type, data in diag:
                items.append(Context.PROMPT_MAP[data_type].format(data))
        return Context.PROMPT_HEADER + "\n".join(items)
