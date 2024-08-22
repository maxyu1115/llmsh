# hermitd
`hermitd` is the hermit crab living inside the shell, and just like a hermit, can give you useful suggestions :D

In all seriousness, `hermitd` is your llm-powered assistant/copilot to use with your shell. 

**NOTE**: `hermitd` is designed to be used along with `llmsh`, where `llmsh` wraps a shell of your choice (bash, csh, etc).

## Installation
You can install hermitd directly using 
```shell
pip install hermitd
```
Then you can install hermitd as a systemd service using
```shell
sudo hermitd-install
```

NOTE: as an alternative you can run directly using `OPENAI_API_KEY=xxx python3 hermitd -c <config-file>` or `ANTHROPIC_API_KEY=xxx python3 hermitd -c <config-file>`.
