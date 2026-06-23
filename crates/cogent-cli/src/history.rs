//! History tracking and reporting.

#![deny(clippy::all)]

/// History command dispatcher.
pub fn history_command(
    action: &str,
    dir: &str,
    last: usize,
    report_path: Option<&str>,
    format: &str,
) -> i32 {
    match action {
        "record" => history_record(dir, report_path),
        "show" => {
            if format == "html" {
                history_html(dir, last)
            } else {
                history_show(dir, last)
            }
        }
        _ => history_show(dir, last),
    }
}

fn history_record(dir: &str, report_path: Option<&str>) -> i32 {
    use std::io::Read;

    let json_str = if let Some(path) = report_path {
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("history record: cannot read {}: {}", path, e);
                return 1;
            }
        }
    } else {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            eprintln!("history record: failed to read stdin");
            return 1;
        }
        buf
    };

    let report: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("history record: invalid JSON: {}", e);
            return 1;
        }
    };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = chrono_yymm(ts);

    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("history record: cannot create {}: {}", dir, e);
        return 1;
    }

    let path = format!("{}/{}.jsonl", dir, date);
    let tools_summary: serde_json::Value = report
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            let mut m = serde_json::Map::new();
            for t in arr {
                if let Some(name) = t.get("tool").and_then(|v| v.as_str()) {
                    m.insert(
                        name.to_string(),
                        serde_json::json!({
                            "success": t.get("success"),
                            "duration_ms": t.get("duration_ms"),
                        }),
                    );
                }
            }
            serde_json::Value::Object(m)
        })
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let record = serde_json::json!({
        "ts": ts,
        "run_id": report.get("run_id"),
        "passed": report.get("summary").and_then(|s| s.get("passed")),
        "failed": report.get("summary").and_then(|s| s.get("failed")),
        "tools": tools_summary,
    });

    let line = serde_json::to_string(&record).unwrap_or_default();
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", line)
        })
    {
        eprintln!("history record: write failed: {}", e);
        return 1;
    }

    eprintln!("history: recorded run to {}", path);
    0
}

fn history_show(dir: &str, last: usize) -> i32 {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            println!("No history found in {}", dir);
            return 0;
        }
    };

    let mut lines: Vec<String> = Vec::new();
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in &files {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for line in content.lines() {
                lines.push(line.to_string());
            }
        }
    }

    let show: Vec<&String> = lines.iter().rev().take(last).collect();
    if show.is_empty() {
        println!("No history records found.");
        return 0;
    }

    println!("\n{:<20} {:>6} {:>6}  TOOLS", "TIMESTAMP", "PASS", "FAIL");
    println!("{}", "─".repeat(70));
    for raw in show.iter().rev() {
        if let Ok(rec) = serde_json::from_str::<serde_json::Value>(raw) {
            let ts = rec.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
            let passed = rec.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
            let failed = rec.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
            let tools_str = rec
                .get("tools")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            let ok = v.get("success").and_then(|b| b.as_bool()).unwrap_or(false);
                            format!("{}:{}", k, if ok { "✓" } else { "✗" })
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                })
                .unwrap_or_default();
            println!(
                "{:<20} {:>6} {:>6}  {}",
                format_ts(ts),
                passed,
                failed,
                tools_str
            );
        }
    }
    println!();
    0
}

