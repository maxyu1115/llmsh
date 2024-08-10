from enum import Enum
from typing import Literal, Final
from pydantic import BaseModel

# Alive messages
ALIVE_REQ: Final[str] = ""
ALIVE_RESP: Final[str] = "Ack"
BUSY_RESP: Final[str] = "Busy"


# Requests
class HermitRequest(BaseModel):
    pass


class Setup(HermitRequest):
    type: Literal["Setup"]
    user: str


class GenerateCommand(HermitRequest):
    type: Literal["GenerateCommand"]
    session_id: int
    prompt: str


class ShellOutputType(str, Enum):
    Header = "Header"
    Input = "Input"
    InputAborted = "InputAborted"
    Output = "Output"


class SaveContext(HermitRequest):
    type: Literal["SaveContext"]
    session_id: int
    context: str
    context_type: ShellOutputType


# Responses
class HermitResponse(BaseModel):
    pass


class Success(HermitResponse):
    type: Literal["Success"]


# Singleton
SUCCESS: Final[Success] = Success(type="Success")


class Error(HermitResponse):
    type: Literal["Error"]
    status: str


class SetupSuccess(HermitResponse):
    type: Literal["SetupSuccess"]
    session_id: int


class CommandResponse(HermitResponse):
    type: Literal["CommandResponse"]
    command: str
