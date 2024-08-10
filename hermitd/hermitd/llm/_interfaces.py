class LLM:
    def generate(self, message: str, prompt: str, header: str = None) -> str:
        raise NotImplementedError()


class LLMFactory:
    def get_llm(self) -> LLM:
        raise NotImplementedError()


class SingletonLLMFactory(LLMFactory):
    def __init__(self, llm: LLM) -> None:
        self.llm = llm

    def get_llm(self) -> LLM:
        return self.llm
