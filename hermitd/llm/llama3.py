from typing import Optional, Iterator
from vllm.engine.llm_engine import LLMEngine
from vllm.engine.arg_utils import EngineArgs
from vllm.usage.usage_lib import UsageContext
from vllm.utils import Counter
from vllm.outputs import RequestOutput
from vllm import SamplingParams
from transformers import PreTrainedTokenizer, PreTrainedTokenizerFast
from llm.interfaces import LLM


class StreamingLLM:
    def __init__(
        self,
        model: str,
        dtype: str = "auto",
        quantization: Optional[str] = None,
        **kwargs,
    ) -> None:
        engine_args = EngineArgs(
            model=model,
            quantization=quantization,
            dtype=dtype,
            enforce_eager=True,
            max_model_len=2048,
        )
        self.llm_engine = LLMEngine.from_engine_args(
            engine_args, usage_context=UsageContext.LLM_CLASS
        )
        self.request_counter = Counter()

    def generate(
        self,
        prompt: Optional[str] = None,
        sampling_params: Optional[SamplingParams] = None,
    ) -> Iterator[RequestOutput]:

        request_id = str(next(self.request_counter))
        self.llm_engine.add_request(request_id, prompt, sampling_params)

        while self.llm_engine.has_unfinished_requests():
            step_outputs = self.llm_engine.step()
            for output in step_outputs:
                yield output


class Llama3(LLM):
    def __init__(self) -> None:
        self.llm = StreamingLLM(
            model="casperhansen/llama-3-8b-instruct-awq",
            quantization="AWQ",
            dtype="float16",
        )
        # llm = StreamingLLM(model="casperhansen/llama-3-70b-instruct-awq", quantization="AWQ", dtype="float16")
        self.tokenizer = self.llm.llm_engine.tokenizer.tokenizer
        self.sampling_params = SamplingParams(
            temperature=0.6,
            top_p=0.9,
            max_tokens=4096,
            stop_token_ids=[
                self.tokenizer.eos_token_id,
                self.tokenizer.convert_tokens_to_ids("<|eot_id|>"),
            ],
        )

    def generate(
        self, message: str, history: list[tuple[str, str]] = None, header: str = None
    ) -> str:
        history_chat_format = []
        history = history or []
        if header:
            history_chat_format.append({"role": "system", "content": header})

        for human, assistant in history:
            history_chat_format.append({"role": "user", "content": human})
            history_chat_format.append({"role": "assistant", "content": assistant})
        history_chat_format.append({"role": "user", "content": message})

        prompt = self.tokenizer.apply_chat_template(history_chat_format, tokenize=False)

        response = ""
        for chunk in self.llm.generate(prompt, self.sampling_params):
            response = chunk.outputs[0].text
        return response
