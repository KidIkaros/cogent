//! HTML and Markdown report generation for Cogent.
//! Extracted from cogent-cli/src/main.rs.

#![deny(clippy::all)]

use crate::html_escape;
use cogent_common::{CheckReport, CheckResult, health_score};
use tracing::info;

fn severity_color_html(sev: &str) -> &'static str {
    match sev {
        "high" | "critical" | "error" => "#ef4444",
        "medium" | "warning" => "#f59e0b",
        "low" => "#3b82f6",
        _ => "#6b7280",
    }
}

fn severity_badge(sev: &str) -> String {
    let color = severity_color_html(sev);
    format!(
        r#"<span style="background:{c};color:#fff;padding:2px 8px;border-radius:12px;font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:.03em">{s}</span>"#,
        c = color,
        s = sev
    )
}

/// Build a collapsible offender list from CheckResult.details JSON
fn offender_rows_html(c: &CheckResult) -> String {
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
                    r#"<div style="display:flex;gap:12px;padding:4px 0;border-bottom:1px solid #f3f4f6;font-size:12px">
  <span style="color:#6366f1;font-family:monospace;white-space:nowrap;min-width:180px">{}</span>
  <span style="color:#6b7280">{}</span>
</div>"#,
                    html_escape(&loc), html_escape(&desc_trunc)
                ));
            }
            let more = if arr.len() > 10 {
                format!(
                    r#"<div style="font-size:12px;color:#9ca3af;padding-top:6px">… {} more findings</div>"#,
                    arr.len() - 10
                )
            } else {
                String::new()
            };
            return format!(
                r#"<details style="margin-top:8px">
<summary style="font-size:12px;color:#6366f1;cursor:pointer;user-select:none;padding:4px 0">▶ Show {} finding{}</summary>
<div style="margin-top:8px;padding:8px 12px;background:#f9fafb;border-radius:6px;border-left:3px solid #6366f1">
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

fn check_row_html(c: &CheckResult) -> String {
    let icon = if c.passed { "&#10003;" } else { "&#10007;" };
    let icon_color = if c.passed { "#22c55e" } else { "#ef4444" };
    let row_bg = if c.passed { "#fff" } else { "#fef2f2" };
    let name_color = if c.passed { "#111827" } else { "#ef4444" };
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
        "<div style=\"font-size:13px;color:#374151\">{msg}</div><div style=\"font-size:11px;color:#9ca3af;margin-top:3px\">{help}</div>{off}",
        msg = html_escape(&c.message), help = html_escape(help), off = offenders
    );
    format!(
        "<tr style=\"background:{rb};border-bottom:1px solid #f3f4f6;vertical-align:top\">\n  <td style=\"padding:12px 14px;font-size:18px;color:{ic};text-align:center;width:40px;font-weight:700\">{icon}</td>\n  <td style=\"padding:12px 14px;font-weight:600;font-size:13px;color:{nc};white-space:nowrap\">{name}</td>\n  <td style=\"padding:12px 14px\">{mc}</td>\n  <td style=\"padding:12px 14px;font-size:12px;color:#6b7280;white-space:nowrap\">{score}</td>\n  <td style=\"padding:12px 14px;white-space:nowrap\">{sb}</td>\n</tr>",
        rb = row_bg, ic = icon_color, icon = icon, nc = name_color,
        name = c.name, mc = msg_cell, score = score_str, sb = severity_badge(sev),
    )
}

