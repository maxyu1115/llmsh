import dataclasses
import enum
from hermitd.llm._interfaces import *  # noqa: F401, F403


class ModelHost(enum.Enum):
    OpenAI = "openai"
    Anthropic = "anthr"
    Local = "local"


@dataclasses.dataclass
class ModelID:
    host: ModelHost
    model_tag: str

    def __str__(self) -> str:
        return f"{self.host.value}-{self.model_tag}"


class SupportedLLMs(enum.Enum):
    Claude = ModelID(ModelHost.Anthropic, "claude-3.5")
    GPT = ModelID(ModelHost.OpenAI, "gpt-4o-mini")
    Llama3 = ModelID(ModelHost.Local, "llama-8b")
    Gemma2 = ModelID(ModelHost.Local, "gemma-2b")

    @staticmethod
    def from_tag(string_value):
        try:
            return next(
                member for member in SupportedLLMs if str(member.value) == string_value
            )
        except StopIteration:
            raise ValueError(f"{string_value} is not a supported llm")
