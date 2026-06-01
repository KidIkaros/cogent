//! Serve command for cogent-cli.

#![deny(clippy::all)]

use colored::Colorize;

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
        let response = match url.as_str() {
            "/" => {
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
                tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    )
                    .unwrap(),
                )
            }
            "/latest" => {
                let candidates = ["cogent-report.html", "check-report.html"];
                let mut served = false;
                let mut content = String::new();
                for cand in &candidates {
                    if let Ok(html) = std::fs::read_to_string(cand) {
                        content = html;
                        served = true;
                        break;
                    }
                }
                if served {
                    tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"text/html; charset=utf-8"[..],
                        )
                        .unwrap(),
                    )
                } else {
                    tiny_http::Response::from_string(
                        "No latest report found. Run cogent check --ci to generate one.",
                    )
                    .with_status_code(tiny_http::StatusCode(404))
                }
            }
            "/api/latest" => {
                let candidates = ["cogent-summary.json", "check-report.json"];
                let mut served = false;
                let mut content = String::new();
                for cand in &candidates {
                    if let Ok(json) = std::fs::read_to_string(cand) {
                        content = json;
                        served = true;
                        break;
                    }
                }
                if served {
                    tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    )
                } else {
                    tiny_http::Response::from_string(r##"{"error":"No latest report found. Run cogent check --ci to generate one."}"##).with_status_code(tiny_http::StatusCode(404)).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                }
            }
            path if path.starts_with("/report/") => {
                let file_name = &path[8..];
                let file_path = std::path::Path::new(history_dir).join(file_name);
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    ),
                    Err(_) => tiny_http::Response::from_string("Not found")
                        .with_status_code(tiny_http::StatusCode(404)),
                }
            }
            _ => tiny_http::Response::from_string("Not found")
                .with_status_code(tiny_http::StatusCode(404)),
        };
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

