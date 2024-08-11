import enum
from hermitd.llm._interfaces import *


class SupportedLLMs(enum.Enum):
    Claude = "anthr-claude-3.5"
    GPT = "openai-gpt-4o-mini"
    Llama3 = "local-llama-3"

    @staticmethod
    def from_tag(string_value):
        try:
            return next(
                member for member in SupportedLLMs if member.value == string_value
            )
        except StopIteration:
            raise ValueError(f"{string_value} is not a supported llm")
