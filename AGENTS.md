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
  - `events.rs` - Transaction view command
  - `summary.rs` - Tax summary command
  - `html_report.rs` - HTML/JSON report generation
- `src/tax/` - Tax calculation logic
  - `cgt.rs` - Capital gains tax with HMRC share identification rules
  - `income.rs` - Income tax calculations
- `src/events.rs` - Event types and parsing

### Module Preferences
- If a top-level entity has non-trivial logic or multiple associated methods, prefer moving it into its own module/file (e.g., `Price`).

## Testing

Run tests and linting before committing:
```bash
cargo test
cargo clippy
```

## Notes

Record learnings, ideas, and follow-ups in `notes/` (one file per topic or a shared backlog) so we can improve iteratively.