fn history_html(dir: &str, last: usize) -> i32 {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            println!("No history found in {}", dir);
            return 0;
        }
    };

    let mut records: Vec<(u64, u64, u64, u32)> = Vec::new();
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in &files {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for line in content.lines() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    let passed = json.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
                    let failed = json.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                    let ts = json.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                    let total = passed + failed;
                    let score = passed.checked_div(total).map_or(100, |r| (r * 100) as u32);
                    records.push((ts, passed, failed, score));
                }
            }
        }
    }

    records.sort_by_key(|(ts, _, _, _)| *ts);
    let mut recent: Vec<_> = records.into_iter().rev().take(last).collect();
    recent.reverse();

    if recent.is_empty() {
        println!("No history records found.");
        return 0;
    }

    let scores: Vec<u32> = recent.iter().map(|(_, _, _, s)| *s).collect();
    let sparkline = cogent_report::html::sparkline_svg(&scores, 600, 120);
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let mut table_rows = String::new();
    for (ts, passed, failed, score) in &recent {
        let trend = if recent.len() > 1 {
            let idx = recent
                .iter()
                .position(|(t, _, _, _)| *t == *ts)
                .unwrap_or(0);
            if idx > 0 {
                let prev = recent[idx - 1].3;
                if *score > prev {
                    "<span style=\"color:var(--green)\">↑</span>"
                } else if *score < prev {
                    "<span style=\"color:var(--red)\">↓</span>"
                } else {
                    "<span style=\"color:var(--text-muted)\">→</span>"
                }
            } else {
                ""
            }
        } else {
            ""
        };
        table_rows.push_str(&format!(
            "<tr style=\"border-bottom:1px solid var(--border-light)\"><td style=\"padding:10px 14px\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}</td><td style=\"padding:10px 14px;text-align:center;color:var(--green)\">{}</td><td style=\"padding:10px 14px;text-align:center;color:var(--red)\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}/100 {}</td></tr>",
            format_ts(*ts), recent.iter().position(|(t, _, _, _)| *t == *ts).map(|i| i + 1).unwrap_or(0), passed, failed, score, trend
        ));
    }

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cogent History Trend</title>
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link href="https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;500;700;800&family=JetBrains+Mono:wght@400;600&display=swap" rel="stylesheet">
<style>
:root{{
  --bg:#f8fafc;--surface:#fff;--surface-alt:#f1f5f9;--border:#e2e8f0;--border-light:#f1f5f9;
  --text:#0f172a;--text-secondary:#475569;--text-muted:#94a3b8;
  --accent:#6366f1;--green:#22c55e;--red:#ef4444;
  --shadow-sm:0 1px 2px rgba(0,0,0,.04),0 1px 3px rgba(0,0,0,.06);
}}
[data-theme="dark"]{{
  --bg:#0c1222;--surface:#131b2e;--surface-alt:#1a2340;--border:#1e293b;--border-light:#1e293b;
  --text:#f1f5f9;--text-secondary:#94a3b8;--text-muted:#64748b;
  --accent:#818cf8;--green:#34d399;--red:#f87171;
  --shadow-sm:0 1px 3px rgba(0,0,0,.3);
}}
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif;background:var(--bg);color:var(--text);line-height:1.5;padding:40px;max-width:900px;margin:0 auto;-webkit-font-smoothing:antialiased}}
.card{{background:var(--surface);border-radius:14px;padding:28px;box-shadow:var(--shadow-sm);margin-bottom:28px;border:1px solid var(--border-light)}}
.card-title{{font-size:15px;font-weight:700;color:var(--text);margin-bottom:16px}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
th{{padding:8px 14px;text-align:center;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600;border-bottom:2px solid var(--border)}}
.footer{{font-size:12px;color:var(--text-muted);text-align:center;margin-top:48px}}
.theme-toggle{{position:fixed;top:16px;right:16px;padding:8px 16px;border-radius:8px;background:var(--surface);border:1px solid var(--border);cursor:pointer;font-size:12px;color:var(--text-muted);transition:all .15s}}
.theme-toggle:hover{{color:var(--text);border-color:var(--accent)}}
</style>
</head>
<body>
<button class="theme-toggle" onclick="toggleTheme()">◐ Theme</button>
<h1 style="font-size:22px;font-weight:800;margin-bottom:8px">Cogent History Trend</h1>
<p style="color:var(--text-secondary);font-size:13px;margin-bottom:28px">{} &nbsp;&middot;&nbsp; {} runs</p>

<div class="card">
  <div class="card-title">Health Score Over Time</div>
  {}
</div>

<div class="card">
  <div class="card-title">Run Details</div>
  <table>
    <thead><tr><th style="text-align:left">Date</th><th>#</th><th>Passed</th><th>Failed</th><th>Score</th></tr></thead>
    <tbody>{}</tbody>
  </table>
</div>

<div class="footer">Generated by Cogent — {}</div>
<script>
function toggleTheme(){{
  const h=document.documentElement;
  const d=h.getAttribute('data-theme')==='dark';
  h.setAttribute('data-theme',d?'':'dark');
  localStorage.setItem('cogent-theme',d?'':'dark');
}}
(function(){{const s=localStorage.getItem('cogent-theme');if(s==='dark')document.documentElement.setAttribute('data-theme','dark');}})();
</script>
</body>
</html>"##,
        dir,
        recent.len(),
        sparkline,
        table_rows,
        date
    );
    println!("{}", html);
    0
}

fn chrono_yymm(ts: u64) -> String {
    let secs = ts % (365 * 24 * 3600);
    let _ = secs;
    let d = std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts);
    if let Ok(dur) = d.duration_since(std::time::UNIX_EPOCH) {
        let days = dur.as_secs() / 86400;
        let year = 1970 + days / 365;
        let month = (days % 365) / 30 + 1;
        return format!("{}-{:02}", year, month);
    }
    "unknown".to_string()
}

