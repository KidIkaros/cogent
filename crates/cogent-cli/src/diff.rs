//! Diff command for cogent-cli.

#![deny(clippy::all)]

use crate::progress::health_score;
use crate::report::html_escape;
use crate::types::CheckReport;
use colored::Colorize;

pub(crate) fn diff_command(before_path: &str, after_path: &str, format: &str) -> i32 {
    let load = |p: &str| -> Option<CheckReport> {
        let content = std::fs::read_to_string(p)
            .map_err(|e| eprintln!("Error reading {}: {}", p, e))
            .ok()?;
        serde_json::from_str(&content)
            .map_err(|e| eprintln!("Error parsing {}: {}", p, e))
            .ok()
    };

    let before = match load(before_path) {
        Some(r) => r,
        None => return 2,
    };
    let after = match load(after_path) {
        Some(r) => r,
        None => return 2,
    };

    let mut regressions: Vec<&str> = Vec::new();
    let mut fixes: Vec<&str> = Vec::new();
    let mut unchanged_pass = 0usize;
    let mut unchanged_fail = 0usize;

    for ac in &after.checks {
        if let Some(bc) = before.checks.iter().find(|b| b.name == ac.name) {
            match (bc.passed, ac.passed) {
                (true, false) => regressions.push(&ac.name),
                (false, true) => fixes.push(&ac.name),
                (true, true) => unchanged_pass += 1,
                (false, false) => unchanged_fail += 1,
            }
        }
    }

    let new_checks: Vec<&str> = after
        .checks
        .iter()
        .filter(|ac| !before.checks.iter().any(|bc| bc.name == ac.name))
        .map(|ac| ac.name.as_str())
        .collect();

    if format == "html" {
        let html = diff_html(
            &before,
            &after,
            before_path,
            after_path,
            &regressions,
            &fixes,
            &new_checks,
            unchanged_pass,
            unchanged_fail,
        );
        println!("{}", html);
        return if regressions.is_empty() { 0 } else { 1 };
    }

    eprintln!();
    eprintln!(
        "  {} {} → {}",
        "diff".bright_black(),
        before_path.cyan(),
        after_path.cyan()
    );
    eprintln!();

    if regressions.is_empty() && fixes.is_empty() && new_checks.is_empty() {
        eprintln!(
            "  {} No changes — {} pass, {} fail (unchanged)",
            "◉".bright_black(),
            unchanged_pass,
            unchanged_fail
        );
    } else {
        for name in &fixes {
            eprintln!(
                "  {} {} {}",
                "↑".green().bold(),
                name.green().bold(),
                "now passing".green()
            );
        }
        for name in &regressions {
            eprintln!(
                "  {} {} {}",
                "↓".red().bold(),
                name.red().bold(),
                "now failing".red()
            );
        }
        for name in &new_checks {
            let status = after
                .checks
                .iter()
                .find(|c| c.name == *name)
                .map(|c| c.passed)
                .unwrap_or(false);
            let icon = if status {
                "✓".green().bold().to_string()
            } else {
                "✗".red().bold().to_string()
            };
            eprintln!("  {} {} {}", icon, name, "(new check)".bright_black());
        }
        eprintln!();
        if unchanged_pass > 0 || unchanged_fail > 0 {
            eprintln!(
                "  {} {} unchanged passing, {} unchanged failing",
                "◉".bright_black(),
                unchanged_pass,
                unchanged_fail
            );
        }
    }

    let score_before = {
        let (s, g) = health_score(&before.checks);
        format!("{}/100 ({})", s, g)
    };
    let score_after = {
        let (s, g) = health_score(&after.checks);
        format!("{}/100 ({})", s, g)
    };
    eprintln!();
    eprintln!(
        "  {} Health: {} → {}",
        "▶".cyan(),
        score_before.bright_black(),
        score_after.cyan().bold()
    );
    eprintln!();

    if regressions.is_empty() {
        0
    } else {
        1
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn diff_html(
    before: &CheckReport,
    after: &CheckReport,
    before_path: &str,
    after_path: &str,
    regressions: &[&str],
    fixes: &[&str],
    new_checks: &[&str],
    unchanged_pass: usize,
    unchanged_fail: usize,
) -> String {
    let (health_before, grade_before) = health_score(&before.checks);
    let (health_after, grade_after) = health_score(&after.checks);
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let mut rows = String::new();
    for ac in &after.checks {
        let bc = before.checks.iter().find(|b| b.name == ac.name);
        let change = match bc {
            Some(b) => {
                if !b.passed && ac.passed {
                    "<span style=\"color:#22c55e;font-weight:700\">↑ FIXED</span>"
                } else if b.passed && !ac.passed {
                    "<span style=\"color:#ef4444;font-weight:700\">↓ REGRESSION</span>"
                } else {
                    "<span style=\"color:#9ca3af\">—</span>"
                }
            }
            None => "<span style=\"color:#6366f1;font-weight:700\">NEW</span>",
        };
        let status = if ac.passed {
            "<span style=\"color:#22c55e\">✓ PASS</span>"
        } else {
            "<span style=\"color:#ef4444\">✗ FAIL</span>"
        };
        rows.push_str(&format!(
            "<tr style=\"border-bottom:1px solid #f3f4f6\"><td style=\"padding:10px 14px;font-weight:600\">{}</td><td style=\"padding:10px 14px\">{}</td><td style=\"padding:10px 14px\">{}</td></tr>",
            html_escape(&ac.name), status, change
        ));
    }

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cogent Diff — {} → {}</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;line-height:1.5;padding:40px;max-width:900px;margin:0 auto}}
.card{{background:#fff;border-radius:12px;padding:28px;box-shadow:0 1px 3px rgba(0,0,0,.06);margin-bottom:28px;border:1px solid #f1f5f9}}
.card-title{{font-size:15px;font-weight:700;color:#0f172a;margin-bottom:16px}}
.score-grid{{display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-bottom:28px}}
.score-card{{background:#fff;border-radius:12px;padding:24px;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.06)}}
.score-card .n{{font-size:36px;font-weight:900}}
.score-card .lbl{{font-size:11px;color:#94a3b8;margin-top:6px;text-transform:uppercase}}
.arrow{{text-align:center;font-size:28px;color:#64748b;margin:12px 0}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
th{{padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;border-bottom:2px solid #e5e7eb}}
.footer{{font-size:12px;color:#94a3b8;text-align:center;margin-top:48px}}
</style>
</head>
<body>
<h1 style="font-size:22px;font-weight:800;margin-bottom:8px">Cogent Diff Report</h1>
<p style="color:#6b7280;font-size:13px;margin-bottom:28px">{} &nbsp;&middot;&nbsp; {} → {}</p>

<div class="score-grid">
  <div class="score-card">
    <div class="n" style="color:#1e293b">{}/100</div>
    <div class="lbl">Before — Grade {}</div>
  </div>
  <div class="score-card">
    <div class="n" style="color:#1e293b">{}/100</div>
    <div class="lbl">After — Grade {}</div>
  </div>
</div>

<div class="card">
  <div class="card-title">Summary</div>
  <p style="font-size:13px;color:#475569;line-height:1.6">
    {} regression(s), {} fix(es), {} new check(s).<br>
    {} unchanged passing, {} unchanged failing.
  </p>
</div>

<div class="card">
  <div class="card-title">Per-Check Comparison</div>
  <table>
    <thead><tr><th>Check</th><th>Status</th><th>Change</th></tr></thead>
    <tbody>{}</tbody>
  </table>
</div>

<div class="footer">Generated by Cogent — {}</div>
</body>
</html>"##,
        html_escape(before_path),
        html_escape(after_path),
        date,
        html_escape(before_path),
        html_escape(after_path),
        health_before,
        grade_before,
        health_after,
        grade_after,
        regressions.len(),
        fixes.len(),
        new_checks.len(),
        unchanged_pass,
        unchanged_fail,
        rows,
        date
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CheckResult, CheckSummary};
    use serde_json::json;

    fn make_check(name: &str, passed: bool) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            passed,
            score: None,
            threshold: None,
            message: if passed { "ok".into() } else { "fail".into() },
            details: json!({}),
            severity: None,
            help: None,
            rule_id: None,
            findings: vec![],
        }
    }

    fn make_report(checks: Vec<CheckResult>) -> CheckReport {
        let passed = checks.iter().all(|c| c.passed);
        let total = checks.len();
        let passed_count = checks.iter().filter(|c| c.passed).count();
        let (health, grade) = health_score(&checks);
        CheckReport {
            passed,
            path: ".".into(),
            checks,
            summary: CheckSummary {
                total_checks: total,
                passed_checks: passed_count,
                failed_checks: total - passed_count,
                functions_analyzed: 0,
                avg_complexity: 0.0,
                avg_crap: 0.0,
            },
            health_score: health,
            grade: grade.to_string(),
            audit: None,
            file_summary: vec![],
        }
    }

    #[test]
    fn test_diff_html_basic_structure() {
        let before = make_report(vec![
            make_check("check-a", true),
            make_check("check-b", true),
        ]);
        let after = make_report(vec![
            make_check("check-a", true),
            make_check("check-b", true),
        ]);
        let html = diff_html(
            &before,
            &after,
            "before.json",
            "after.json",
            &[],
            &[],
            &[],
            2,
            0,
        );
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("Cogent Diff Report"));
        assert!(html.contains("before.json"));
        assert!(html.contains("after.json"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_diff_html_regression_and_fix() {
        let before = make_report(vec![
            make_check("check-a", true),
            make_check("check-b", false),
            make_check("check-c", true),
        ]);
        let after = make_report(vec![
            make_check("check-a", false),
            make_check("check-b", true),
            make_check("check-c", true),
        ]);
        let html = diff_html(
            &before,
            &after,
            "b.json",
            "a.json",
            &["check-a"],
            &["check-b"],
            &[],
            1,
            0,
        );
        assert!(html.contains("REGRESSION"));
        assert!(html.contains("FIXED"));
        assert!(html.contains("1 regression(s), 1 fix(es)"));
    }

    #[test]
    fn test_diff_html_new_check() {
        let before = make_report(vec![make_check("check-a", true)]);
        let after = make_report(vec![
            make_check("check-a", true),
            make_check("check-d", false),
        ]);
        let html = diff_html(
            &before,
            &after,
            "b.json",
            "a.json",
            &[],
            &[],
            &["check-d"],
            1,
            0,
        );
        assert!(html.contains("NEW"));
        assert!(html.contains("check-d"));
        assert!(html.contains("FAIL"));
    }

    #[test]
    fn test_diff_html_health_scores() {
        let before = make_report(vec![make_check("a", true), make_check("b", true)]);
        let after = make_report(vec![make_check("a", true), make_check("b", false)]);
        let html = diff_html(&before, &after, "b.json", "a.json", &["b"], &[], &[], 1, 0);
        assert!(html.contains("100/100"));
        assert!(html.contains("50/100"));
        assert!(html.contains("Grade A"));
        assert!(html.contains("Grade D"));
    }

    #[test]
    fn test_diff_html_summary_counts() {
        let (_r, _f, _n, up, uf) = (3usize, 2, 1, 5, 4);
        let before = make_report(vec![make_check("keep", true)]);
        let after = make_report(vec![make_check("keep", true)]);
        let html = diff_html(
            &before,
            &after,
            "b.json",
            "a.json",
            &["a", "b", "c"],
            &["d", "e"],
            &["f"],
            up,
            uf,
        );
        assert!(html.contains("3 regression(s), 2 fix(es), 1 new check(s)"));
        assert!(html.contains("5 unchanged passing, 4 unchanged failing"));
    }

    #[test]
    fn test_diff_html_footer() {
        let before = make_report(vec![]);
        let after = make_report(vec![]);
        let html = diff_html(&before, &after, "b.json", "a.json", &[], &[], &[], 0, 0);
        assert!(html.contains("Generated by Cogent"));
    }
}
