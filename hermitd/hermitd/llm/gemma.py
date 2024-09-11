from transformers import pipeline

from hermitd.llm._interfaces import LLM


class Gemma2(LLM):
    def __init__(self, api_key: str) -> None:
        self.llm = pipeline(
            "text-generation",
            model="google/gemma-2-2b-it",
            token=api_key,
            device="cuda:0",
        )

    def generate(
        self, message: str, history: list[tuple[str, str]] = None, header: str = None
    ) -> str:
        history_chat = []
        history = history or []
        if header:
            history_chat.append({"role": "user", "content": header})
            history_chat.append({"role": "assistant", "content": "I understand."})

        for human, assistant in history:
            history_chat.append({"role": "user", "content": human})
            history_chat.append({"role": "assistant", "content": assistant})
        history_chat.append({"role": "user", "content": message})

        return self.llm(history_chat, max_new_tokens=4096)[0]["generated_text"][-1][
            "content"
        ]
