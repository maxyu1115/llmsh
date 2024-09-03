import pytest
from hermitd.bot import find_all, parse_code_md


def test_find_all_no_overlap():
    result = find_all("ababa", "aba")
    assert result == [0]


def test_find_all_with_overlap():
    result = find_all("ababa", "aba", overlap=True)
    assert result == [0, 2]


def test_find_all_no_match():
    result = find_all("ababa", "xyz")
    assert result == []


def test_find_all_empty_string():
    result = find_all("", "a")
    assert result == []


def test_find_all_empty_substring():
    with pytest.raises(ValueError):
        find_all("ababa", "")


def test_find_all_full_match():
    result = find_all("aaaa", "aa")
    assert result == [0, 2]


def test_parse_code_md_single_code_block():
    response = """\
Some text
```python
print('hello world')
```
More text"""
    result = parse_code_md(response)
    assert result == ["print('hello world')"]


def test_parse_code_md_multiple_code_blocks():
    response = """\
Text
```python
print('hello')
```
More text
```bash
echo 'world'
```
End"""
    result = parse_code_md(response)
    assert result == ["print('hello')", "echo 'world'"]


def test_parse_code_md_malformed_response():
    response = """\
Some text
```python
print('hello world')
More text"""
    with pytest.raises(ValueError):
        parse_code_md(response)


def test_parse_code_md_no_code_block():
    response = "Some text with no code blocks"
    result = parse_code_md(response)
    assert result == []


def test_parse_code_md_code_block_with_header():
    response = """\
```python
# Comment
print('test')
```
"""
    result = parse_code_md(response)
    assert result == ["# Comment\nprint('test')"]
