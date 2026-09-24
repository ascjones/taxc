//! HTML report generation.

use super::build_report_data;
use crate::cmd::filter::EventFilter;
use crate::core::transactions::Transaction;
use crate::core::{CgtReport, TaxableEvent};

const TEMPLATE: &str = include_str!("report.html");
const CSS: &str = include_str!("report.css");
const JS: &str = include_str!("report.js");

/// Generate HTML report content
pub fn generate_html(
    transactions: &[Transaction],
    events: &[TaxableEvent],
    cgt_report: &CgtReport,
    filter: &EventFilter,
) -> anyhow::Result<String> {
    let data = build_report_data(transactions, events, cgt_report, filter);
    let json_data = script_safe(&serde_json::to_string(&data)?);
    let js = JS.replace("__JSON_DATA__", &json_data);

    Ok(TEMPLATE.replace("__CSS__", CSS).replace("__JS__", &js))
}

/// Make serialized JSON safe to embed in an inline `<script>` block.
///
/// The HTML parser ends a script at the first `</script`, whatever the JS
/// context, and `<!--` switches it into an escaped state; user strings such
/// as descriptions can contain either. JSON allows `\u003c` for `<` anywhere
/// in a string, and U+2028/U+2029 are escaped because they end a line in
/// older JS engines.
fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::script_safe;

    #[test]
    fn script_safe_escapes_markup_openers() {
        let json = serde_json::to_string("</script><!--\u{2028}").unwrap();
        let safe = script_safe(&json);
        assert!(!safe.contains('<'));
        assert!(!safe.contains('\u{2028}'));
        let back: String = serde_json::from_str(&safe).unwrap();
        assert_eq!(back, "</script><!--\u{2028}");
    }
}
