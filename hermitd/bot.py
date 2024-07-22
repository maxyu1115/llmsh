from llm.interfaces import LLM
from llm.llama3 import Llama3
from pydantic import BaseModel, Field

class Setup(BaseModel):
    type: str = Field("Setup", const=True)
    user: str
    session_id: str
    system: str

class GenerateCommand(BaseModel):
    type: str = Field("GenerateCommand", const=True)
    input: str

class SaveContext(BaseModel):
    type: str = Field("SaveContext", const=True)
    command: str # the command user previously run.
    output: str
    exit_code: int

class ResponseMessage(BaseModel):
    type: str
    status: str
    command: str = None
    
    
class History:
    class Blob:
        command: str
        output: str
        exit_code: str
        summary: str
        
        def __init__(self, command: str, output: str, exit_code: str, summary: str) -> None:
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
    
    def append(self, context: SaveContext, summary: str) -> None:
        blob = self._saveContextToBlob(context, summary)
        self.blob_list.append(blob)
        
    def _saveContextToBlob(self, context: SaveContext, summary: str) -> Blob:
        return self.Blob(context.command, context.output, context.exit_code, summary)

class Bot:
    llm: LLM
    spec: Setup
    history: History
    
    def __init__(self, spec: Setup):
        self.llm = Llama3()
        self.history = History(spec.session_id)
        self.spec = spec
        
    def handle(self, request: GenerateCommand) -> ResponseMessage:
        prompt = "Generate a bash command to solve the issue: "
        command = self.llm.generate(request.input, prompt)
        return ResponseMessage(
                type="response",
                status="success",
                result=command
            )
        
    def saveContext(self, context: SaveContext):
        prompt = "This is the commands user previous ran, the corresponding output, and exit code. Generate a summary of user's action, and outcome."
        msg = "input: " + context.command + "\n output: " + context.output
        summary = self.llm.generate(msg, prompt)
        self.history.append(context, summary)
        
        