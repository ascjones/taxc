# Refactoring Learnings

- Avoid lookup keys based on non-unique fields (e.g., `description` or `(date, asset)`); prefer `id` and fallback to composite keys that include `datetime`.
- When CGT matching can link to non-trade acquisitions (staking/gifts), events views should link those rows to keep UI consistent with tax logic.
- Centralize display mapping for `(EventType, Label)` to prevent drift between CLI and HTML views.
- Add regression tests for collision scenarios (multiple disposals on same day, duplicate descriptions) to keep report linkage correct.
- Keep JS/HTML logic separated from data binding for readability; embedding a single JS template string simplifies maintenance.
