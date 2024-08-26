from hermitd.messages import ShellOutputType
import textwrap


IGNORED_TYPES = {ShellOutputType.Header}


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

    def __init__(self) -> None:
        self.shell_ctx: list[list[(ShellOutputType, str)]] = []
        self.current_dialogue: list[(ShellOutputType, str)] = []

    def save_shell_context(self, data_type: ShellOutputType, data: str):
        if data_type in IGNORED_TYPES:
            return

        self.current_dialogue.append((data_type, data))

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
