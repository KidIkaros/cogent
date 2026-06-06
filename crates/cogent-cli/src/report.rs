//! Report and setup commands for cogent-cli.

#![deny(clippy::all)]

use colored::Colorize;
use crate::types::{CheckReport, CheckResult};
use crate::progress::health_score;
use crate::serve::open_in_browser;

pub(crate) fn report_command(
    path: &str,
    format: &str,
    output: Option<&str>,
    project: Option<&str>,
    from_json: Option<&str>,
    skip: Option<&str>,
    open: bool,
) -> i32 {
    // --- 1. Gather check data ---
    let check_report: CheckReport = if let Some(json_path) = from_json {
        let content = match std::fs::read_to_string(json_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading {}: {}", json_path, e);
                return 2;
            }
        };
        match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error parsing JSON: {}", e);
                return 2;
            }
        }
    } else {
        // Run cogent check internally
        eprintln!("  {} Running checks…", "·".bright_black());
        let mut args = vec![path, "--format", "json"];
        if let Some(s) = skip {
            args.push("--skip");
            args.push(s);
        }
        let output_bytes =
            std::process::Command::new(std::env::current_exe().unwrap_or_else(|_| "cogent".into()))
                .arg("check")
                .args(&args)
                .output();
        match output_bytes {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                match serde_json::from_str(&stdout) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to parse check output: {}", e);
                        return 2;
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to run checks: {}", e);
                return 2;
            }
        }
    };

    let project_name = project
        .map(|s| s.to_string())
        .or_else(|| {
            std::fs::canonicalize(path)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        })
        .unwrap_or_else(|| path.to_string());

    let now = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hh = (secs % 86400) / 3600;
        let mm = (secs % 3600) / 60;
        // Correct Gregorian calendar from Unix timestamp
        let mut days = (secs / 86400) as i64;
        let mut year = 1970i64;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let y_days = if leap { 366 } else { 365 };
            if days < y_days {
                break;
            }
            days -= y_days;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let month_days = [
            31i64,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut month = 0usize;
        for &md in &month_days {
            if days < md {
                break;
            }
            days -= md;
            month += 1;
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            year,
            month + 1,
            days + 1,
            hh,
            mm
        )
    };

    let passed = check_report.passed;
    let total = check_report.summary.total_checks;
    let passed_n = check_report.summary.passed_checks;
    let failed_n = check_report.summary.failed_checks;

    // Categorise checks by domain
    let security_tools = [
        "secrets",
        "vulnscan",
        "taint",
        "errhandle",
        "sast",
        "crypto",
    ];
    let compliance_tools = ["licenses", "sbom"];
    let quality_tools = [
        "crap",
        "debt",
        "doc_coverage",
        "complexity",
        "duplication",
        "cohesion",
        "coupling",
        "riskmap",
        "linelen",
        "halstead",
        "deadcode",
        "comments",
        "propcov",
        "fuzz",
        "typecov",
    ];

    match format {
        "markdown" | "md" => {
            let md = render_markdown_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
                "",
            );
            let out_path = output.unwrap_or("cogent-report.md");
            std::fs::write(out_path, &md).expect("Failed to write report");
            eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
            if open {
                open_in_browser(out_path);
            }
        }
        "pdf" => {
            // Render HTML first, then convert via headless Chrome/Chromium
            let html = render_html_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
            );
            let html_tmp = "/tmp/cogent-report-tmp.html";
            std::fs::write(html_tmp, &html).expect("Failed to write temp HTML");
            let pdf_path = output.unwrap_or("cogent-report.pdf");
            let browser = [
                "chromium",
                "chromium-browser",
                "google-chrome",
                "google-chrome-stable",
            ]
            .iter()
            .find(|b| {
                std::process::Command::new(b)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            })
            .copied();
            match browser {
                Some(bin) => {
                    let abs_html = std::fs::canonicalize(html_tmp)
                        .map(|p| format!("file://{}", p.display()))
                        .unwrap_or_else(|_| format!("file://{}", html_tmp));
                    let result = std::process::Command::new(bin)
                        .args([
                            "--headless",
                            "--disable-gpu",
                            "--no-sandbox",
                            &format!("--print-to-pdf={}", pdf_path),
                            &abs_html,
                        ])
                        .output();
                    match result {
                        Ok(o) if o.status.success() => {
                            eprintln!("  {} Report written to {}", "✓".green().bold(), pdf_path);
                            if open {
                                open_in_browser(pdf_path);
                            }
                        }
                        Ok(o) => {
                            let err = String::from_utf8_lossy(&o.stderr);
                            eprintln!(
                                "  {} PDF conversion failed: {}",
                                "✗".red().bold(),
                                err.lines().next().unwrap_or("unknown error")
                            );
                            eprintln!("  {} HTML saved to {}", "ℹ".cyan(), html_tmp);
                        }
                        Err(e) => eprintln!("  {} Could not run {}: {}", "✗".red().bold(), bin, e),
                    }
                }
                None => {
                    eprintln!(
                        "  {} No Chromium/Chrome found — falling back to HTML",
                        "!".yellow().bold()
                    );
                    let out_path = output.unwrap_or("cogent-report.html");
                    std::fs::write(out_path, &html).expect("Failed to write HTML report");
                    eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
                    if open {
                        open_in_browser(out_path);
                    }
                }
            }
        }
        _ => {
            let html = render_html_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
            );
            let out_path = output.unwrap_or("cogent-report.html");
            std::fs::write(out_path, &html).expect("Failed to write report");
            eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
            eprintln!(
                "  {} {} checks: {}/{} passed",
                if passed {
                    "✓".green().bold()
                } else {
                    "✗".red().bold()
                },
                total,
                passed_n,
                total
            );
            if open {
                open_in_browser(out_path);
            }
        }
    }

    let _ = (passed_n, failed_n);
    if passed {
        0
    } else {
        1
    }
}

pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn severity_color_html(sev: &str) -> &'static str {
    match sev {
        "high" | "critical" | "error" => "var(--red)",
        "medium" | "warning" => "var(--amber)",
        "low" => "var(--blue)",
        _ => "var(--text-muted)",
    }
}

pub(crate) fn severity_badge(sev: &str) -> String {
    let color = severity_color_html(sev);
    let bg = match sev {
        "high" | "critical" | "error" => "var(--red-bg)",
        "medium" | "warning" => "var(--amber-bg)",
        "low" => "var(--blue-bg)",
        _ => "var(--surface-alt)",
    };
    format!(
        r#"<span style="background:{bg};color:{c};padding:2px 8px;border-radius:12px;font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:.03em">{s}</span>"#,
        bg = bg, c = color, s = sev
    )
}

