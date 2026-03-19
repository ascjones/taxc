---
name: report-ui
description: Generate the HTML report from a transactions file and preview it in Chrome via devtools. Checks the UI is working and suggests improvements.
argument-hint: "[transactions-file] [-- extra-flags]"
---

# Report UI

Generate an HTML report and interactively test it in the browser using Chrome DevTools.

## Steps

1. **Generate the report**
   - Run `cargo run -- report <file> --output /tmp/taxc-report-preview.html` where `<file>` is `$ARGUMENTS` if provided, otherwise `tests/data/mixed_rules.json`.
   - If the user provided extra flags (e.g. `--year 2025`), pass them through.

2. **Open in Chrome**
   - Use `mcp__chrome-devtools__new_page` to open `file:///tmp/taxc-report-preview.html`.
   - Use `mcp__chrome-devtools__take_screenshot` to capture the initial state.

3. **Smoke test the UI**
   Run these checks using `mcp__chrome-devtools__evaluate_script`:

   - **Data loaded**: `DATA` object exists and has `events` and `summary`.
   - **Summary populated**: `#summary-proceeds` contains a `£` value (not the `—` placeholder).
   - **Date range picker**: `#date-from` and `#date-to` have valid date values.
   - **Tables rendered**: At least one `.tx-row` in `#transactions-body`.
   - **Tab switching**: Click the Events tab, verify `#events-section` is visible and has rows.
   - **Date panel**: Call `toggleDatePanel()` and verify `.date-panel.open` exists. Then call `closeDatePanel()`.
   - **No JS errors**: Use `mcp__chrome-devtools__list_console_messages` and check for errors.

   Report pass/fail for each check.

4. **Take final screenshots**
   - Screenshot the transactions tab.
   - Open the date panel (`toggleDatePanel()`), screenshot it, then close it.
   - Switch to events tab, screenshot it.

5. **Suggest UI improvements**
   Based on the screenshots and the current state of the CSS/HTML/JS, use the `frontend-design:frontend-design` skill's design thinking to suggest 3-5 concrete, actionable UI improvements. Consider:
   - Visual hierarchy and information density
   - Micro-interactions and hover states
   - Typography and spacing refinements
   - Color usage and contrast
   - Mobile responsiveness
   - Any visual rough edges visible in the screenshots

   Present suggestions as a prioritized list with brief rationale for each.