/// SVG donut ring showing pass percentage. r=44 → circumference≈276.
fn donut_svg(pct: f64, color: &str) -> String {
    let circ = 276.46f64;
    let dash = circ * pct / 100.0;
    let gap = circ - dash;
    let pct_int = pct as u32;
    // Build without format! to avoid Rust 2021 prefixed-literal issues with HTML
    let mut s = String::from(
        r#"<svg viewBox="0 0 100 100" width="120" height="120" style="display:block">"#,
    );
    s.push_str("\n  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" stroke=\"#e5e7eb\" stroke-width=\"10\"/>\n");
    s.push_str(&format!(
        "  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" stroke=\"{}\" stroke-width=\"10\"\n",
        color
    ));
    s.push_str(&format!(
        "    stroke-dasharray=\"{:.2} {:.2}\" stroke-dashoffset=\"69.12\"\n",
        dash, gap
    ));
    s.push_str("    stroke-linecap=\"round\" transform=\"rotate(-90 50 50)\"/>\n");
    s.push_str(&format!("  <text x=\"50\" y=\"46\" text-anchor=\"middle\" font-size=\"18\" font-weight=\"800\" fill=\"{}\" font-family=\"system-ui\">{}%</text>\n", color, pct_int));
    s.push_str("  <text x=\"50\" y=\"60\" text-anchor=\"middle\" font-size=\"9\" fill=\"#9ca3af\" font-family=\"system-ui\">pass rate</text>\n");
    s.push_str("</svg>");
    s
}

/// Inline horizontal mini-bar for a category (e.g. "6/8 ██████░░")
fn mini_bar(pass: usize, total: usize, color: &str) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (pass * 12) / total;
    let bar: String = "█".repeat(filled) + &"░".repeat(12 - filled);
    let pct = pass * 100 / total;
    format!(
        "<div style=\"display:flex;align-items:center;gap:8px;font-size:12px\">\n  <span style=\"font-family:monospace;color:{color};letter-spacing:.1em\">{bar}</span>\n  <span style=\"color:#6b7280\">{pass}/{total} ({pct}%)</span>\n</div>",
        color = color, bar = bar, pass = pass, total = total, pct = pct
    )
}

/// Semi-circle gauge SVG for health score 0-100.
fn gauge_svg(score: u32, color: &str) -> String {
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
        r##"<path d="M {} {} A {} {} 0 0 1 {} {}" fill="none" stroke="#e5e7eb" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, cx + radius, cy
    ));
    // Foreground arc
    let large_arc = if score > 50 { 1 } else { 0 };
    s.push_str(&format!(
        r##"<path d="M {} {} A {} {} 0 {} 1 {} {}" fill="none" stroke="{}" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, large_arc, needle_x, needle_y, color
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="28" font-weight="800" fill="{}" font-family="system-ui">{}</text>"##,
        cx, cy + 8.0, color, score
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="10" fill="#9ca3af" font-family="system-ui">Health Score</text>"##,
        cx, cy + 24.0
    ));
    s.push_str("</svg>");
    s
}

/// Horizontal bar chart SVG for severity distribution.
fn severity_bar_chart_svg(counts: &[(String, usize, &str)]) -> String {
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
            r##"<rect x="40" y="{}" width="{}" height="{}" rx="3" fill="{}"/>"##,
            y,
            w.max(2.0),
            bar_h,
            color
        ));
        s.push_str(&format!(
            r##"<text x="35" y="{}" text-anchor="end" font-size="11" fill="#6b7280" font-family="system-ui" dy="{}">{} ({})</text>"##,
            y + bar_h / 2 + 4, 0, html_escape(label), count
        ));
    }
    s.push_str("</svg>");
    s
}

/// Sparkline SVG from a series of health scores.
pub fn sparkline_svg(scores: &[u32], width: usize, height: usize) -> String {
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
        r##"<line x1="0" y1="{}" x2="{}" y2="{}" stroke="#e2e8f0" stroke-width="1" stroke-dasharray="3,3"/>"##,
        y50, width, y50
    ));
    // Polyline
    s.push_str(&format!(
        r##"<polyline points="{}" fill="none" stroke="#6366f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##,
        points
    ));
    // Dots
    for (i, &score) in scores.iter().enumerate() {
        let x = i as f64 * step_x;
        let y = padding + plot_h - ((score - min_score) as f64 / range) * plot_h;
        s.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="2.5" fill="#6366f1"/>"##,
            x, y
        ));
    }
    s.push_str("</svg>");
    s
}

