import zmq
import json
from pydantic import BaseModel, Field
# from llm import llama3
from bot import Bot
import messages

# def init_llm() -> llm:
#     return llama3()

def main():
    context = zmq.Context()
    socket = context.socket(zmq.REP)
    socket.bind("ipc:///tmp/hermitd-ipc")
    
    # initiates llm.
    bot = Bot()

    print("Python server is running...")

    while True:
        message = socket.recv_string()
        print("Recieved:" + message)
        if message == "":
            socket.send_string("Ack")
            continue

        data = json.loads(message)
        message_type = data["type"]
        
        if message_type == "GenerateCommand":
            msg = messages.GenerateCommand(**data)
            reply = bot.generateCommand(msg)
        elif message_type == "SaveContext":
            msg = messages.SaveContext(**data)
            bot.saveContext(msg)
            reply = messages.Success(type="Success")
        elif message_type == "Setup":
            msg = messages.Setup(**data)
            bot.setUp(msg)
            reply = messages.SetupSuccess(type="SetupSuccess", session_id=0)
        else:
            reply = messages.Error(status="Illegal message type")
        
        socket.send_string(reply.model_dump_json())

if __name__ == "__main__":
    main()