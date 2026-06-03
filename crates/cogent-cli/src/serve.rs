//! Serve command for cogent-cli.
//!
//! Provides an HTTP server for browsing Cogent reports.
//! Route handler logic is in pure functions for testability.

#![deny(clippy::all)]

use colored::Colorize;

// ═══════════════════════════════════════════
// PURE ROUTE HANDLER FUNCTIONS
// ═══════════════════════════════════════════

/// Generate the HTML index page listing reports in `history_dir`.
pub(crate) fn serve_index_html(history_dir: &str) -> String {
    let mut body = String::new();
    body.push_str("<!DOCTYPE html><html><head><meta charset='UTF-8'><meta http-equiv='refresh' content='30'><title>Cogent Reports</title><style>");
    body.push_str("body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;padding:40px;max-width:800px;margin:0 auto}");
    body.push_str("h1{font-size:24px;font-weight:800;margin-bottom:24px}");
    body.push_str("ul{list-style:none;padding:0}");
    body.push_str("li{border-bottom:1px solid #e2e8f0;padding:12px 0;display:flex;justify-content:space-between;align-items:center}");
    body.push_str("a{color:#6366f1;text-decoration:none;font-weight:600}");
    body.push_str("a:hover{text-decoration:underline}");
    body.push_str(".meta{font-size:12px;color:#94a3b8}");
    body.push_str(".nav{margin-bottom:20px}");
    body.push_str(".nav a{margin-right:16px;font-size:13px}");
    body.push_str("</style></head><body>");
    body.push_str("<div class='nav'><a href='/latest'>Latest Report</a><a href='/api/latest'>API (JSON)</a></div>");
    body.push_str("<h1>Cogent Reports</h1><ul>");

    if let Ok(entries) = std::fs::read_dir(history_dir) {
        let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        files.sort_by_key(|a| std::cmp::Reverse(a.file_name()));
        for entry in files.iter().take(50) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                let path = format!("/report/{}", name);
                body.push_str(&format!(
                    "<li><a href='{}'>{}</a><span class='meta'>{} bytes</span></li>",
                    path,
                    name,
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                ));
            }
        }
    }
    body.push_str("</ul></body></html>");
    body
}

/// Serve the latest HTML report (copies from known candidate filenames).
pub(crate) fn serve_latest_html() -> (String, u16) {
    let candidates = ["cogent-report.html", "check-report.html"];
    for cand in &candidates {
        if let Ok(html) = std::fs::read_to_string(cand) {
            return (html, 200);
        }
    }
    (
        "No latest report found. Run cogent check --ci to generate one.".to_string(),
        404,
    )
}

/// Serve the latest JSON summary.
pub(crate) fn serve_api_latest_json() -> (String, u16) {
    let candidates = ["cogent-summary.json", "check-report.json"];
    for cand in &candidates {
        if let Ok(json) = std::fs::read_to_string(cand) {
            return (json, 200);
        }
    }
    (
        r##"{"error":"No latest report found. Run cogent check --ci to generate one."}"##.to_string(),
        404,
    )
}

/// Serve a specific report file from the history directory.
pub(crate) fn serve_report_file(history_dir: &str, file_name: &str) -> (String, u16) {
    let file_path = std::path::Path::new(history_dir).join(file_name);
    match std::fs::read_to_string(&file_path) {
        Ok(content) => (content, 200),
        Err(_) => ("Not found".to_string(), 404),
    }
}

// ═══════════════════════════════════════════
// SERVER ENTRYPOINT
// ═══════════════════════════════════════════

/// Start the Cogent report HTTP server. Runs until Ctrl+C.
pub(crate) fn serve_command(port: u16, history_dir: &str) {
    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr).expect("Failed to start HTTP server");
    eprintln!(
        "  {} Cogent serve running at http://{}",
        "▶".cyan().bold(),
        addr.cyan()
    );
    eprintln!("  {} Press Ctrl+C to stop", "ℹ".cyan());

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let (content, status, content_type) = match url.as_str() {
            "/" => (serve_index_html(history_dir), 200, "text/html; charset=utf-8"),
            "/latest" => {
                let (content, status) = serve_latest_html();
                (content, status, "text/html; charset=utf-8")
            }
            "/api/latest" => {
                let (content, status) = serve_api_latest_json();
                (content, status, "application/json")
            }
            path if path.starts_with("/report/") => {
                let file_name = &path[8..];
                let (content, status) = serve_report_file(history_dir, file_name);
                let content_type = if status == 200 {
                    "application/json"
                } else {
                    "text/plain; charset=utf-8"
                };
                (content, status, content_type)
            }
            _ => ("Not found".to_string(), 404, "text/plain; charset=utf-8"),
        };            let response = tiny_http::Response::from_string(content)
            .with_status_code(tiny_http::StatusCode(status))
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes())
                    .unwrap(),
            );

        let _ = request.respond(response);
    }
}