/// Read `.cogent-history/` and return last N health scores.
fn history_health_scores(dir: &str, last: usize) -> Vec<u32> {
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

pub fn render_html_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
) -> String {
    info!(checks = report.checks.len(), project, "rendering HTML report");
    let (health, grade) = health_score(&report.checks);
    let overall_color = if report.passed { "#22c55e" } else { "#ef4444" };
    let overall_label = if report.passed { "PASSED" } else { "FAILED" };
    let pct = if report.summary.total_checks == 0 {
        100.0
    } else {
        report.summary.passed_checks as f64 / report.summary.total_checks as f64 * 100.0
    };
    let grade_color = match grade {
        'A' => "#22c55e",
        'B' => "#06b6d4",
        'C' => "#f59e0b",
        _ => "#ef4444",
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
    let sec_col = if sec_pass == sec_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };
    let qual_col = if qual_pass == qual_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };
    let comp_col = if comp_pass == comp_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };

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
        r#"<p style="color:#22c55e;font-size:14px">✓ No action items — all checks passed.</p>"#
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
                r#"<div style="display:flex;gap:14px;padding:12px 0;border-bottom:1px solid #f3f4f6;align-items:flex-start">
  <div style="font-size:20px;font-weight:800;color:#d1d5db;min-width:24px">{}</div>
  <div style="flex:1">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:4px">
      <span style="font-weight:700;font-size:14px">{}</span>{}
      <span style="font-size:11px;color:#9ca3af;margin-left:auto">{}</span>
    </div>
    <div style="font-size:13px;color:#6b7280">{}</div>
  </div>
