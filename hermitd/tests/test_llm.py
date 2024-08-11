import pytest
import hermitd.llm as llm


def test_supported_llms():
    assert llm.SupportedLLMs.from_tag("local-llama-3") == llm.SupportedLLMs.Llama3

    with pytest.raises(ValueError) as err:
        llm.SupportedLLMs.from_tag("illegal name??")
