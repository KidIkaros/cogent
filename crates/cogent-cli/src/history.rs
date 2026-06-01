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
                    "<span style=\"color:#22c55e\">↑</span>"
                } else if *score < prev {
                    "<span style=\"color:#ef4444\">↓</span>"
                } else {
                    "<span style=\"color:#9ca3af\">→</span>"
                }
            } else {
                ""
            }
        } else {
            ""
        };
        table_rows.push_str(&format!(
            "<tr style=\"border-bottom:1px solid #f3f4f6\"><td style=\"padding:10px 14px\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}</td><td style=\"padding:10px 14px;text-align:center;color:#22c55e\">{}</td><td style=\"padding:10px 14px;text-align:center;color:#ef4444\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}/100 {}</td></tr>",
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
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;line-height:1.5;padding:40px;max-width:900px;margin:0 auto}}
.card{{background:#fff;border-radius:12px;padding:28px;box-shadow:0 1px 3px rgba(0,0,0,.06);margin-bottom:28px;border:1px solid #f1f5f9}}
.card-title{{font-size:15px;font-weight:700;color:#0f172a;margin-bottom:16px}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
th{{padding:8px 14px;text-align:center;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;border-bottom:2px solid #e5e7eb}}
.footer{{font-size:12px;color:#94a3b8;text-align:center;margin-top:48px}}
</style>
</head>
<body>
<h1 style="font-size:22px;font-weight:800;margin-bottom:8px">Cogent History Trend</h1>
<p style="color:#6b7280;font-size:13px;margin-bottom:28px">{} &nbsp;&middot;&nbsp; {} runs</p>

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
