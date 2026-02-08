# Improvements Backlog

## High Priority
- Fix HTML report disposal lookup to avoid collisions on `description` (use `id` or stable composite key).
- Centralize event display mapping `(EventType, Label) -> display string` to avoid duplication.
- Consider making acquisition row linking include non-Trade acquisitions (e.g., staking) when appropriate.
- Add `datetime` to `DisposalRecord` and use it in disposal matching keys for better uniqueness.

## Medium Priority
- Add tests for Gift label display in `events` output and HTML report JSON.
- Add tests for duplicate disposal descriptions in HTML report mapping.
- Add tests for multiple disposals of the same asset on the same day to verify correct CGT linkage.

## Low Priority
- Consider splitting HTML report template/JS/CSS for SRP and readability.
