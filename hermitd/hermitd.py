import zmq
import json
from pydantic import BaseModel, Field

class GenerateCommand(BaseModel):
    type: str = Field("GenerateCommand", const=True)
    prompt: str

class SaveContext(BaseModel):
    type: str = Field("SaveContext", const=True)
    output: str
    exit_code: int

class ResponseMessage(BaseModel):
    type: str
    status: str
    command: str = None

def main():
    context = zmq.Context()
    socket = context.socket(zmq.REP)
    socket.bind("ipc:///tmp/hermitd-ipc")

    print("Python server is running...")

    while True:
        message = socket.recv_string()
        data = json.loads(message)
        message_type = data["type"]
        
        if message_type == "GenerateCommand":
            msg = GenerateCommand(**data)
            reply = ResponseMessage(
                type="response",
                status="success",
                message=f"Hello from Python, you sent: {msg.content}"
            )
        elif message_type == "SaveContext":
            msg = SaveContext(**data)
            result = msg.num1 + msg.num2
            reply = ResponseMessage(
                type="response",
                status="success",
                result=result
            )
        else:
            reply = ResponseMessage(
                type="error",
                status="unknown message type"
            )
        
        socket.send_string(reply.json())

if __name__ == "__main__":
    main()
