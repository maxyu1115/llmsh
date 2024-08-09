import zmq
import json
import hermitd.bot as bot
import hermitd.messages as messages


HERMITD_IPC_ENDPOINT = "ipc:///tmp/hermitd-ipc"


def run_daemon():
    context = zmq.Context()
    socket = context.socket(zmq.REP)
    socket.bind(HERMITD_IPC_ENDPOINT)

    # initiates llm.
    # TODO: properly intialize it on a per session basis
    hermit = bot.Bot("", 0)

    print("Python server is running...")

    while True:
        message = socket.recv_string()
        if message == messages.ALIVE_REQ:
            socket.send_string(messages.ALIVE_RESP)
            continue
        print("Recieved:" + message)

        data = json.loads(message)
        message_type = data["type"]

        try:
            if message_type == "GenerateCommand":
                msg = messages.GenerateCommand(**data)
                reply = hermit.generate_command(msg)
            elif message_type == "SaveContext":
                msg = messages.SaveContext(**data)
                # hermit.save_context(msg)
                reply = messages.Success(type="Success")
            elif message_type == "Setup":
                msg = messages.Setup(**data)
                reply = messages.SetupSuccess(type="SetupSuccess", session_id=0)
            else:
                reply = messages.Error(status="Illegal message type")
        except Exception as e:
            reply = messages.Error(status=str(e))

        reply_json = reply.model_dump_json()
        print("Reply: " + reply_json)

        socket.send_string(reply_json)
