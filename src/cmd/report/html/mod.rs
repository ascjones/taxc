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
    use super::generate_html;
    use crate::cmd::filter::EventFilter;
    use crate::core::{calculate_cgt, read_transactions_json, transactions_to_events};

    #[test]
    fn generate_html_keeps_hostile_strings_inside_the_data() {
        let doc = r#"{"assets":[{"symbol":"BTC","asset_class":"Crypto"}],"transactions":[
            {"id":"</script><!--","datetime":"2024-01-15T10:00:00Z","account":"k\u2028","type":"Trade",
             "description":"</SCRIPT x><script>window.pwned = true</script>",
             "sold":{"asset":"GBP","quantity":100},"bought":{"asset":"BTC","quantity":1}}]}"#;
        let (txs, registry) = read_transactions_json(doc.as_bytes()).unwrap();
        let events = transactions_to_events(&txs, &registry, Default::default()).unwrap();
        let filter = EventFilter {
            from: None,
            to: None,
            asset: None,
            event_kind: None,
        };
        let html = generate_html(&txs, &events, &calculate_cgt(events.clone()), &filter).unwrap();

        let lower = html.to_lowercase();
        assert_eq!(
            lower.matches("</script").count(),
            1,
            "only the template's own"
        );
        assert!(!html.contains("<!--"));
        assert!(!html.contains('\u{2028}'));
        // The strings survive intact inside the data.
        let data_line = html
            .lines()
            .find(|l| l.starts_with("const DATA = "))
            .unwrap();
        let json = data_line
            .trim_start_matches("const DATA = ")
            .trim_end_matches(';');
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            data["transactions"][0]["description"],
            "</SCRIPT x><script>window.pwned = true</script>"
        );
    }
}
