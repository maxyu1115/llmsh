import anthropic
from hermitd.llm._interfaces import LLM

DEFAULT_MAX_TOKENS = 1000


class Claude(LLM):
    def __init__(self, api_key: str, model: str, max_tokens: int) -> None:
        self.model = model
        self.max_tokens = max_tokens

        self.client = anthropic.Anthropic(
            # defaults to os.environ.get("ANTHROPIC_API_KEY")
            api_key=api_key,
        )

    def generate(
        self, message: str, history: list[tuple[str, str]] = None, header: str = None
    ) -> str:
        messages = []
        history = history or []

        for human, assistant in history:
            messages.append({"role": "user", "content": human})
            messages.append({"role": "assistant", "content": assistant})

        messages.append({"role": "user", "content": message})

        if header:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=self.max_tokens,
                system=header,
                messages=messages,
                temperature=0,
            )
        else:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=self.max_tokens,
                messages=messages,
                temperature=0,
            )
        return response.content[0].text


class Claude3Opus(Claude):
    def __init__(self, api_key: str, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__(api_key, "claude-3-opus-20240229", max_tokens)


class Claude3Sonnet(Claude):
    def __init__(self, api_key: str, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__(api_key, "claude-3-sonnet-20240229", max_tokens)


class Claude35Sonnet(Claude):
    def __init__(self, api_key: str, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__(api_key, "claude-3-5-sonnet-20240620", max_tokens)


class Claude3Haiku(Claude):
    def __init__(self, api_key: str, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__(api_key, "claude-3-haiku-20240307", max_tokens)
