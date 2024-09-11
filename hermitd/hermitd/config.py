import dataclasses
import logging
import os
from typing import Optional
import yaml
from hermitd.llm import SupportedLLMs


@dataclasses.dataclass
class Config:
    llm: Optional[SupportedLLMs]
    log_level: int


LOG_LEVELS = {
    "debug": logging.DEBUG,
    "info": logging.INFO,
    "warning": logging.WARNING,
    "error": logging.ERROR,
    "critical": logging.CRITICAL,
}


def _parse_log_level(log_level_str: str) -> int:
    if log_level_str not in LOG_LEVELS:
        raise ValueError(f"Unsupported log_level '{log_level_str}'.")
    return LOG_LEVELS[log_level_str]


def _read_config(loaded_yaml):
    llm_tag = loaded_yaml.get("llm", "")
    try:
        llm = SupportedLLMs.from_tag(llm_tag)
    except ValueError:
        llm = None

    # Defaults to info
    log_level_str = loaded_yaml.get("log_level", "info")
    log_level = _parse_log_level(log_level_str)
    return Config(llm=llm, log_level=log_level)


# Wrapper function for _read_config, so that it's easier to test
def read_config(config_path):
    with open(config_path) as f:
        conf = yaml.safe_load(f.read())
    return _read_config(conf)


@dataclasses.dataclass
class Secrets:
    anthropic: Optional[str]
    openai: Optional[str]


def read_api_keys() -> Secrets:
    anthropic_api_key = os.environ.get("ANTHROPIC_API_KEY")
    openai_api_key = os.environ.get("OPENAI_API_KEY")
    return Secrets(anthropic=anthropic_api_key, openai=openai_api_key)
