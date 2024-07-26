import zmq
import json
from pydantic import BaseModel, Field
import bot
import messages


def main():
    context = zmq.Context()
    socket = context.socket(zmq.REP)
    socket.bind("ipc:///tmp/hermitd-ipc")

    # initiates llm.
    hermit = bot.Bot()

    print("Python server is running...")

    while True:
        message = socket.recv_string()
        print("Recieved:" + message)
        if message == messages.ALIVE_REQ:
            socket.send_string(messages.ALIVE_RESP)
            continue

        data = json.loads(message)
        message_type = data["type"]

        if message_type == "GenerateCommand":
            msg = messages.GenerateCommand(**data)
            reply = hermit.generate_command(msg)
        elif message_type == "SaveContext":
            msg = messages.SaveContext(**data)
            # hermit.save_context(msg)
            reply = messages.Success(type="Success")
        elif message_type == "Setup":
            msg = messages.Setup(**data)
            hermit.set_up(msg)
            reply = messages.SetupSuccess(type="SetupSuccess", session_id=0)
        else:
            reply = messages.Error(status="Illegal message type")

        socket.send_string(reply.model_dump_json())


if __name__ == "__main__":
    main()
