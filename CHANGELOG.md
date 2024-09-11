# Change Log
## [0.2.0] - 2024-09-10
### Added
- ability to directly execute commands hermitd suggests

### Changed
- refactored output parsing, significantly improved performance for long outputs

### Fixed
- llmsh parsing state machine bug where it matched based on order of transition list instead of matching identifier position in buffer.
- llmsh bug where parsing identifier is sometimes partially displayed
- hermitd no longer prints every IPC to stdout by default. (Implemented configurable logging levels)


## [0.1.0] - 2024-08-24
First release :D