/// Build a collapsible offender list from CheckResult.details JSON
pub(crate) fn offender_rows_html(c: &CheckResult) -> String {
    let arrays = [
        "items",
        "functions",
        "findings",
        "violations",
        "secrets",
        "duplicates",
    ];
    for key in &arrays {
        if let Some(arr) = c.details.get(key).and_then(|v| v.as_array()) {
            if arr.is_empty() {
                continue;
            }
            let mut rows = String::new();
            for item in arr.iter().take(10) {
                let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("");
                let line = item
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                let desc = item
                    .get("context")
                    .or_else(|| item.get("kind"))
                    .or_else(|| item.get("name"))
                    .or_else(|| item.get("type"))
                    .or_else(|| item.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let loc = if file.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", file, line)
                };
                let desc_trunc = if desc.len() > 80 {
                    format!("{}…", &desc[..80])
                } else {
                    desc.to_string()
                };
                rows.push_str(&format!(
                    r#"<div style="display:flex;gap:12px;padding:4px 0;border-bottom:1px solid var(--offender-border);font-size:12px">
  <span style="color:var(--offender-loc);font-family:monospace;white-space:nowrap;min-width:180px">{}</span>
  <span style="color:var(--offender-desc)">{}</span>
</div>"#,
                    html_escape(&loc), html_escape(&desc_trunc)
                ));
            }
            let more = if arr.len() > 10 {
                format!(
                    r#"<div style="font-size:12px;color:var(--text-muted);padding-top:6px">… {} more findings</div>"#,
                    arr.len() - 10
                )
            } else {
                String::new()
            };
            return format!(
                r#"<details style="margin-top:8px">
<summary style="font-size:12px;color:var(--accent);cursor:pointer;user-select:none;padding:4px 0">▶ Show {} finding{}</summary>
<div style="margin-top:8px;padding:8px 12px;background:var(--offender-bg);border-radius:6px;border-left:3px solid var(--accent)">
{}{}
</div>
</details>"#,
                arr.len().min(10),
                if arr.len() == 1 { "" } else { "s" },
                rows,
                more
            );
        }
    }
    String::new()
}

pub(crate) fn check_row_html(c: &CheckResult) -> String {
    let icon = if c.passed { "&#10003;" } else { "&#10007;" };
    let icon_color = if c.passed { "var(--icon-pass)" } else { "var(--icon-fail)" };
    let row_bg = if c.passed { "var(--surface)" } else { "var(--red-bg)" };
    let name_color = if c.passed { "var(--text)" } else { "var(--red)" };
    let sev = c.severity.as_deref().unwrap_or("info");
    let help = c.help.as_deref().unwrap_or("");
    let score_str = match (c.score, c.threshold) {
        (Some(s), Some(t)) => format!("{:.1} / {:.1}", s, t),
        (Some(s), None) => format!("{:.1}", s),
        _ => "&#8212;".to_string(),
    };
    let offenders = if !c.passed {
        offender_rows_html(c)
    } else {
        String::new()
    };
    let msg_cell = format!(
        "<div style=\"font-size:13px;color:var(--text-secondary)\">{msg}</div><div style=\"font-size:11px;color:var(--text-muted);margin-top:3px\">{help}</div>{off}",
        msg = html_escape(&c.message), help = html_escape(help), off = offenders
    );
    format!(
        "<tr style=\"background:{rb};border-bottom:1px solid var(--border-light);vertical-align:top\">\n  <td style=\"padding:12px 14px;font-size:18px;color:{ic};text-align:center;width:40px;font-weight:700\">{icon}</td>\n  <td style=\"padding:12px 14px;font-weight:600;font-size:13px;color:{nc};white-space:nowrap\">{name}</td>\n  <td style=\"padding:12px 14px\">{mc}</td>\n  <td style=\"padding:12px 14px;font-size:12px;color:var(--text-muted);white-space:nowrap\">{score}</td>\n  <td style=\"padding:12px 14px;white-space:nowrap\">{sb}</td>\n</tr>",
        rb = row_bg, ic = icon_color, icon = icon, nc = name_color,
        name = c.name, mc = msg_cell, score = score_str, sb = severity_badge(sev),
    )
}

/// SVG donut ring showing pass percentage. r=44 → circumference≈276.
pub(crate) fn donut_svg(pct: f64, color: &str) -> String {
    let circ = 276.46f64;
    let dash = circ * pct / 100.0;
    let gap = circ - dash;
    let pct_int = pct as u32;
    // Build without format! to avoid Rust 2021 prefixed-literal issues with HTML
    let mut s = String::from(
        r#"<svg viewBox="0 0 100 100" width="120" height="120" style="display:block">"#,
    );
    s.push_str("\n  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" style=\"stroke:var(--border)\" stroke-width=\"10\"/>\n");
    s.push_str(&format!(
        "  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" style=\"stroke:{}\" stroke-width=\"10\"\n",
        color
    ));
    s.push_str(&format!(
        "    stroke-dasharray=\"{:.2} {:.2}\" stroke-dashoffset=\"69.12\"\n",
        dash, gap
    ));
    s.push_str("    stroke-linecap=\"round\" transform=\"rotate(-90 50 50)\"/>\n");
    s.push_str(&format!("  <text x=\"50\" y=\"46\" text-anchor=\"middle\" font-size=\"18\" font-weight=\"800\" style=\"fill:{}\" font-family=\"system-ui\">{}%</text>\n", color, pct_int));
    s.push_str("  <text x=\"50\" y=\"60\" text-anchor=\"middle\" font-size=\"9\" style=\"fill:var(--text-muted)\" font-family=\"system-ui\">pass rate</text>\n");
    s.push_str("</svg>");
    s
}

/// Inline horizontal mini-bar for a category (e.g. "6/8 ██████░░")
pub(crate) fn mini_bar(pass: usize, total: usize, color: &str) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (pass * 12) / total;
    let bar: String = "█".repeat(filled) + &"░".repeat(12 - filled);
    let pct = pass * 100 / total;
    format!(
        "<div style=\"display:flex;align-items:center;gap:8px;font-size:12px\">\n  <span style=\"font-family:monospace;color:{color};letter-spacing:.1em\">{bar}</span>\n  <span style=\"color:var(--text-muted)\">{pass}/{total} ({pct}%)</span>\n</div>",
        color = color, bar = bar, pass = pass, total = total, pct = pct
    )
}

