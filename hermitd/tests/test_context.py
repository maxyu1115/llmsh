import pytest
from hermitd.context import Context, TRUNCATED_MARKER
from hermitd.messages import ShellOutputType


@pytest.mark.parametrize(
    "input, expected_output",
    [
        ([(ShellOutputType.Header, "1")], []),
        ([(ShellOutputType.Input, "1")], [[(ShellOutputType.Input, "1")]]),
        (
            [
                (ShellOutputType.Output, "1"),
                (ShellOutputType.Input, "2"),
                (ShellOutputType.Output, "3"),
            ],
            [
                [(ShellOutputType.Output, "1")],
                [(ShellOutputType.Input, "2"), (ShellOutputType.Output, "3")],
            ],
        ),
        (
            [
                (ShellOutputType.Input, "1"),
                (ShellOutputType.Output, "2"),
                (ShellOutputType.Input, "3"),
            ],
            [
                [(ShellOutputType.Input, "1"), (ShellOutputType.Output, "2")],
                [(ShellOutputType.Input, "3")],
            ],
        ),
        (
            [
                (None, "0"),
                (ShellOutputType.Input, "1"),
                (None, "0"),
                (ShellOutputType.Output, "2"),
                (None, "0"),
            ],
            [
                [(ShellOutputType.Input, "01"), (ShellOutputType.Output, "02")],
            ],
        ),
    ],
)
def test_save_shell_context(input, expected_output):
    ctx = Context()
    for data_type, data in input:
        ctx.save_shell_context(data_type, data)

    assert ctx._get_shell_ctx() == expected_output


@pytest.mark.parametrize(
    "input, expected_output",
    [
        (
            [
                (None, "0"),
                (None, "1"),
                (None, "2"),
                (None, "3"),
                (ShellOutputType.Input, "input"),
            ],
            [
                [(ShellOutputType.Input, f"012{TRUNCATED_MARKER}input")],
            ],
        ),
        (
            [
                (None, "01234567"),
                (ShellOutputType.Input, "input"),
            ],
            [
                [(ShellOutputType.Input, f"012{TRUNCATED_MARKER}input")],
            ],
        ),
        (
            [
                (None, "012"),
                (ShellOutputType.Input, "input"),
            ],
            [
                [(ShellOutputType.Input, "012input")],
            ],
        ),
    ],
)
def test_save_shell_context_truncation(input, expected_output):
    ctx = Context(max_chunk_length=3)

    for data_type, data in input:
        ctx.save_shell_context(data_type, data)

    assert ctx._get_shell_ctx() == expected_output
