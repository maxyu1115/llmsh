# llmsh
llmsh is a shell wrapper, designed to work on top of your existing shell (bash, zsh, etc), and integrate seeminglessly with a LLM assistant. 

## How to use
After llmsh is installed, run `llmsh` to use it with `$SHELL`, or run like `llmsh bash` to run over bash.

**Currently only bash is tested.**

## Developers: Getting Started
The llmsh part is developed using rust. Install using the `rustup` toolchain: https://www.rust-lang.org/learn/get-started. This will automatically install the newest rust version.
- `rustup install 1.79.0` to install rust 1.79.
- `cargo build` to build the llmsh
- `cargo fmt` to run the formatter
