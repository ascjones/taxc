---
name: report-ui
description: Generate the HTML report from a transactions file and preview it in Chrome. Screenshots the main UI states and suggests design improvements.
argument-hint: "[transactions-file] [-- extra-flags]"
---

# Report UI

Generate an HTML report, look at it in a real browser, and critique the design.

Functional correctness is NOT this skill's job: `tests/html_report.rs` asserts
that the report renders, tabs switch, the date panel opens, filters and sorting
work, etc., and runs in CI. Do not re-check those here. This skill exists for
what only a human-style look can give: screenshots and a design critique.

## Steps

1. **Generate the report**
   - Run `cargo run -- report <file> --output /tmp/taxc-report-preview.html` where `<file>` is `$ARGUMENTS` if provided, otherwise `tests/data/mixed_rules.json`.
   - If the user provided extra flags (e.g. `--year 2025`), pass them through.

2. **Open in Chrome**
   - Load the `claude-in-chrome` skill to get the browser automation tools.
   - Open `file:///tmp/taxc-report-preview.html` in a new tab. If the browser can't open `file://` URLs, serve it instead: `./scripts/serve-report.sh` serves `/tmp` at `http://localhost:8765/`, then open `http://localhost:8765/taxc-report-preview.html`.
   - Read the browser console; if there are JS errors, stop and report them — that is a bug, not a design issue.

3. **Screenshot the three states**
   - Transactions tab (the initial view).
   - Date panel open: call `toggleDatePanel()`, screenshot, then `closeDatePanel()`.
   - Events tab: click the Events tab and screenshot.

4. **Suggest UI improvements**
   Based on the screenshots and the current state of the CSS/HTML/JS, use the `frontend-design:frontend-design` skill's design thinking to suggest 3-5 concrete, actionable UI improvements. Consider:
   - Visual hierarchy and information density
   - Micro-interactions and hover states
   - Typography and spacing refinements
   - Color usage and contrast
   - Mobile responsiveness
   - Any visual rough edges visible in the screenshots

   Present suggestions as a prioritized list with brief rationale for each.
