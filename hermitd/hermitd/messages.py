from enum import Enum
from typing import Literal, Final
from pydantic import BaseModel

API_VERSION: Final[str] = "0.1"

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
    api_version: str


class GenerateCommand(HermitRequest):
    type: Literal["GenerateCommand"]
    session_id: int
    prompt: str


class Exit(HermitRequest):
    type: Literal["Exit"]
    session_id: int


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
    motd: str


class CommandResponse(HermitResponse):
    type: Literal["CommandResponse"]
    command: str