fn format_ts(ts: u64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    let month = (days % 365) / 30 + 1;
    let day = (days % 365) % 30 + 1;
    let h = (ts % 86400) / 3600;
    let m = (ts % 3600) / 60;
    format!("{}-{:02}-{:02} {:02}:{:02}", year, month, day, h, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── chrono_yymm ──

    #[test]
    fn test_chrono_yymm_epoch() {
        // 1970-01-01
        assert_eq!(chrono_yymm(0), "1970-01");
    }

    #[test]
    fn test_chrono_yymm_typical() {
        // 2025-06-01 = seconds from 1970-01-01
        // days = 55 * 365 + 13 leap days ≈ 20088
        // 20088 * 86400 = 1735603200 (roughly June 2025)
        let ts = 1748736000; // 2025-06-01 00:00:00 UTC
        assert_eq!(chrono_yymm(ts), "2025-06");
    }

    #[test]
    fn test_chrono_yymm_january() {
        // 2024-01-15 00:00:00 UTC
        let ts = 1705276800;
        assert_eq!(chrono_yymm(ts), "2024-01");
    }

    #[test]
    fn test_chrono_yymm_december() {
        // 2023-12-01 00:00:00 UTC
        let ts = 1701388800;
        assert_eq!(chrono_yymm(ts), "2023-12");
    }

    #[test]
    fn test_chrono_yymm_far_future() {
        // 2070-01-01
        let ts = 3155760000;
        let result = chrono_yymm(ts);
        assert!(result.starts_with("2070-"));
    }

    // ── format_ts ──

    #[test]
    fn test_format_ts_epoch() {
        assert_eq!(format_ts(0), "1970-01-01 00:00");
    }

    #[test]
    fn test_format_ts_typical() {
        // Function uses approximate math: days/365, months/30, no leap years
        // ts=1718461800 → days=19889, year=2024, month=6 (179/30+1), day=30 (179%30+1), h=14, m=30
        let ts = 1718461800;
        let result = format_ts(ts);
        assert_eq!(result, "2024-06-30 14:30");
    }

    #[test]
    fn test_format_ts_midnight() {
        // ts=1735689600 → days=20089, year=2025, month=1 (14/30+1), day=15 (14%30+1), h=0, m=0
        let ts = 1735689600;
        assert_eq!(format_ts(ts), "2025-01-15 00:00");
    }

    #[test]
    fn test_format_ts_end_of_month() {
        // ts=1677628740 → days=19416, year=2023, month=3 (71/30+1), day=12 (71%30+1), h=23, m=59
        let ts = 1677628740;
        let result = format_ts(ts);
        assert_eq!(result, "2023-03-12 23:59");
    }

    #[test]
    fn test_format_ts_one_second() {
        assert_eq!(format_ts(1), "1970-01-01 00:00");
    }

    #[test]
    fn test_format_ts_far_future() {
        let ts = 4102444800; // 2100-01-01
        let result = format_ts(ts);
        assert!(result.starts_with("2100-"));
    }

    #[test]
    fn test_chrono_yymm_vs_format_ts_midnight() {
        // chrono_yymm("2024-06-15") should match format_ts("2024-06-15...")
        let ts = 1718409600; // 2024-06-15 00:00 UTC
        let yymm = chrono_yymm(ts);
        let fmt = format_ts(ts);
        assert!(
            fmt.starts_with(&format!("{}-", yymm)),
            "chrono_yymm({})={} should match format_ts prefix",
            ts,
            yymm
        );
    }
}

// ── history_record ──

#[test]
fn test_history_record_writes_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let report_json = serde_json::json!({
        "run_id": "test-run",
        "summary": {"passed": 5, "failed": 1},
        "tools": [],
    });
    let report_path = dir.path().join("report.json");
    std::fs::write(&report_path, report_json.to_string()).expect("write");

    let code = history_record(
        dir.path().to_str().unwrap(),
        Some(report_path.to_str().unwrap()),
    );
    assert_eq!(code, 0, "history_record should succeed");

    // Should have created a .jsonl file
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    assert!(!entries.is_empty(), "should create a .jsonl file");

    // The JSONL file should contain valid JSON
    let content = std::fs::read_to_string(entries[0].path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(parsed["passed"], 5);
    assert_eq!(parsed["failed"], 1);
}

#[test]
fn test_history_record_missing_file_returns_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_record(
        dir.path().to_str().unwrap(),
        Some("/tmp/nonexistent-report-12345.json"),
    );
    assert_eq!(code, 1, "missing file should return 1");
}

