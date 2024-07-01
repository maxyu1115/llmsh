from llm.interfaces import LLM
from llm.llama3 import Llama3
from pydantic import BaseModel, Field

class Setup(BaseModel):
    type: str = Field("Setup")
    user: str
    session_id: str
    system: str

class GenerateCommand(BaseModel):
    type: str = Field("GenerateCommand")
    input: str

class SaveContext(BaseModel):
    type: str = Field("SaveContext")
    command: str # the command user previously run.
    output: str
    exit_code: int
    summary: str

class ResponseMessage(BaseModel):
    type: str
    status: str
    command: str = None

class Bot:
    llm: LLM
    context : list
    spec: Setup
    
    def __init__(self, spec: Setup):
        self.llm = Llama3()
        self.context = []
        self.spec = spec
        
    def generateCommand(self, request: GenerateCommand) -> ResponseMessage:
        prompt = "Generate a bash command to solve the issue: "
        response = self.llm.generate(request.input, prompt)
        return ResponseMessage(
                type="response",
                status="success",
                command=response
            )
        
    def saveContext(self, context: SaveContext):
        prompt = "This is the commands user previous ran, the corresponding output, and exit code. Generate a summary of user's action, and outcome."
        msg = "input: " + context.input + "\n output: " + context.output
        context.summary = self.llm.generate(msg, prompt)
        self.context.append(context)
        

if __name__ == "__main__":
   b = Bot(None)
   print("started llm")
   r = b.generateCommand(GenerateCommand(input="list all files in the current directory"))
   print("result", r.command)