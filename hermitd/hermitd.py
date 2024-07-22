import zmq
import json
from pydantic import BaseModel, Field
from llm import llm, llama3
from bot import Bot, SaveContext, GenerateCommand, ResponseMessage
    
def init_llm() -> llm:
    return llama3()

def main():
    context = zmq.Context()
    socket = context.socket(zmq.REP)
    socket.bind("ipc:///tmp/hermitd-ipc")
    
    # initiates llm.
    bot = Bot()

    print("Python server is running...")

    while True:
        message = socket.recv_string()
        data = json.loads(message)
        message_type = data["type"]
        
        if message_type == "GenerateCommand":
            reply = bot.handle(GenerateCommand(**data))
        elif message_type == "SaveContext":
            bot.saveContext(SaveContext(**data))
        else:
            reply = ResponseMessage(
                type="error",
                status="unknown message type"
            )
        
        socket.send_string(reply.json())

if __name__ == "__main__":
    main()