#[test]
fn test_history_record_invalid_json_returns_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad_path = dir.path().join("bad.json");
    std::fs::write(&bad_path, "not valid json").unwrap();
    let code = history_record(
        dir.path().to_str().unwrap(),
        Some(bad_path.to_str().unwrap()),
    );
    assert_eq!(code, 1, "invalid JSON should return 1");
}

// ── history_show ──

#[test]
fn test_history_show_empty_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_show(dir.path().to_str().unwrap(), 10);
    assert_eq!(code, 0, "empty dir should return 0");
}

#[test]
fn test_history_show_with_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Create a mock history file
    let record = serde_json::json!({
        "ts": 1718409600,
        "passed": 10,
        "failed": 2,
        "tools": {}
    });
    let history_path = dir.path().join("2024-06.jsonl");
    std::fs::write(&history_path, format!("{}\n", record)).unwrap();

    let code = history_show(dir.path().to_str().unwrap(), 10);
    assert_eq!(code, 0, "history_show should return 0");
}

#[test]
fn test_history_show_filters_non_jsonl() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Create a .txt file — should be ignored
    std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();
    let code = history_show(dir.path().to_str().unwrap(), 10);
    assert_eq!(code, 0, "should handle non-jsonl files gracefully");
}

#[test]
fn test_history_show_respects_last_param() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Create 5 records in the file
    let mut content = String::new();
    for i in 0..5u64 {
        let record = serde_json::json!({
            "ts": 1718409600 + i * 86400,
            "passed": 10,
            "failed": i,
            "tools": {}
        });
        content.push_str(&format!("{}\n", record));
    }
    let history_path = dir.path().join("2024-06.jsonl");
    std::fs::write(&history_path, content).unwrap();

    // Should not crash with any last value
    assert_eq!(history_show(dir.path().to_str().unwrap(), 3), 0);
    assert_eq!(history_show(dir.path().to_str().unwrap(), 0), 0);
}

// ── history_html ──

#[test]
fn test_history_html_empty_dir_returns_0() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_html(dir.path().to_str().unwrap(), 10);
    assert_eq!(code, 0);
}

#[test]
fn test_history_html_with_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let record = serde_json::json!({
        "ts": 1718409600,
        "passed": 8,
        "failed": 2,
        "tools": {
            "crap": {"success": true, "duration_ms": 1500},
            "debt": {"success": false, "duration_ms": 800},
        }
    });
    let history_path = dir.path().join("2024-06.jsonl");
    std::fs::write(&history_path, format!("{}\n", record)).unwrap();

    let code = history_html(dir.path().to_str().unwrap(), 10);
    assert_eq!(code, 0, "history_html should succeed");
}

#[test]
fn test_history_html_multiple_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut content = String::new();
    for i in 0..3u64 {
        let record = serde_json::json!({
            "ts": 1718409600 + i * 86400,
            "passed": 10,
            "failed": i,
            "tools": {}
        });
        content.push_str(&format!("{}\n", record));
    }
    std::fs::write(dir.path().join("2024-06.jsonl"), content).unwrap();

    let code = history_html(dir.path().to_str().unwrap(), 5);
    assert_eq!(code, 0);
}

#[test]
fn test_history_html_respects_last_param() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut content = String::new();
    for i in 0..10u64 {
        let record = serde_json::json!({
            "ts": 1718409600 + i * 86400,
            "passed": 10,
            "failed": 0,
            "tools": {}
        });
        content.push_str(&format!("{}\n", record));
    }
    std::fs::write(dir.path().join("2024-06.jsonl"), content).unwrap();

    // last=3 should work without issues
    let code = history_html(dir.path().to_str().unwrap(), 3);
    assert_eq!(code, 0);
}

// ── history_command ──

#[test]
fn test_history_command_unknown_action_defaults_to_show() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_command("foobar", dir.path().to_str().unwrap(), 10, None, "text");
    assert_eq!(code, 0);
}

#[test]
fn test_history_command_html_format() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_command("show", dir.path().to_str().unwrap(), 10, None, "html");
    assert_eq!(code, 0);
}

#[test]
fn test_history_command_record_without_report() {
    // No report_path and stdin not available → will fail
    let dir = tempfile::tempdir().expect("tempdir");
    let code = history_command("record", dir.path().to_str().unwrap(), 10, None, "text");
    assert_eq!(code, 1, "record without stdin should fail");
}
