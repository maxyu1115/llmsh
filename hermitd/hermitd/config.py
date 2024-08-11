import dataclasses
import os
import yaml
from hermitd.llm import SupportedLLMs


@dataclasses.dataclass
class Config:
    llm: SupportedLLMs | None


def _read_config(loaded_yaml):
    llm_tag = loaded_yaml.get("llm", "")
    try:
        llm = SupportedLLMs.from_tag(llm_tag)
    except ValueError:
        llm = None

    return Config(llm=llm)


# Wrapper function for _read_config, so that it's easier to test
def read_config(config_path):
    with open(config_path) as f:
        conf = yaml.safe_load(f.read())
    return _read_config(conf)


@dataclasses.dataclass
class Secrets:
    anthropic: str | None
    openai: str | None


def read_api_keys() -> Secrets:
    anthropic_api_key = os.environ.get("ANTHROPIC_API_KEY")
    openai_api_key = os.environ.get("OPENAI_API_KEY")
    return Secrets(anthropic=anthropic_api_key, openai=openai_api_key)