/// Semi-circle gauge SVG for health score 0-100.
pub(crate) fn gauge_svg(score: u32, color: &str) -> String {
    let radius = 80f64;
    let cx = 100f64;
    let cy = 100f64;
    let start_angle = std::f64::consts::PI;
    let _end_angle = 0f64;
    let angle = start_angle - (score as f64 / 100.0) * start_angle;
    let needle_x = cx + radius * angle.cos();
    let needle_y = cy - radius * angle.sin();
    let mut s = String::from(
        r##"<svg viewBox="0 0 200 110" width="200" height="110" style="display:block">"##,
    );
    // Background arc
    s.push_str(&format!(
        r##"<path d="M {} {} A {} {} 0 0 1 {} {}" fill="none" style="stroke:var(--border)" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, cx + radius, cy
    ));
    // Foreground arc
    let large_arc = if score > 50 { 1 } else { 0 };
    s.push_str(&format!(
        r##"<path d="M {} {} A {} {} 0 {} 1 {} {}" fill="none" style="stroke:{}" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, large_arc, needle_x, needle_y, color
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="28" font-weight="800" style="fill:{}" font-family="system-ui">{}</text>"##,
        cx, cy + 8.0, color, score
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="10" style="fill:var(--text-muted)" font-family="system-ui">Health Score</text>"##,
        cx, cy + 24.0
    ));
    s.push_str("</svg>");
    s
}

/// Horizontal bar chart SVG for severity distribution.
pub(crate) fn severity_bar_chart_svg(counts: &[(String, usize, &str)]) -> String {
    let total: usize = counts.iter().map(|(_, c, _)| *c).sum();
    if total == 0 {
        return String::new();
    }
    let bar_h = 16usize;
    let gap = 4usize;
    let max_w = 280f64;
    let max_c = counts.iter().map(|(_, c, _)| *c).max().unwrap_or(1).max(1);
    let h = counts.len() * (bar_h + gap) + gap;
    let mut s = format!(
        r##"<svg viewBox="0 0 320 {}" width="320" height="{}" style="display:block">"##,
        h, h
    );
    for (i, (label, count, color)) in counts.iter().enumerate() {
        let y = gap + i * (bar_h + gap);
        let w = (*count as f64 / max_c as f64) * max_w;
        s.push_str(&format!(
            r##"<rect x="40" y="{}" width="{}" height="{}" rx="3" style="fill:{}"/>"##,
            y,
            w.max(2.0),
            bar_h,
            color
        ));
        s.push_str(&format!(
            r##"<text x="35" y="{}" text-anchor="end" font-size="11" style="fill:var(--text-muted)" font-family="system-ui" dy="{}">{} ({})</text>"##,
            y + bar_h / 2 + 4, 0, html_escape(label), count
        ));
    }
    s.push_str("</svg>");
    s
}

/// Sparkline SVG from a series of health scores.
pub(crate) fn sparkline_svg(scores: &[u32], width: usize, height: usize) -> String {
    if scores.len() < 2 {
        return String::new();
    }
    let max_score = scores.iter().copied().max().unwrap_or(100).max(1);
    let min_score = scores.iter().copied().min().unwrap_or(0);
    let range = (max_score - min_score).max(1) as f64;
    let n = scores.len();
    let step_x = width as f64 / (n - 1) as f64;
    let padding = 4f64;
    let plot_h = (height as f64) - padding * 2.0;

    let mut points = String::new();
    for (i, &score) in scores.iter().enumerate() {
        let x = i as f64 * step_x;
        let y = padding + plot_h - ((score - min_score) as f64 / range) * plot_h;
        if i > 0 {
            points.push(' ');
        }
        points.push_str(&format!("{:.1},{:.1}", x, y));
    }

    let mut s = format!(
        r##"<svg viewBox="0 0 {} {}" width="{}" height="{}" style="display:block">"##,
        width, height, width, height
    );
    // Grid line at 50%
    let y50 = padding + plot_h * 0.5;
    s.push_str(&format!(
        r##"<line x1="0" y1="{}" x2="{}" y2="{}" style="stroke:var(--border)" stroke-width="1" stroke-dasharray="3,3"/>"##,
        y50, width, y50
    ));
    // Polyline
    s.push_str(&format!(
        r##"<polyline points="{}" fill="none" style="stroke:var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##,
        points
    ));
    // Dots
    for (i, &score) in scores.iter().enumerate() {
        let x = i as f64 * step_x;
        let y = padding + plot_h - ((score - min_score) as f64 / range) * plot_h;
        s.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="2.5" style="fill:var(--accent)"/>"##,
            x, y
        ));
    }
    s.push_str("</svg>");
    s
}

/// Read `.cogent-history/` and return last N health scores.
pub(crate) fn history_health_scores(dir: &str, last: usize) -> Vec<u32> {
    let mut scores: Vec<(u64, u32)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".jsonl") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    for line in content.lines() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                            let passed = json.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
                            let failed = json.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                            let ts = json.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                            let total = passed + failed;
                            let score = passed.checked_div(total).map_or(100, |r| (r * 100) as u32);
                            scores.push((ts, score));
                        }
                    }
                }
            }
        }
    }
    scores.sort_by_key(|(ts, _)| *ts);
    let mut result: Vec<u32> = scores
        .into_iter()
        .map(|(_, s)| s)
        .rev()
        .take(last)
        .collect();
    result.reverse();
    result
}

