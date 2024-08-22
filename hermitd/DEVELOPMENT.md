## Development Prerequisite
I recommend setting up a virtual environment using `python3 -m venv .venv`. 

Then run `pip install -r requirements.txt` to install all the needed dependencies.

In order to run LLMs locally, you'll need to install things like cuda drivers. The specifics will depend on your computer/gpu setup.

## How to Run
If you have vllm working locally, run using `python3 -m hermitd -c hermitd.conf.local.sample`.

Or you can run `OPENAI_API_KEY=xxx python3 hermitd -c hermitd.conf.remote.sample`

## Style
Remember to run `source format.sh` before commits.

## Builds
First install the latest version of `pip install --upgrade build`

From `llmsh/hermitd` you can run `pip install .` to install locally. (**Note that `sudo hermitd-install` does not work quite well with venv, in that case run with the absolute path of <project>/.venv/bin/hermitd-install**)

Command to build wheels:
```shell
python3 -m build --wheel
```