</div>"#, i+1, html_escape(&c.name), severity_badge(sev), effort, html_escape(help)));
        }
        h
    };

    // ── Remediation table ─────────────────────────────────────
    let remediation_html = if failed_checks.is_empty() {
        r#"<p style="color:#22c55e;font-weight:600;font-size:14px">✓ No findings — all checks passed.</p>"#.to_string()
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
                r#"<tr style="border-bottom:1px solid #f3f4f6">
  <td style="padding:10px 14px;font-weight:700;color:#9ca3af">{}</td>
  <td style="padding:10px 14px;font-weight:600">{}</td>
  <td style="padding:10px 14px">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:#6b7280">{}</td>
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
<thead><tr style="background:#f9fafb;border-bottom:2px solid #e5e7eb">
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">#</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Check</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Severity</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Effort</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Action</th>
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
        let status_color = if fail_c == 0 { "#22c55e" } else { "#ef4444" };
        let status_pill = if fail_c == 0 {
            r#"<span style="background:#dcfce7;color:#16a34a;padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">ALL PASSED</span>"#.to_string()
        } else {
            format!(
                r#"<span style="background:#fee2e2;color:#ef4444;padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">{} FAILED</span>"#,
                fail_c
            )
        };
        format!(
            "<section id=\"{anch}\" style=\"margin-bottom:40px\">\n<div style=\"display:flex;align-items:center;gap:12px;margin-bottom:16px;padding-bottom:12px;border-bottom:2px solid #f3f4f6\">\n  <span style=\"font-size:22px\">{icn}</span>\n  <h2 style=\"font-size:18px;font-weight:800;color:#111827;margin:0\">{ttl}</h2>\n  <span style=\"font-size:13px;color:{sc};font-weight:600;margin-left:4px\">{ps}/{tot}</span>\n  <div style=\"margin-left:auto\">{pill}</div>\n</div>\n<div style=\"border-radius:10px;overflow:hidden;box-shadow:0 1px 4px rgba(0,0,0,.08)\">\n<table style=\"width:100%;border-collapse:collapse;font-size:13px\">\n<thead><tr style=\"background:#f9fafb;border-bottom:2px solid #e5e7eb\">\n  <th style=\"padding:9px 14px;width:42px\"></th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Check</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Result / Details</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Score</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Sev</th>\n</tr></thead>\n<tbody>{rows}</tbody>\n</table></div></section>",
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
                "#ef4444"
            } else if fs.severity_score >= 5 {
                "#f59e0b"
            } else {
                "#22c55e"
            };
            rows.push_str(&format!(
                r#"<div class="heatmap-row" style="display:flex;align-items:center;gap:10px;padding:6px 0;border-bottom:1px solid #f3f4f6">
  <div style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{}</div>
  <div style="width:{}px;height:8px;background:{};border-radius:4px"></div>
  <div style="font-size:11px;color:#6b7280;min-width:24px;text-align:right">{}</div>
</div>"#,
                html_escape(&fs.file), bar_width, bar_color, fs.issue_count
            ));
        }
        format!(
            r#"<div class="card" id="heatmap">
  <div class="card-title">&#128293; File Heatmap</div>
  <p style="font-size:13px;color:#9ca3af;margin-bottom:12px">Top files by total issue count across all tools.</p>
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
                        r#"<tr class="finding-row" style="border-bottom:1px solid #f3f4f6">
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:8px 12px;font-size:12px">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
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
  <div class="collapsible-header" style="display:flex;align-items:center;gap:10px;margin-bottom:0px;padding-bottom:12px;border-bottom:1px solid #f3f4f6">
    <span style="font-weight:700;font-size:14px">{}</span>
    <span style="margin-left:4px">{}</span>
    <span style="margin-left:auto;font-size:12px;color:#6b7280">{} findings</span>
  </div>
  <div class="collapsible-body closed" style="max-height:0">
  <table style="width:100%;border-collapse:collapse;font-size:13px;margin-top:12px">
    <thead><tr style="background:#f9fafb;border-bottom:2px solid #e5e7eb">
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">File</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;width:60px">Line</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Message</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Fix Hint</th>
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
      <input type="text" placeholder="Search findings..." oninput="filterFindings(this.value)" style="margin-left:auto;padding:6px 10px;border:1px solid #e2e8f0;border-radius:6px;font-size:12px;width:220px">
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
        ("critical", "#dc2626"),
        ("high", "#ef4444"),
        ("error", "#ef4444"),
        ("medium", "#f59e0b"),
        ("warning", "#f59e0b"),
        ("low", "#22c55e"),
        ("info", "#3b82f6"),
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
            badges.push_str(&format!(
                r#"<span style="background:{}20;color:{};padding:3px 10px;border-radius:12px;font-size:11px;font-weight:600;margin-right:6px">{} {}</span>"#,
                color, color, count, html_escape(sev)
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
  <p style="font-size:12px;color:#9ca3af;margin-bottom:10px">Last {} runs from .cogent-history</p>
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

pub fn render_markdown_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
) -> String {
    info!(checks = report.checks.len(), project, "rendering Markdown report");
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

    md.push_str("---\n\n*Generated by Cogent — automated code quality & security auditing.*  \n");
    md.push_str("*This report is machine-generated. Results should be reviewed by a qualified engineer before use in compliance filings.*\n");
    md
}

#[cfg(test)]
mod html_tests {
    use super::*;
    use cogent_common::{CheckReport, CheckResult, CheckSummary, Finding};

    // ── severity_color_html ────────────────────────────────────────

    #[test]
    fn test_severity_color_high() {
        assert_eq!(severity_color_html("high"), "#ef4444");
        assert_eq!(severity_color_html("critical"), "#ef4444");
        assert_eq!(severity_color_html("error"), "#ef4444");
    }

    #[test]
    fn test_severity_color_medium() {
        assert_eq!(severity_color_html("medium"), "#f59e0b");
        assert_eq!(severity_color_html("warning"), "#f59e0b");
    }

    #[test]
    fn test_severity_color_low() {
        assert_eq!(severity_color_html("low"), "#3b82f6");
    }

    #[test]
    fn test_severity_color_default() {
        assert_eq!(severity_color_html("info"), "#6b7280");
        assert_eq!(severity_color_html("unknown"), "#6b7280");
    }

    // ── severity_badge ──────────────────────────────────────────────

    #[test]
    fn test_severity_badge_contains_text() {
        let badge = severity_badge("high");
        assert!(badge.contains(">high<"));
        assert!(badge.contains("#ef4444"));
    }

    #[test]
    fn test_severity_badge_uppercase() {
        let badge = severity_badge("critical");
        assert!(badge.contains("uppercase"));
    }

    // ── donut_svg ──────────────────────────────────────────────────

    #[test]
    fn test_donut_svg_contains_percentage() {
        let svg = donut_svg(75.0, "#22c55e");
        assert!(svg.contains("75%"));
        assert!(svg.contains("#22c55e"));
    }

    #[test]
    fn test_donut_svg_zero_percent() {
        let svg = donut_svg(0.0, "#ef4444");
        assert!(svg.contains("0%"));
    }

    #[test]
    fn test_donut_svg_full_percent() {
        let svg = donut_svg(100.0, "#22c55e");
        assert!(svg.contains("100%"));
    }

    #[test]
    fn test_donut_svg_contains_viewbox() {
        let svg = donut_svg(50.0, "#f59e0b");
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("<text"));
    }

    // ── mini_bar ────────────────────────────────────────────────────

    #[test]
    fn test_mini_bar_shows_fraction() {
        let bar = mini_bar(6, 8, "#22c55e");
        assert!(bar.contains("6/8"));
        assert!(bar.contains("75%"));
    }

    #[test]
    fn test_mini_bar_all_passed() {
        let bar = mini_bar(8, 8, "#22c55e");
        assert!(bar.contains("8/8"));
        assert!(bar.contains("100%"));
    }

    #[test]
    fn test_mini_bar_all_failed() {
        let bar = mini_bar(0, 5, "#ef4444");
        assert!(bar.contains("0/5"));
        assert!(bar.contains("0%"));
    }

    #[test]
    fn test_mini_bar_zero_total() {
        assert_eq!(mini_bar(0, 0, "#000"), "");
    }

    // ── gauge_svg ──────────────────────────────────────────────────

    #[test]
    fn test_gauge_svg_contains_score() {
        let svg = gauge_svg(85, "#22c55e");
        assert!(svg.contains("85"));
        assert!(svg.contains("Health Score"));
    }

    #[test]
    fn test_gauge_svg_zero() {
        let svg = gauge_svg(0, "#ef4444");
        assert!(svg.contains("0"));
    }

    #[test]
    fn test_gauge_svg_full() {
        let svg = gauge_svg(100, "#22c55e");
        assert!(svg.contains("100"));
    }

    #[test]
    fn test_gauge_svg_contains_path() {
        let svg = gauge_svg(50, "#f59e0b");
        assert!(svg.contains("<path"));
        assert!(svg.contains("<text"));
    }

    // ── severity_bar_chart_svg ──────────────────────────────────────

    #[test]
    fn test_severity_bar_chart_empty() {
        assert_eq!(severity_bar_chart_svg(&[]), "");
    }

    #[test]
    fn test_severity_bar_chart_single() {
        let data = vec![("critical".to_string(), 5, "#dc2626")];
        let svg = severity_bar_chart_svg(&data);
        assert!(svg.contains("critical"));
        assert!(svg.contains("5"));
    }

    #[test]
    fn test_severity_bar_chart_multiple() {
        let data = vec![
            ("critical".to_string(), 3, "#dc2626"),
            ("high".to_string(), 7, "#ef4444"),
            ("low".to_string(), 1, "#22c55e"),
        ];
        let svg = severity_bar_chart_svg(&data);
        assert!(svg.contains("critical"));
        assert!(svg.contains("high"));
        assert!(svg.contains("low"));
        assert!(svg.contains("3"));
        assert!(svg.contains("7"));
        assert!(svg.contains("1"));
    }

    // ── sparkline_svg ───────────────────────────────────────────────

    #[test]
    fn test_sparkline_svg_fewer_than_two_points() {
        assert_eq!(sparkline_svg(&[50], 100, 30), "");
    }

    #[test]
    fn test_sparkline_svg_two_points() {
        let svg = sparkline_svg(&[50, 100], 100, 30);
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn test_sparkline_svg_multiple_points() {
        let svg = sparkline_svg(&[30, 50, 80, 100, 95, 70], 200, 60);
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("stroke-dasharray=\"3,3\""), "should have grid line at 50%");
    }

    #[test]
    fn test_sparkline_svg_flat_line() {
        let svg = sparkline_svg(&[50, 50, 50, 50], 100, 30);
        assert!(svg.contains("<polyline"));
    }

    // ── offender_rows_html ──────────────────────────────────────────

    fn make_check_result_with_items(items: serde_json::Value) -> CheckResult {
        CheckResult {
            name: "test".into(),
            passed: false,
            score: None,
            threshold: None,
            message: "test check".into(),
            details: items,
            severity: Some("high".into()),
            help: Some("fix it".into()),
            rule_id: Some("T-001".into()),
            findings: vec![],
        }
    }

    #[test]
    fn test_offender_rows_empty() {
        let c = make_check_result_with_items(serde_json::json!({"items": []}));
        assert_eq!(offender_rows_html(&c), "");
    }

    #[test]
    fn test_offender_rows_with_items() {
        let c = make_check_result_with_items(serde_json::json!({
            "items": [
                {"file": "src/main.rs", "line": 42, "context": "dangerous call"},
                {"file": "src/lib.rs", "line": 10, "context": "bad pattern"}
            ]
        }));
        let html = offender_rows_html(&c);
        assert!(html.contains("src/main.rs"));
        assert!(html.contains("src/lib.rs"));
        assert!(html.contains("<details"));
        assert!(html.contains("<summary"));
    }

    #[test]
    fn test_offender_rows_more_than_ten_shows_more() {
        // Build 15 items to test the 'N more findings' truncation
        let mut items_json = Vec::new();
        for i in 0..15 {
            items_json.push(serde_json::json!({
                "file": format!("f{}.rs", i),
                "line": i,
                "context": format!("item {}", i)
            }));
        }
        let c = make_check_result_with_items(serde_json::json!({"items": items_json}));
        let html = offender_rows_html(&c);
        assert!(html.contains("5 more findings"), "should indicate remaining findings");
    }

    // ── check_row_html ──────────────────────────────────────────────

    #[test]
    fn test_check_row_html_passed() {
        let c = CheckResult {
            name: "crap".into(),
            passed: true,
            score: Some(12.5),
            threshold: Some(15.0),
            message: "all good".into(),
            details: serde_json::Value::Null,
            severity: None,
            help: None,
            rule_id: None,
            findings: vec![],
        };
        let row = check_row_html(&c);
        assert!(row.contains("&#10003;"));  // checkmark
        assert!(row.contains("12.5"));
        assert!(row.contains("15.0"));
        assert!(row.contains("crap"));
    }

    #[test]
    fn test_check_row_html_failed() {
        let c = CheckResult {
            name: "secrets".into(),
            passed: false,
            score: None,
            threshold: None,
            message: "found secrets".into(),
            details: serde_json::Value::Null,
            severity: Some("high".into()),
            help: Some("use env vars".into()),
            rule_id: None,
            findings: vec![],
        };
        let row = check_row_html(&c);
        assert!(row.contains("&#10007;"));  // x mark
        assert!(row.contains("secrets"));
        assert!(row.contains("use env vars"));
        assert!(row.contains("high"));
    }

    // ── render_html_report ──────────────────────────────────────────

    #[test]
    fn test_render_html_report_basic_structure() {
        let report = CheckReport {
            passed: true,
            path: ".".into(),
            checks: vec![CheckResult {
                name: "crap".into(), passed: true, score: None, threshold: None,
                message: "ok".into(), details: serde_json::Value::Null,
                severity: None, help: None, rule_id: None, findings: vec![],
            }],
            summary: CheckSummary {
                total_checks: 1, passed_checks: 1, failed_checks: 0,
                functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
            },
            health_score: 100,
            grade: "A".into(),
            audit: None,
            file_summary: vec![],
        };
        let html = render_html_report(&report, "testproj", "2024-01-01", &["secrets"], &["crap"], &["licenses"]);
        assert!(html.contains("<html"));
        assert!(html.contains("testproj"));
        assert!(html.contains("PASSED"));
        assert!(html.contains("Health Score"));
    }

    // ── render_markdown_report ──────────────────────────────────────

    #[test]
    fn test_render_markdown_report_passed() {
        let report = CheckReport {
            passed: true,
            path: ".".into(),
            checks: vec![CheckResult {
                name: "crap".into(), passed: true, score: None, threshold: None,
                message: "ok".into(), details: serde_json::Value::Null,
                severity: None, help: None, rule_id: None, findings: vec![],
            }],
            summary: CheckSummary {
                total_checks: 1, passed_checks: 1, failed_checks: 0,
                functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
            },
            health_score: 100,
            grade: "A".into(),
            audit: None,
            file_summary: vec![],
        };
        let md = render_markdown_report(&report, "testproj", "2024-01-01", &["secrets"], &["crap"], &["licenses"]);
        assert!(md.contains("Cogent Audit Report"));
        assert!(md.contains("✅"));
    }

    #[test]
    fn test_render_markdown_report_with_file_heatmap() {
        let report = CheckReport {
            passed: false,
            path: ".".into(),
            checks: vec![],
            summary: CheckSummary {
                total_checks: 0, passed_checks: 0, failed_checks: 0,
                functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
            },
            health_score: 0,
            grade: "F".into(),
            audit: None,
            file_summary: vec![
                cogent_common::FileSummary {
                    file: "src/main.rs".into(),
                    issue_count: 5,
                    severity_score: 12,
                    findings_by_severity: std::collections::HashMap::new(),
                },
            ],
        };
        let md = render_markdown_report(&report, "testproj", "2024-01-01", &[], &[], &[]);
        assert!(md.contains("File Heatmap"));
        assert!(md.contains("src/main.rs"));
        assert!(md.contains("5"));
    }

    #[test]
    fn test_render_markdown_report_with_findings() {
        let report = CheckReport {
            passed: false,
            path: ".".into(),
            checks: vec![CheckResult {
                name: "secrets".into(), passed: false, score: None, threshold: None,
                message: "found secret".into(), details: serde_json::Value::Null,
                severity: Some("high".into()), help: None, rule_id: None,
                findings: vec![Finding {
                    file: "src/main.rs".into(), line: Some(42), column: None,
                    severity: "high".into(), message: "API key hardcoded".into(),
                    rule_id: "SEC-001".into(), fix_hint: "use env var".into(),
                    evidence: None, suggested_fix: None, controls: None,
                }],
            }],
            summary: CheckSummary {
                total_checks: 1, passed_checks: 0, failed_checks: 1,
                functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
            },
            health_score: 0,
            grade: "F".into(),
            audit: None,
            file_summary: vec![],
        };
        let md = render_markdown_report(&report, "testproj", "2024-01-01", &["secrets"], &[], &[]);
        assert!(md.contains("Findings by Tool"));
        assert!(md.contains("API key hardcoded"));
        assert!(md.contains("SEC-001"));
    }
}