pub(crate) fn render_html_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
) -> String {
    let (health, grade) = health_score(&report.checks);
    let overall_color = if report.passed { "var(--green)" } else { "var(--red)" };
    let overall_label = if report.passed { "PASSED" } else { "FAILED" };
    let pct = if report.summary.total_checks == 0 {
        100.0
    } else {
        report.summary.passed_checks as f64 / report.summary.total_checks as f64 * 100.0
    };
    let grade_color = match grade {
        'A' => "var(--green)",
        'B' => "var(--blue)",
        'C' => "var(--amber)",
        _ => "var(--red)",
    };

    // Split checks by category
    let mut sec_checks: Vec<&CheckResult> = Vec::new();
    let mut qual_checks: Vec<&CheckResult> = Vec::new();
    let mut comp_checks: Vec<&CheckResult> = Vec::new();
    let mut other_checks: Vec<&CheckResult> = Vec::new();
    for c in &report.checks {
        if security_tools.contains(&c.name.as_str()) {
            sec_checks.push(c);
        } else if compliance_tools.contains(&c.name.as_str()) {
            comp_checks.push(c);
        } else if quality_tools.contains(&c.name.as_str()) {
            qual_checks.push(c);
        } else {
            other_checks.push(c);
        }
    }
    qual_checks.extend(other_checks);

    // Category pass counts
    let sec_pass = sec_checks.iter().filter(|c| c.passed).count();
    let qual_pass = qual_checks.iter().filter(|c| c.passed).count();
    let comp_pass = comp_checks.iter().filter(|c| c.passed).count();
    let sec_col = if sec_pass == sec_checks.len() { "var(--green)" } else { "var(--red)" };
    let qual_col = if qual_pass == qual_checks.len() { "var(--green)" } else { "var(--red)" };
    let comp_col = if comp_pass == comp_checks.len() { "var(--green)" } else { "var(--red)" };

    let failed_checks: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();

    // ── Executive summary ────────────────────────────────────
    let risk_domain = if sec_checks.iter().any(|c| !c.passed) {
        "security"
    } else if comp_checks.iter().any(|c| !c.passed) {
        "compliance"
    } else if qual_checks.iter().any(|c| !c.passed) {
        "code quality"
    } else {
        "none"
    };
    let exec_verdict = if report.passed {
        format!("This codebase passed all {} checks with a health score of {}/100 (grade {}). No critical findings were detected across security, quality, or compliance domains.", report.summary.total_checks, health, grade)
    } else {
        let high_count = failed_checks
            .iter()
            .filter(|c| {
                matches!(
                    c.severity.as_deref(),
                    Some("high") | Some("critical") | Some("error")
                )
            })
            .count();
        format!(
            "{} of {} checks failed, concentrated in {}. {} finding{} rated high/critical severity require immediate attention before the next release.",
            failed_checks.len(), report.summary.total_checks, risk_domain,
            high_count, if high_count == 1 { "" } else { "s" }
        )
    };
    let top3: Vec<&CheckResult> = {
        let mut sorted = failed_checks.clone();
        sorted.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        sorted.into_iter().take(3).collect()
    };
    let top3_html = if top3.is_empty() {
        r#"<p style="color:var(--green);font-size:14px">✓ No action items — all checks passed.</p>"#
            .to_string()
    } else {
        let mut h = String::new();
        for (i, c) in top3.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" | "error" => "High effort",
                "medium" | "warning" => "Medium effort",
                _ => "Low effort",
            };
            let help = c.help.as_deref().unwrap_or("Review and fix flagged items.");
            h.push_str(&format!(
                r#"<div style="display:flex;gap:14px;padding:12px 0;border-bottom:1px solid var(--border-light);align-items:flex-start">
  <div style="font-size:20px;font-weight:800;color:var(--text-faint);min-width:24px">{}</div>
  <div style="flex:1">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:4px">
      <span style="font-weight:700;font-size:14px">{}</span>{}
      <span style="font-size:11px;color:var(--text-muted);margin-left:auto">{}</span>
    </div>
    <div style="font-size:13px;color:var(--text-secondary)">{}</div>
  </div>
</div>"#, i+1, html_escape(&c.name), severity_badge(sev), effort, html_escape(help)));
        }
        h
    };

    // ── Remediation table ─────────────────────────────────────
    let remediation_html = if failed_checks.is_empty() {
        r#"<p style="color:var(--green);font-weight:600;font-size:14px">✓ No findings — all checks passed.</p>"#.to_string()
    } else {
        let mut rows = String::new();
        let mut sorted_failed = failed_checks.clone();
        sorted_failed.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        for (i, c) in sorted_failed.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" | "error" => "High",
                "medium" | "warning" => "Medium",
                _ => "Low",
            };
            let help = c
                .help
                .as_deref()
                .unwrap_or("Review and fix the flagged items.");
            rows.push_str(&format!(
                r#"<tr style="border-bottom:1px solid var(--border-light)">
  <td style="padding:10px 14px;font-weight:700;color:var(--text-muted)">{}</td>
  <td style="padding:10px 14px;font-weight:600">{}</td>
  <td style="padding:10px 14px">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:var(--text-secondary)">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:var(--text-secondary)">{}</td>
</tr>"#,
                i + 1,
                html_escape(&c.name),
                severity_badge(sev),
                effort,
                html_escape(help),
            ));
        }
        format!(
            r#"<table style="width:100%;border-collapse:collapse;font-size:13px">
<thead><tr style="background:var(--surface-alt);border-bottom:2px solid var(--border)">
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">#</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Check</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Severity</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Effort</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Action</th>
</tr></thead><tbody>{rows}</tbody></table>"#,
            rows = rows
        )
    };

    // ── Section builder ───────────────────────────────────────
    fn section_html(title: &str, icon: &str, anchor: &str, checks: &[&CheckResult]) -> String {
        if checks.is_empty() {
            return String::new();
        }
        let rows: String = checks.iter().map(|c| check_row_html(c)).collect();
        let pass_c = checks.iter().filter(|c| c.passed).count();
        let fail_c = checks.len() - pass_c;
        let status_color = if fail_c == 0 { "var(--green)" } else { "var(--red)" };
        let status_pill = if fail_c == 0 {
            r#"<span style="background:var(--green-bg);color:var(--green);padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">ALL PASSED</span>"#.to_string()
        } else {
            format!(
                r#"<span style="background:var(--red-bg);color:var(--red);padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">{} FAILED</span>"#,
                fail_c
            )
        };
        format!(
            "<section id=\"{anch}\" style=\"margin-bottom:40px\">\n<div style=\"display:flex;align-items:center;gap:12px;margin-bottom:16px;padding-bottom:12px;border-bottom:2px solid var(--border-light)\">\n  <span style=\"font-size:22px\">{icn}</span>\n  <h2 style=\"font-size:18px;font-weight:800;color:var(--text);margin:0\">{ttl}</h2>\n  <span style=\"font-size:13px;color:{sc};font-weight:600;margin-left:4px\">{ps}/{tot}</span>\n  <div style=\"margin-left:auto\">{pill}</div>\n</div>\n<div style=\"border-radius:10px;overflow:hidden;box-shadow:var(--shadow-sm)\">\n<table style=\"width:100%;border-collapse:collapse;font-size:13px\">\n<thead><tr style=\"background:var(--surface-alt);border-bottom:2px solid var(--border)\">\n  <th style=\"padding:9px 14px;width:42px\"></th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600\">Check</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600\">Result / Details</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600\">Score</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600\">Sev</th>\n</tr></thead>\n<tbody>{rows}</tbody>\n</table></div></section>",
            anch = anchor, icn = icon, ttl = title, sc = status_color,
            ps = pass_c, tot = checks.len(), pill = status_pill, rows = rows,
        )
    }

    // ── File heatmap ─────────────────────────────────────────
    let file_heatmap_html = if report.file_summary.is_empty() {
        String::new()
    } else {
        let mut rows = String::new();
        for fs in &report.file_summary {
            let max_issues = report
                .file_summary
                .first()
                .map(|f| f.issue_count)
                .unwrap_or(1)
                .max(1);
            let bar_width = (fs.issue_count as f64 / max_issues as f64 * 200.0) as usize;
            let bar_color = if fs.severity_score >= 10 {
                "var(--red)"
            } else if fs.severity_score >= 5 {
                "var(--amber)"
            } else {
                "var(--green)"
            };
            rows.push_str(&format!(
                r#"<div class="heatmap-row" style="display:flex;align-items:center;gap:10px;padding:6px 0;border-bottom:1px solid var(--border-light)">
  <div style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{}</div>
  <div style="width:{}px;height:8px;background:{};border-radius:4px"></div>
  <div style="font-size:11px;color:var(--text-muted);min-width:24px;text-align:right">{}</div>
</div>"#,
                html_escape(&fs.file), bar_width, bar_color, fs.issue_count
            ));
        }
        format!(
            r#"<div class="card" id="heatmap">
  <div class="card-title">&#128293; File Heatmap</div>
  <p style="font-size:13px;color:var(--text-muted);margin-bottom:12px">Top files by total issue count across all tools.</p>
  {}
</div>"#,
            rows
        )
    };

    // ── Per-tool findings drill-down ─────────────────────────
    let findings_section_html = {
        let mut checks_with_findings: Vec<&CheckResult> = report
            .checks
            .iter()
            .filter(|c| !c.findings.is_empty())
            .collect();
        checks_with_findings.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        if checks_with_findings.is_empty() {
            String::new()
        } else {
            let mut sections = String::new();
            for check in &checks_with_findings {
                let mut table_rows = String::new();
                for finding in &check.findings {
                    let line = finding
                        .line
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    table_rows.push_str(&format!(
                        r#"<tr class="finding-row" style="border-bottom:1px solid var(--border-light)">
  <td style="padding:8px 12px;font-size:12px;color:var(--text-secondary)">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:var(--text-secondary)">{}</td>
  <td style="padding:8px 12px;font-size:12px">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:var(--text-secondary)">{}</td>
</tr>"#,
                        html_escape(&finding.file),
                        line,
                        html_escape(&finding.message),
                        html_escape(&finding.fix_hint)
                    ));
                }
                let sev_badge = severity_badge(check.severity.as_deref().unwrap_or("info"));
                sections.push_str(&format!(
                    r#"<div class="card" style="margin-bottom:20px">
  <div class="collapsible-header" style="display:flex;align-items:center;gap:10px;margin-bottom:0px;padding-bottom:12px;border-bottom:1px solid var(--border-light)">
    <span style="font-weight:700;font-size:14px">{}</span>
    <span style="margin-left:4px">{}</span>
    <span style="margin-left:auto;font-size:12px;color:var(--text-muted)">{} findings</span>
  </div>
  <div class="collapsible-body closed" style="max-height:0">
  <table style="width:100%;border-collapse:collapse;font-size:13px;margin-top:12px">
    <thead><tr style="background:var(--surface-alt);border-bottom:2px solid var(--border)">
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">File</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600;width:60px">Line</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Message</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:var(--text-muted);font-weight:600">Fix Hint</th>
    </tr></thead>
    <tbody>{}</tbody>
  </table>
  </div>
</div>"#,
                    html_escape(&check.name), sev_badge, check.findings.len(), table_rows
                ));
            }
            format!(
                r#"<div id="findings" style="scroll-margin-top:80px">
  <div class="card">
    <div class="card-title">&#128269; Findings Drill-Down
      <input type="text" placeholder="Search findings..." oninput="filterFindings(this.value)" style="margin-left:auto;padding:6px 10px;border:1px solid var(--border);border-radius:6px;font-size:12px;width:220px;background:var(--surface);color:var(--text)">
    </div>
  </div>
  {}
</div>"#,
                sections
            )
        }
    };

    let sec_section = section_html("Security Checks", "🔒", "security", &sec_checks);
    let qual_section = section_html("Code Quality Checks", "📊", "quality", &qual_checks);
    let comp_section = section_html("Compliance Checks", "📋", "compliance", &comp_checks);

    let donut = donut_svg(pct, overall_color);
    let sec_bar = mini_bar(sec_pass, sec_checks.len(), sec_col);
    let qual_bar = mini_bar(qual_pass, qual_checks.len(), qual_col);
    let comp_bar = mini_bar(comp_pass, comp_checks.len(), comp_col);

    // ── Severity distribution ────────────────────────────────
    let mut sev_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for check in &report.checks {
        for finding in &check.findings {
            *sev_counts.entry(finding.severity.clone()).or_insert(0) += 1;
        }
    }
    let severity_order = [
        ("critical", "var(--red)"),
        ("high", "var(--red)"),
        ("error", "var(--red)"),
        ("medium", "var(--amber)"),
        ("warning", "var(--amber)"),
        ("low", "var(--green)"),
        ("info", "var(--blue)"),
    ];
    let mut severity_chart_data: Vec<(String, usize, &str)> = Vec::new();
    for (sev, color) in &severity_order {
        if let Some(&count) = sev_counts.get(*sev) {
            severity_chart_data.push((sev.to_string(), count, *color));
        }
    }
    let severity_chart_html = if severity_chart_data.is_empty() {
        String::new()
    } else {
        let chart = severity_bar_chart_svg(&severity_chart_data);
        let mut badges = String::new();
        for (sev, count, color) in &severity_chart_data {
            let badge_bg = match sev.as_str() {
                "high" | "critical" | "error" => "var(--red-bg)",
                "medium" | "warning" => "var(--amber-bg)",
                "low" => "var(--green-bg)",
                _ => "var(--surface-alt)",
            };
            badges.push_str(&format!(
                r#"<span style="background:{};color:{};padding:3px 10px;border-radius:12px;font-size:11px;font-weight:600;margin-right:6px">{} {}</span>"#,
                badge_bg, color, count, html_escape(sev)
            ));
        }
        format!(
            r#"<div class="card" id="severity-dist">
  <div class="card-title">&#128202; Severity Distribution</div>
  <div style="margin-bottom:14px">{}</div>
  {}
</div>"#,
            badges, chart
        )
    };

    // ── Health score gauge ─────────────────────────────────
    let gauge = gauge_svg(health, grade_color);
    let gauge_html = format!(
        r#"<div class="card" style="display:flex;align-items:center;justify-content:center;flex-direction:column;padding:20px">
  {}
</div>"#,
        gauge
    );

    // ── Trend sparkline ──────────────────────────────────────
    let history_scores = history_health_scores(".cogent-history", 10);
    let sparkline_html = if history_scores.len() >= 2 {
        let spark = sparkline_svg(&history_scores, 320, 80);
        format!(
            r#"<div class="card" id="trends">
  <div class="card-title">&#128200; Health Score Trend</div>
  <p style="font-size:12px;color:var(--text-muted);margin-bottom:10px">Last {} runs from .cogent-history</p>
  {}
</div>"#,
            history_scores.len(),
            spark
        )
    } else {
        String::new()
    };

    // Use token replacement instead of format! to avoid CSS hex/class-name conflicts
    let tmpl = include_str!("report.html.tmpl");

    tmpl.replace("__PROJECT__", &html_escape(project))
        .replace("__DATE__", date)
        .replace("__PATH__", &html_escape(&report.path))
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__OC__", overall_color)
        .replace("__GC__", grade_color)
        .replace("__GRADE__", &grade.to_string())
        .replace("__OVERALL_LABEL__", overall_label)
        .replace("__PCT__", &format!("{:.0}", pct))
        .replace("__TOTAL__", &report.summary.total_checks.to_string())
        .replace("__PASSED_N__", &report.summary.passed_checks.to_string())
        .replace("__FAILED_N__", &report.summary.failed_checks.to_string())
        .replace("__HEALTH__", &health.to_string())
        .replace("__DONUT__", &donut)
        .replace("__SEC_BAR__", &sec_bar)
        .replace("__QUAL_BAR__", &qual_bar)
        .replace("__COMP_BAR__", &comp_bar)
        .replace("__SEC_PASS__", &sec_pass.to_string())
        .replace("__SEC_TOTAL__", &sec_checks.len().to_string())
        .replace("__QUAL_PASS__", &qual_pass.to_string())
        .replace("__QUAL_TOTAL__", &qual_checks.len().to_string())
        .replace("__COMP_PASS__", &comp_pass.to_string())
        .replace("__COMP_TOTAL__", &comp_checks.len().to_string())
        .replace(
            "__SS__",
            if sec_pass == sec_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__QS__",
            if qual_pass == qual_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__CS__",
            if comp_pass == comp_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__OS__",
            if report.passed {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace("__OL__", if report.passed { "PASS" } else { "FAIL" })
        .replace("__EXEC_VERDICT__", &html_escape(&exec_verdict))
        .replace("__TOP3_HTML__", &top3_html)
        .replace("__REMEDIATION_HTML__", &remediation_html)
        .replace("__FILE_HEATMAP__", &file_heatmap_html)
        .replace("__FINDINGS_SECTION__", &findings_section_html)
        .replace("__SEVERITY_CHART__", &severity_chart_html)
        .replace("__GAUGE__", &gauge_html)
        .replace("__SPARKLINE__", &sparkline_html)
        .replace("__QUAL_SECTION__", &qual_section)
        .replace("__SEC_SECTION__", &sec_section)
        .replace("__COMP_SECTION__", &comp_section)
}

pub(crate) fn render_markdown_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
    framework: &str,
) -> String {
    let overall = if report.passed {
        "✅ PASSED"
    } else {
        "❌ FAILED"
    };
    let pct = if report.summary.total_checks == 0 {
        100.0
    } else {
        report.summary.passed_checks as f64 / report.summary.total_checks as f64 * 100.0
    };

    let mut md = format!(
        "# Cogent Audit Report — {}\n\n\
         **Status:** {}  \n\
         **Generated:** {}  \n\
         **Path:** `{}`  \n\
         **Version:** Cogent v{}\n\n\
         ---\n\n\
         ## Summary\n\n\
         | Metric | Value |\n|---|---|\n\
         | Total Checks | {} |\n\
         | Passed | {} |\n\
         | Failed | {} |\n\
         | Pass Rate | {:.0}% |\n\n",
        project,
        overall,
        date,
        report.path,
        env!("CARGO_PKG_VERSION"),
        report.summary.total_checks,
        report.summary.passed_checks,
        report.summary.failed_checks,
        pct,
    );

    // File heatmap
    if !report.file_summary.is_empty() {
        md.push_str("## File Heatmap\n\n");
        md.push_str("| File | Issues | Severity Score |\n|---|---|---|\n");
        for fs in &report.file_summary {
            md.push_str(&format!(
                "| `{}` | {} | {} |\n",
                fs.file, fs.issue_count, fs.severity_score
            ));
        }
        md.push('\n');
    }

    let failed: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();
    if !failed.is_empty() {
        md.push_str("## Remediation Checklist\n\n");
        md.push_str("| # | Check | Severity | Effort | Action |\n|---|---|---|---|---|\n");
        for (i, c) in failed.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" => "High",
                "medium" => "Medium",
                _ => "Low",
            };
            let help = c.help.as_deref().unwrap_or("Review and fix.");
            md.push_str(&format!(
                "| {} | `{}` | {} | {} | {} |\n",
                i + 1,
                c.name,
                sev,
                effort,
                help
            ));
        }
        md.push('\n');
    }

    // Per-tool findings
    let checks_with_findings: Vec<&CheckResult> = report
        .checks
        .iter()
        .filter(|c| !c.findings.is_empty())
        .collect();
    if !checks_with_findings.is_empty() {
        md.push_str("## Findings by Tool\n\n");
        for check in &checks_with_findings {
            md.push_str(&format!(
                "### `{}` ({} findings)\n\n",
                check.name,
                check.findings.len()
            ));
            md.push_str("| File | Line | Message | Rule | Fix Hint |\n|---|---|---|---|---|\n");
            for finding in &check.findings {
                let line = finding
                    .line
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!(
                    "| `{}` | {} | {} | `{}` | {} |\n",
                    finding.file, line, finding.message, finding.rule_id, finding.fix_hint
                ));
            }
            md.push('\n');
        }
    }

    let categories: &[(&str, &str, &[&str])] = &[
        ("Code Quality Checks", "📊", quality_tools),
        ("Security Checks", "🔒", security_tools),
        ("Compliance Checks", "📋", compliance_tools),
    ];
    for (title, icon, tools) in categories {
        let checks: Vec<&CheckResult> = report
            .checks
            .iter()
            .filter(|c| tools.contains(&c.name.as_str()))
            .collect();
        if checks.is_empty() {
            continue;
        }
        md.push_str(&format!("## {} {}\n\n", icon, title));
        md.push_str("| Check | Status | Score | Severity | Message |\n|---|---|---|---|---|\n");
        for c in &checks {
            let status = if c.passed { "✅" } else { "❌" };
            let sev = c.severity.as_deref().unwrap_or("info");
            let score = match (c.score, c.threshold) {
                (Some(s), Some(t)) => format!("{:.1}/{:.1}", s, t),
                (Some(s), None) => format!("{:.1}", s),
                _ => "—".to_string(),
            };
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                c.name, status, score, sev, c.message
            ));
        }
        md.push('\n');
    }

    // HQSE Lifecycle Coverage section (only when --framework hqse)
    if framework == "hqse" {
        md.push_str("## HQSE Lifecycle Coverage\n\n");
        md.push_str("| HQSE Phase | Checks | Status |\n|---|---|---|\n");
        let hqse_phases: &[(&str, &[&str])] = &[
            ("§2 Requirements", &[]),
            ("§3 Design", &["design-docs", "doccov"]),
            ("§4 Code", &["complexity", "crap", "debt", "secrets", "sast", "crypto", "taint", "deadcode", "linelen", "halstead", "cohesion", "coupling"]),
            ("§4.5 Tracing", &["observability", "debuggability"]),
            ("§5 Code Review", &[]),
            ("§6 Test", &["test-quality", "propcov", "errhandle", "typecov"]),
            ("§7 Support", &["errhandle", "debuggability", "observability"]),
            ("§8–9 Planning", &[]),
        ];
        for (phase, tools) in hqse_phases {
            if tools.is_empty() {
                md.push_str(&format!("| {} | — | ⬜ not automatable |\n", phase));
                continue;
            }
            let phase_checks: Vec<&CheckResult> = report.checks.iter()
                .filter(|c| tools.contains(&c.name.as_str()))
                .collect();
            if phase_checks.is_empty() {
                md.push_str(&format!("| {} | {} | ⬜ not run |\n", phase, tools.join(", ")));
                continue;
            }
            let all_pass = phase_checks.iter().all(|c| c.passed);
            let status = if all_pass { "✅" } else { "❌" };
            let names: Vec<&str> = phase_checks.iter().map(|c| c.name.as_str()).collect();
            md.push_str(&format!("| {} | {} | {} |\n", phase, names.join(", "), status));
        }
        md.push('\n');
    }

    md.push_str("---\n\n*Generated by Cogent — automated code quality & security auditing.*  \n");
    md.push_str("*This report is machine-generated. Results should be reviewed by a qualified engineer before use in compliance filings.*\n");
    md
}

