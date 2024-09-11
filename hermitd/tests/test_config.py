import logging
import os
import pytest
import hermitd.config as config


def test_parse_bad_log_level():
    with pytest.raises(ValueError):
        config._parse_log_level("???")


def test_read_config_file():
    cfg = config.read_config(
        os.path.join(os.path.dirname(__file__), "resources", "hermitd.conf")
    )
    assert isinstance(cfg, config.Config)


@pytest.mark.parametrize(
    "config_data, expected",
    [({"llm": "bad llm tag"}, config.Config(llm=None, log_level=logging.INFO))],
)
def test_read_bad_config(config_data, expected):
    cfg = config._read_config(config_data)
    assert cfg == expected
