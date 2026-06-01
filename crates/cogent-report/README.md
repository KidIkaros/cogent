# cogent-report

Output formatting and report generation for Cogent.

## What's inside

- `formatters.rs` — JSON, NDJSON, SARIF, JUnit XML, and plain-text formatters
- `html.rs` — HTML and Markdown report generation with sparkline charts
- `html_escape()` — minimal HTML escaping utility

## Supported formats

| Format | Function | Notes |
|--------|----------|-------|
| JSON | `output_json` | Pretty-printed `CheckReport` |
| NDJSON | `output_ndjson` | One JSON object per finding |
| SARIF | `output_sarif` | OASIS SARIF v2.1.0 for GitHub Security tab |
| JUnit | `output_junit` | CI-friendly XML for test result parsers |
| HTML | `render_html_report` | Full interactive report with CSS |
| Markdown | `render_markdown_report` | For README / PR comment embedding |

## Usage

```rust
use cogent_report::formatters::output_json;
use cogent_common::CheckReport;

let report: CheckReport = ...;
output_json(&report);
```
