import pytest
from hermitd.context import Context
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