pub(crate) fn open_in_browser(path: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", path])
        .spawn();
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── serve_index_html ──

    #[test]
    fn test_serve_index_html_empty_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let html = serve_index_html(dir.path().to_str().unwrap());
        assert!(html.starts_with("<!DOCTYPE html>"), "should start with doctype");
        assert!(html.contains("Cogent Reports"), "should have title");
        assert!(html.contains("</html>"), "should close html");
        // Empty dir → no list items
        assert!(!html.contains("<li>"), "empty dir should have no list items");
    }

    #[test]
    fn test_serve_index_html_with_json_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Create a mock report
        std::fs::write(dir.path().join("report-123.json"), "{}").expect("write");
        std::fs::write(dir.path().join("report-456.json"), "{}").expect("write");
        // Non-JSON file should be ignored
        std::fs::write(dir.path().join("notes.txt"), "hello").expect("write");

        let html = serve_index_html(dir.path().to_str().unwrap());
        assert!(html.contains("report-123.json"), "should list first report");
        assert!(html.contains("report-456.json"), "should list second report");
        assert!(!html.contains("notes.txt"), "should not list non-JSON files");
        assert!(html.contains("/report/report-123.json"), "should link to report");
        assert!(html.contains("/latest"), "should have latest link");
        assert!(html.contains("/api/latest"), "should have API link");
    }

    #[test]
    fn test_serve_index_html_nonexistent_dir() {
        let html = serve_index_html("/tmp/nonexistent-serve-test-dir-12345");
        assert!(html.starts_with("<!DOCTYPE html>"), "nonexistent dir should still produce HTML");
        assert!(!html.contains("<li>"), "nonexistent dir should have no items");
    }

    // ── serve_latest_html ──

    #[test]
    fn test_serve_latest_html_no_report() {
        // Run in a temp dir to avoid picking up real reports
        let dir = tempfile::tempdir().expect("tempdir");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_latest_html();

        // Restore cwd
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 404, "no report → 404");
        assert!(
            content.contains("No latest report found"),
            "should show helpful message"
        );
    }

    #[test]
    fn test_serve_latest_html_with_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("cogent-report.html"), "<html>report</html>")
            .expect("write");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_latest_html();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 200, "should serve report");
        assert!(content.contains("report"), "should contain report content");
    }

    #[test]
    fn test_serve_latest_html_fallback_to_check_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("check-report.html"), "<html>check</html>")
            .expect("write");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_latest_html();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 200, "should fallback to check-report.html");
        assert!(content.contains("check"), "should contain check report content");
    }

    #[test]
    fn test_serve_latest_html_prefers_cogent_over_check() {
        // Both exist → cogent-report.html takes priority
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("cogent-report.html"), "cogent").expect("write");
        std::fs::write(dir.path().join("check-report.html"), "check").expect("write");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_latest_html();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 200);
        assert_eq!(content, "cogent", "should prefer cogent-report.html");
    }

    // ── serve_api_latest_json ──

    #[test]
    fn test_serve_api_latest_json_no_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_api_latest_json();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 404);
        assert!(content.contains("error"), "should have error key");
    }

    #[test]
    fn test_serve_api_latest_json_with_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("cogent-summary.json"), r#"{"passed": true}"#)
            .expect("write");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_api_latest_json();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 200);
        assert!(content.contains("passed"), "should contain report data");
    }

    #[test]
    fn test_serve_api_latest_json_fallback_to_check_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("check-report.json"), r#"{"passed": false}"#)
            .expect("write");
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(dir.path()).ok();

        let (content, status) = serve_api_latest_json();
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }

        assert_eq!(status, 200);
        assert!(content.contains("false"), "should fallback to check-report.json");
    }

    // ── serve_report_file ──

    #[test]
    fn test_serve_report_file_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (content, status) = serve_report_file(dir.path().to_str().unwrap(), "missing.json");
        assert_eq!(status, 404);
        assert_eq!(content, "Not found");
    }

    #[test]
    fn test_serve_report_file_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("report.json"), r#"{"key": "value"}"#).expect("write");
        let (content, status) = serve_report_file(dir.path().to_str().unwrap(), "report.json");
        assert_eq!(status, 200);
        assert!(content.contains("value"), "should contain file content");
    }

    #[test]
    fn test_serve_report_file_nonexistent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (content, status) =
            serve_report_file(dir.path().to_str().unwrap(), "nonexistent-file.json");
        assert_eq!(status, 404, "non-existent file should 404");
        assert_eq!(content, "Not found");
    }

    #[test]
    fn test_serve_report_file_subdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        std::fs::write(dir.path().join("sub/data.json"), "data").expect("write");
        let (content, status) = serve_report_file(dir.path().to_str().unwrap(), "sub/data.json");
        assert_eq!(status, 200);
        assert_eq!(content, "data");
    }
}
