from llm.interfaces import LLM
from llm.llama3 import Llama3
import messages


class History:
    class Blob:
        command: str
        output: str
        exit_code: str
        summary: str

        def __init__(
            self, command: str, output: str, exit_code: str, summary: str
        ) -> None:
            self.command = command
            self.output = output
            self.exit_code = exit_code
            self.summary = summary

    blob_list: list[Blob]
    session_id: str
    summary: str

    def __init__(self, session_id: str) -> None:
        self.session_id = session_id
        self.blob_list = list()

    def append(self, context: messages.SaveContext, summary: str) -> None:
        blob = self._save_context_to_blob(context, summary)
        self.blob_list.append(blob)

    def _save_context_to_blob(
        self, context: messages.SaveContext, summary: str
    ) -> Blob:
        return self.Blob(context.command, context.output, context.exit_code, summary)


class Bot:
    llm: LLM
    spec: messages.Setup
    history: History

    def __init__(self) -> None:
        self.llm = Llama3()

    def set_up(self, spec: messages.Setup):
        # TODO properly define
        self.history = History(0)
        self.spec = spec

    def generate_command(
        self, request: messages.GenerateCommand
    ) -> messages.CommandResponse:
        prompt = "Generate a bash command to solve the issue: "
        command = self.llm.generate(request.prompt, header=prompt)
        return messages.CommandResponse(type="CommandResponse", result=command)

    def save_context(self, context: messages.SaveContext):
        prompt = "This is the commands user previous ran, the corresponding output, and exit code. Generate a summary of user's action, and outcome."
        msg = "input: " + context.command + "\n output: " + context.output
        summary = self.llm.generate(msg, prompt)
        self.history.append(context, summary)
