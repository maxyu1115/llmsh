import json
import pytest
from unittest.mock import MagicMock, patch
import zmq
import hermitd.messages as messages
from hermitd.daemon import Hermitd, MAX_SESSIONS


@pytest.fixture
def mock_llm_provider():
    with patch("hermitd.daemon.LLMFactory") as mock_factory:
        mock_llm = MagicMock()
        mock_factory.return_value.get_llm.return_value = mock_llm
        yield mock_factory


# Fixture to patch the zmq socket
@pytest.fixture
def mock_zmq_socket():
    with patch("hermitd.daemon.zmq.Context") as mock_context:
        mock_socket = MagicMock()
        mock_context.return_value.socket.return_value = mock_socket
        yield mock_socket


@pytest.fixture
def mock_bot():
    with patch("hermitd.daemon.Bot") as mock_bot:
        yield mock_bot.return_value


@pytest.fixture
def patched_hermitd(mock_llm_provider, mock_zmq_socket):
    return Hermitd(mock_llm_provider)


# Sanity test to ensure the fixtures are working as intended
def test_fixtures(mock_zmq_socket, mock_llm_provider):
    hermit = Hermitd(mock_llm_provider)
    mock_zmq_socket.send_string.return_value = "I'm sane"
    assert hermit.zmq_socket.send_string() == "I'm sane"


def test_handle_alive_request(patched_hermitd):
    mock_socket = patched_hermitd.zmq_socket
    mock_socket.recv_string.return_value = messages.ALIVE_REQ

    patched_hermitd._run()

    mock_socket.send_string.assert_called_once_with(messages.ALIVE_RESP)


def test_malformed_ipc(patched_hermitd):
    mock_socket = patched_hermitd.zmq_socket
    mock_socket.recv_string.return_value = "wtf"

    patched_hermitd._run()

    assert json.loads(mock_socket.send_string.call_args.args[0])["type"] == "Error"


def test_handle_setup_message(patched_hermitd):
    data = {"type": "Setup", "user": "test_user"}
    response = patched_hermitd.handle_message(data)

    assert isinstance(response, messages.SetupSuccess)
    assert response.session_id < MAX_SESSIONS

    assert patched_hermitd.llm_provider.is_called()


def test_handle_generate_command(mock_bot, patched_hermitd):
    answer = "bye"
    mock_bot.generate_command.return_value = answer
    # init session
    session_id = patched_hermitd.create_session("test_user")
    data = {"type": "GenerateCommand", "prompt": "hey", "session_id": session_id}

    response = patched_hermitd.handle_message(data)
    assert isinstance(response, messages.CommandResponse)
    assert response.command == answer


def test_handle_generate_command(mock_bot, patched_hermitd):
    # init session
    session_id = patched_hermitd.create_session("test_user")
    exit_data = {"type": "Exit", "session_id": session_id}
    cmd_data = {"type": "GenerateCommand", "prompt": "hey", "session_id": session_id}

    response = patched_hermitd.handle_message(exit_data)
    assert isinstance(response, messages.Success)

    response = patched_hermitd.handle_message(cmd_data)
    assert isinstance(response, messages.Error)


def test_handle_save_context(mock_bot, patched_hermitd):
    context_type = messages.ShellOutputType.Input
    context = "hey"
    session_id = patched_hermitd.create_session("test_user")
    data = {
        "type": "SaveContext",
        "context_type": context_type,
        "context": context,
        "session_id": session_id,
    }

    response = patched_hermitd.handle_message(data)
    assert isinstance(response, messages.Success)
    mock_bot.save_context.assert_called_once_with(context_type, context)


@pytest.mark.parametrize(
    "data",
    [
        {"type": "GenerateCommand", "prompt": "hey"},
        {"type": "GenerateCommand", "prompt": "hey", "session_id": 100},
    ],
)
def test_handle_illegal_payload(data, patched_hermitd):
    response = patched_hermitd.handle_message(data)
    assert isinstance(response, messages.Error)


def test_handle_illegal_message_type(patched_hermitd):
    session_id = patched_hermitd.create_session("test_user")
    response = patched_hermitd.handle_message(
        {"type": "WhatIsThisMessageType", "session_id": session_id}
    )
    assert isinstance(response, messages.Error)
