# Agent Guidelines for taxc

## CLI Documentation

**Important**: Whenever you make changes to the CLI interface (commands, options, arguments, output formats), you MUST update the README.md to reflect those changes.

Changes that require README updates include:
- Adding, removing, or renaming commands
- Adding, removing, or changing command-line options/flags
- Modifying input file formats (CSV/JSON schema)
- Changing output formats or adding new output modes
- Updating supported tax years or rates

## Code Navigation

Use the LSP tool (rust-analyzer) for code navigation and understanding. Prefer LSP over grep/glob when:
- Finding where a type, function, or trait is defined (`goToDefinition`)
- Finding all usages of a symbol (`findReferences`)
- Understanding a symbol's type or documentation (`hover`)
- Listing symbols in a file (`documentSymbol`) or workspace (`workspaceSymbol`)
- Tracing call chains (`incomingCalls`, `outgoingCalls`)

Load the LSP tool at the start of each session with `select:LSP` via ToolSearch.

## Architecture

- `src/cmd/` depends on `src/core/`, never the reverse.
- `src/cmd/` owns CLI/IO concerns (argument parsing, file reading, stdout formatting).
- `src/core/` owns domain logic as pure functions — no file IO, no CLI types.
- If a top-level entity has non-trivial logic, prefer its own module/file.

## Schemas

The `schema/*.json` files are auto-generated via `cargo run -- schema` and should not be manually edited.

## Testing

Always use TDD: write failing tests first, then implement to make them pass.

Run tests and linting before committing:
```bash
cargo test
cargo clippy
```