pub(crate) fn setup_command() {
    let ascii_art = r#"
   ____          _      __  __      _        _          
  / ___|___   __| | ___|  \/  | ___| |_ _ __(_) ___ ___ 
 | |   / _ \ / _` |/ _ \ |\/| |/ _ \ __| '__| |/ __/ __|
 | |__| (_) | (_| |  __/ |  | |  __/ |_| |  | | (__\__ \
  \____\___/ \__,_|\___|_|  |_|\___|\__|_|  |_|\___|___/
"#;
    println!("{}", ascii_art.cyan().bold());
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
    println!("{}", "  Cogent Doctor & Setup".cyan().bold());
    println!(
        "{}\n",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );

    let mut all_passed = true;

    // Check cargo
    if std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .is_ok()
    {
        println!("  {} cargo installed", "[✓]".green().bold());
    } else {
        println!("  {} cargo NOT installed", "[✗]".red().bold());
        println!("      => {}", "Install Rust: https://rustup.rs/".yellow());
        all_passed = false;
    }

    // Check cargo-llvm-cov
    if std::process::Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .output()
        .is_ok()
    {
        println!("  {} cargo-llvm-cov installed", "[✓]".green().bold());
    } else {
        println!("  {} cargo-llvm-cov NOT installed", "[✗]".red().bold());
        println!("      => {}", "Run: cargo install cargo-llvm-cov".yellow());
        all_passed = false;
    }

    // Check llvm-tools-preview
    let rustup_out = std::process::Command::new("rustup")
        .args(["component", "list"])
        .output()
        .ok();
    if let Some(out) = rustup_out {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.contains("llvm-tools-preview (installed)")
            || stdout.contains("llvm-tools (installed)")
        {
            println!("  {} llvm-tools installed", "[✓]".green().bold());
        } else {
            println!("  {} llvm-tools NOT installed", "[✗]".red().bold());
            println!(
                "      => {}",
                "Run: rustup component add llvm-tools-preview".yellow()
            );
            all_passed = false;
        }
    } else {
        println!(
            "  {} rustup not found, could not verify llvm-tools",
            "[?]".yellow().bold()
        );
    }

    // Check .quality.toml
    if std::path::Path::new(".quality.toml").exists() {
        println!(
            "  {} .quality.toml configuration found",
            "[✓]".green().bold()
        );
    } else {
        println!("  {} .quality.toml NOT found", "[✗]".red().bold());
        println!("      => {}", "Run: cogent init".yellow());
        all_passed = false;
    }

    println!(
        "\n{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
    if all_passed {
        println!(
            "  {}",
            "Everything looks good! Your codebase is ready."
                .green()
                .bold()
        );
    } else {
        println!(
            "  {}",
            "Please resolve the missing requirements above."
                .red()
                .bold()
        );
    }
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── html_escape ──

    #[test]
    fn test_html_escape_no_special_chars() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    #[test]
    fn test_html_escape_ampersand() {
        assert_eq!(html_escape("AT&T"), "AT&amp;T");
    }

    #[test]
    fn test_html_escape_angle_brackets() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
    }

    #[test]
    fn test_html_escape_quotes() {
        assert_eq!(html_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn test_html_escape_all() {
        assert_eq!(
            html_escape("<a href=\"x&y\">"),
            "&lt;a href=&quot;x&amp;y&quot;&gt;"
        );
    }

    #[test]
    fn test_html_escape_empty_string() {
        assert_eq!(html_escape(""), "");
    }

    // ── severity_color_html ──

    #[test]
    fn test_severity_color_high() {
        assert_eq!(severity_color_html("high"), "var(--red)");
    }

    #[test]
    fn test_severity_color_critical() {
        assert_eq!(severity_color_html("critical"), "var(--red)");
    }

    #[test]
    fn test_severity_color_error() {
        assert_eq!(severity_color_html("error"), "var(--red)");
    }

    #[test]
    fn test_severity_color_medium() {
        assert_eq!(severity_color_html("medium"), "var(--amber)");
    }

    #[test]
    fn test_severity_color_warning() {
        assert_eq!(severity_color_html("warning"), "var(--amber)");
    }

    #[test]
    fn test_severity_color_low() {
        assert_eq!(severity_color_html("low"), "var(--blue)");
    }

    #[test]
    fn test_severity_color_unknown() {
        assert_eq!(severity_color_html("info"), "var(--text-muted)");
        assert_eq!(severity_color_html(""), "var(--text-muted)");
        assert_eq!(severity_color_html("nonexistent"), "var(--text-muted)");
    }

    // ── severity_badge ──

    #[test]
    fn test_severity_badge_contains_severity_text() {
        let badge = severity_badge("high");
        assert!(badge.contains("high"));
        assert!(badge.contains("var(--red)"));
        assert!(badge.contains("<span"));
    }

    #[test]
    fn test_severity_badge_medium() {
        let badge = severity_badge("medium");
        assert!(badge.contains("medium"));
        assert!(badge.contains("var(--amber)"));
    }

    #[test]
    fn test_severity_badge_low() {
        let badge = severity_badge("low");
        assert!(badge.contains("low"));
        assert!(badge.contains("var(--blue)"));
    }

    #[test]
    fn test_severity_badge_info_fallback() {
        let badge = severity_badge("info");
        assert!(badge.contains("info"));
        assert!(badge.contains("var(--text-muted)"));
    }

    #[test]
    fn test_severity_badge_contains_style_classes() {
        let badge = severity_badge("high");
        assert!(badge.contains("border-radius:12px"));
        assert!(badge.contains("text-transform:uppercase"));
        assert!(badge.contains("font-weight:600"));
    }

    // ── donut_svg ──

    #[test]
    fn test_donut_svg_starts_with_svg_tag() {
        let svg = donut_svg(75.0, "var(--green)");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn test_donut_svg_contains_pct_text() {
        let svg = donut_svg(50.0, "var(--amber)");
        assert!(svg.contains("50%"));
        assert!(svg.contains("var(--amber)"));
    }

    #[test]
    fn test_donut_svg_zero_percent() {
        let svg = donut_svg(0.0, "var(--red)");
        assert!(svg.contains("0%"));
    }

    #[test]
    fn test_donut_svg_hundred_percent() {
        let svg = donut_svg(100.0, "var(--green)");
        assert!(svg.contains("100%"));
    }

    #[test]
    fn test_donut_svg_contains_pass_rate_label() {
        let svg = donut_svg(80.0, "var(--green)");
        assert!(svg.contains("pass rate"));
    }

    #[test]
    fn test_donut_svg_contains_circle_elements() {
        let svg = donut_svg(90.0, "var(--green)");
        assert!(svg.contains("<circle"));
        assert!(svg.contains("stroke-dasharray"));
    }

    // ── mini_bar ──

    #[test]
    fn test_mini_bar_zero_total_returns_empty() {
        assert_eq!(mini_bar(0, 0, "var(--green)"), "");
    }

    #[test]
    fn test_mini_bar_all_passed() {
        let bar = mini_bar(5, 5, "var(--green)");
        assert!(bar.contains("████████████"));  // 12 filled
        assert!(bar.contains("5/5"));
        assert!(bar.contains("100%"));
        assert!(bar.contains("var(--green)"));
    }

    #[test]
    fn test_mini_bar_half_passed() {
        let bar = mini_bar(3, 6, "var(--amber)");
        assert!(bar.contains("██████"));  // 6 filled
        assert!(bar.contains("░░░░░░"));  // 6 empty
        assert!(bar.contains("3/6"));
        assert!(bar.contains("50%"));
        assert!(bar.contains("var(--amber)"));
    }

    #[test]
    fn test_mini_bar_none_passed() {
        let bar = mini_bar(0, 4, "var(--red)");
        assert!(bar.contains("░░░░░░░░░░░░"));  // 12 empty
        assert!(bar.contains("0/4"));
        assert!(bar.contains("0%"));
    }

    #[test]
    fn test_mini_bar_contains_color() {
        let bar = mini_bar(2, 10, "var(--green)");
        assert!(bar.contains("var(--green)"));
    }

    // ── gauge_svg ──

    #[test]
    fn test_gauge_svg_starts_and_ends_with_svg() {
        let svg = gauge_svg(75, "var(--green)");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn test_gauge_svg_contains_score_text() {
        let svg = gauge_svg(42, "var(--red)");
        assert!(svg.contains("42"));
        assert!(svg.contains("Health Score"));
    }

    #[test]
    fn test_gauge_svg_contains_color() {
        let svg = gauge_svg(100, "var(--green)");
        assert!(svg.contains("var(--green)"));
    }

    #[test]
    fn test_gauge_svg_zero_score() {
        let svg = gauge_svg(0, "var(--red)");
        assert!(svg.contains("0"));
    }

    #[test]
    fn test_gauge_svg_contains_path_elements() {
        let svg = gauge_svg(50, "var(--amber)");
        assert!(svg.contains("<path"));
        assert!(svg.contains("M"));  // SVG path command
    }

    // ── sparkline_svg ──

    #[test]
    fn test_sparkline_svg_less_than_two_points_returns_empty() {
        assert_eq!(sparkline_svg(&[], 300, 100), "");
        assert_eq!(sparkline_svg(&[50], 300, 100), "");
    }

    #[test]
    fn test_sparkline_svg_two_points() {
        let svg = sparkline_svg(&[0, 100], 300, 100);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_sparkline_svg_contains_grid_line() {
        let svg = sparkline_svg(&[30, 70, 90], 320, 80);
        assert!(svg.contains("<line"));
        assert!(svg.contains("stroke-dasharray"));
    }

    #[test]
    fn test_sparkline_svg_many_points() {
        let scores: Vec<u32> = (0..=100).step_by(10).collect();
        let svg = sparkline_svg(&scores, 600, 120);
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("var(--accent)") || svg.contains("stroke"));
    }

    #[test]
    fn test_sparkline_svg_flat_line() {
        let svg = sparkline_svg(&[50, 50, 50, 50, 50], 300, 100);
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_sparkline_svg_contains_viewbox() {
        let svg = sparkline_svg(&[10, 90], 300, 100);
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("300"));
        assert!(svg.contains("100"));
    }

    #[test]
    fn test_sparkline_svg_uses_accent_color() {
        let scores: Vec<u32> = (0..=100).step_by(10).collect();
        let svg = sparkline_svg(&scores, 600, 120);
        assert!(svg.contains("var(--accent)") || svg.contains("--accent"));
    }
}
