# Agent Guidelines for taxc

## CLI Documentation

**Important**: Whenever you make changes to the CLI interface (commands, options, arguments, output formats), you MUST update the README.md to reflect those changes.

Changes that require README updates include:
- Adding, removing, or renaming commands
- Adding, removing, or changing command-line options/flags
- Modifying input file formats (CSV/JSON schema)
- Changing output formats or adding new output modes
- Updating supported tax years or rates

## Code Structure

- `src/main.rs` - CLI entry point with clap command definitions
- `src/cmd/` - Command implementations
  - `summary.rs` - Tax summary command
  - `report/mod.rs` - Report command + data model
  - `report/html.rs` - HTML report generation
  - `pools.rs` - Pool history command
  - `validate.rs` - Data quality checks
  - `schema.rs` - JSON schema output
- `src/core/` - Domain logic and tax calculations (flat public surface via re-exports)
  - `events.rs` - Event types and display helpers
  - `transaction.rs` - Transaction parsing and conversion to events
  - `price.rs` - Price model and conversions
  - `cgt.rs` - Capital gains tax with HMRC share identification rules
  - `income.rs` - Income tax calculations
  - `uk.rs` - UK tax year and rate rules

### Module Preferences
- If a top-level entity has non-trivial logic or multiple associated methods, prefer moving it into its own module/file (e.g., `Price`).

### Architecture: Separate CLI from Domain Logic
- Keep CLI/IO concerns (argument parsing, file reading, stdout formatting) in `src/cmd/` and `src/main.rs`.
- Keep domain logic (tax calculations, event enrichment, matching rules) in `src/core/` as pure functions operating on data — no file IO, no CLI types.
- `src/cmd/` may depend on `src/core/`, but never the reverse.
- When adding new logic, ask: "Does this need IO or CLI context?" If no, it belongs in the domain layer.

## Schemas

The `schema/*.json` files are auto-generated via `cargo run -- schema` and should not be manually edited.

## Testing

Run tests and linting before committing:
```bash
cargo test
cargo clippy
```
