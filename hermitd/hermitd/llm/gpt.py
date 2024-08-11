import openai
from hermitd.llm._interfaces import LLM

DEFAULT_MAX_TOKENS = 1000


class GPT(LLM):
    def __init__(self, model: str, max_tokens: int) -> None:
        self.model = model
        self.max_tokens = max_tokens

    def generate(
        self, message: str, history: list[tuple[str, str]] = None, header: str = None
    ) -> str:
        messages = []
        history = history or []
        if header:
            messages.append({"role": "system", "content": header})

        for human, assistant in history:
            messages.append({"role": "user", "content": human})
            messages.append({"role": "assistant", "content": assistant})

        messages.append({"role": "user", "content": message})

        response = openai.chat.completions.create(
            model=self.model,
            messages=messages,
            max_tokens=self.max_tokens,
            n=1,
            stop=None,
            temperature=0,
        )

        return response.choices[0].message.content


class GPT4o(GPT):
    def __init__(self, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__("gpt-4o-2024-08-06", max_tokens)


class GPT4oMini(GPT):
    def __init__(self, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__("gpt-4o-mini-2024-07-18", max_tokens)


class GPT4(GPT):
    def __init__(self, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__("gpt-4-turbo-2024-04-09", max_tokens)


class ChatGPT(GPT):
    def __init__(self, max_tokens: int = DEFAULT_MAX_TOKENS) -> None:
        super().__init__("gpt-3.5-turbo-0125", max_tokens)
