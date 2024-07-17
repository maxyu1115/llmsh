from enum import Enum
from typing import Literal
from pydantic import BaseModel


# Requests
class Setup(BaseModel):
    type: Literal["Setup"]
    user: str

class GenerateCommand(BaseModel):
    type: Literal["GenerateCommand"]
    session_id: int
    prompt: str

class ShellOutputType(str, Enum):
    Header="Header"
    Input="Input"
    InputAborted="InputAborted"
    Output="Output"

class WrappedOutputType(BaseModel):
    type: ShellOutputType

class SaveContext(BaseModel):
    type: Literal["SaveContext"]
    session_id: int
    context: str
    context_type: WrappedOutputType



# Responses
class Success(BaseModel):
    type: Literal["Success"]

class Error(BaseModel):
    type: Literal["Error"]
    status: str

class SetupSuccess(BaseModel):
    type: Literal["SetupSuccess"]
    session_id: int

class CommandResponse(BaseModel):
    type: Literal["CommandResponse"]
    command: str
