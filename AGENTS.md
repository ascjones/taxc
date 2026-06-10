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

## Knowledge Base

- `docs/solutions/` — documented solutions to past problems (bugs, integration issues, patterns), organized by category with YAML frontmatter (`module`, `tags`, `problem_type`). Relevant when implementing or debugging in documented areas.
- `CONCEPTS.md` — shared domain vocabulary (entities, named processes, status concepts). Relevant when orienting to the codebase or discussing domain terms.

## Schemas

The `schema/*.json` files are auto-generated via `cargo run -- schema` and should not be manually edited.

## Testing

Always use TDD: write failing tests first, then implement to make them pass.

### Where tests live

The suite follows a pyramid — keep domain logic at the lowest layer that can exercise it:

- **`src/core/` unit tests** — pure domain logic (CGT matching, tax-event conversion, tax-year/rate math). The bulk of coverage belongs here.
- **`src/cmd/` in-process tests** — aggregation/filtering/report-shaping tested by calling functions like `build_report_data` directly (no process spawn). Prefer this tier over E2E for anything that isn't strictly CLI wiring.
- **`tests/` integration tests** — CLI argument wiring, stdout/JSON formatting, and HTML/JS rendering, exercised through the compiled binary. Some figures are deliberately re-verified end-to-end (defense-in-depth for a tax tool); that redundancy is intentional.

### Conventions

- **Test module placement:** small modules keep an inline `#[cfg(test)] mod tests { ... }`; modules with a large test suite use a sibling `#[cfg(test)] mod tests;` in a `tests.rs` file.
- **Shared fixtures:** build `TaxableEvent`s via `core::events::builders` (`acq`, `disp`, `staking`, `event`); override fields with struct-update syntax (`TaxableEvent { id: 2, ..disp(...) }`). Integration tests run the binary via `tests/common::run_taxc` (uses the prebuilt `CARGO_BIN_EXE_taxc`, not `cargo run`) — never spawn `cargo run` directly.
- **Naming:** `feature_condition_outcome` (e.g. `bnb_matches_across_tax_year_boundary`).
- **Decoupling:** assert observable behaviour through public APIs, not private helpers or exact internal formatting.

Run tests and linting before committing:
```bash
cargo test
cargo clippy
```
