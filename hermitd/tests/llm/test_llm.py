import pytest
import hermitd.llm as llm


def test_model_id_to_string():
    model_id = llm.ModelID(llm.ModelHost.OpenAI, "max's-custom-model")
    assert str(model_id) == "openai-max's-custom-model"


def test_supported_llms():
    assert llm.SupportedLLMs.from_tag("local-llama-3") == llm.SupportedLLMs.Llama3


def test_supported_llms_not_supported():
    with pytest.raises(ValueError):
        llm.SupportedLLMs.from_tag("illegal name??")
