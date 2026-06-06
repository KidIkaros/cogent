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
    body.push_str("<!DOCTYPE html><html lang='en'><head><meta charset='UTF-8'><meta http-equiv='refresh' content='30'><title>Cogent Reports</title>");
    body.push_str("<link rel='preconnect' href='https://fonts.googleapis.com'><link rel='preconnect' href='https://fonts.gstatic.com' crossorigin>");
    body.push_str("<link href='https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;500;700;800&display=swap' rel='stylesheet'>");
    body.push_str("<style>");
    body.push_str(":root{--bg:#f8fafc;--surface:#fff;--surface-alt:#f1f5f9;--border:#e2e8f0;--border-light:#f1f5f9;--text:#0f172a;--text-secondary:#475569;--text-muted:#94a3b8;--accent:#6366f1;--accent-light:#818cf8;--green:#22c55e;--red:#ef4444;--shadow-sm:0 1px 2px rgba(0,0,0,.04),0 1px 3px rgba(0,0,0,.06)}");
    body.push_str("[data-theme=\"dark\"]{--bg:#0c1222;--surface:#131b2e;--surface-alt:#1a2340;--border:#1e293b;--border-light:#1e293b;--text:#f1f5f9;--text-secondary:#94a3b8;--text-muted:#64748b;--accent:#818cf8;--accent-light:#a5b4fc;--green:#34d399;--red:#f87171;--shadow-sm:0 1px 3px rgba(0,0,0,.3)}");
    body.push_str("*{box-sizing:border-box;margin:0;padding:0}");
    body.push_str("body{font-family:'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif;background:var(--bg);color:var(--text);padding:48px;max-width:860px;margin:0 auto;-webkit-font-smoothing:antialiased;line-height:1.6}");
    body.push_str("h1{font-size:26px;font-weight:800;margin-bottom:8px;letter-spacing:-.3px}");
    body.push_str(".subtitle{font-size:13px;color:var(--text-muted);margin-bottom:32px}");
    body.push_str("ul{list-style:none;padding:0}");
    body.push_str("li{border-bottom:1px solid var(--border-light);padding:14px 0;display:flex;justify-content:space-between;align-items:center;transition:background .12s}");
    body.push_str("li:hover{background:var(--surface-alt);border-radius:8px;padding-left:12px;padding-right:12px}");
    body.push_str("a{color:var(--accent);text-decoration:none;font-weight:600;transition:color .15s}");
    body.push_str("a:hover{color:var(--accent-light)}");
    body.push_str(".meta{font-size:12px;color:var(--text-muted);font-family:ui-monospace,monospace}");
    body.push_str(".nav{margin-bottom:28px;display:flex;gap:8px}");
    body.push_str(".nav a{padding:8px 16px;border-radius:8px;font-size:13px;background:var(--surface);border:1px solid var(--border);color:var(--text-secondary);font-weight:500;transition:all .15s}");
    body.push_str(".nav a:hover{background:var(--accent);color:#fff;border-color:var(--accent)}");
    body.push_str(".footer{margin-top:40px;font-size:12px;color:var(--text-muted);text-align:center}");
    body.push_str(".theme-toggle{position:fixed;top:16px;right:16px;padding:8px 16px;border-radius:8px;background:var(--surface);border:1px solid var(--border);cursor:pointer;font-size:12px;color:var(--text-muted);transition:all .15s;font-family:inherit}");
    body.push_str(".theme-toggle:hover{color:var(--text);border-color:var(--accent)}");
    body.push_str("</style></head><body>");
    body.push_str("<button class='theme-toggle' onclick='toggleTheme()'>&#9788; Theme</button>");
    body.push_str("<h1>Cogent Reports</h1><div class='subtitle'>Browse audit reports and quality snapshots</div>");
    body.push_str("<div class='nav'><a href='/latest'>Latest Report</a><a href='/api/latest'>API (JSON)</a></div>");
    body.push_str("<ul>");

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
    body.push_str("</ul><div class='footer'>Cogent — automated code quality &amp; security auditing</div>");
    body.push_str("<script>function toggleTheme(){var h=document.documentElement;var d=h.getAttribute('data-theme')==='dark';h.setAttribute('data-theme',d?'':'dark');localStorage.setItem('cogent-theme',d?'':'dark')}(function(){var s=localStorage.getItem('cogent-theme');if(s==='dark')document.documentElement.setAttribute('data-theme','dark')})()</script>");
    body.push_str("</body></html>");
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
