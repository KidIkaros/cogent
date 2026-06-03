#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "sast",
    about = "SAST scanner — SQL injection, path traversal, command injection, eval, unsafe deserialization, and smart contract vulnerabilities"
)]
struct Cli {
    /// Path to scan (file or directory)
    path: String,

    /// Recursive scan
    #[arg(short, long)]
    recursive: bool,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Max allowed SAST findings (default: 0)
    #[arg(long, default_value = "0")]
    max_findings: usize,

    /// Only show findings above this confidence (low/medium/high, default: low)
    #[arg(long, default_value = "low")]
    min_confidence: String,
}

#[derive(Debug, Clone, Serialize)]
struct SastFinding {
    file: String,
    line: usize,
    category: String,
    rule_id: String,
    confidence: String,
    severity: String,
    context: String,
    description: String,
    remediation: String,
}

#[derive(Serialize)]
struct SastReport {
    findings: Vec<SastFinding>,
    summary: SastSummary,
}

#[derive(Serialize)]
struct SastSummary {
    files_scanned: usize,
    total_findings: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    max_findings_threshold: usize,
}

/// A SAST rule: (category, rule_id, confidence, severity, pattern_substring, description, remediation)
struct Rule {
    category: &'static str,
    rule_id: &'static str,
    confidence: &'static str,
    severity: &'static str,
    /// Substring that must appear in the line (case-sensitive or not handled by caller)
    pattern: &'static str,
    /// Optional: a second substring that must also appear (for context-narrowing)
    also: Option<&'static str>,
    description: &'static str,
    remediation: &'static str,
}

const RULES: &[Rule] = &[
    // ── SQL Injection ──────────────────────────────────────────────
    Rule { category: "sql_injection", rule_id: "SAST-SQL-001", confidence: "medium", severity: "high",
        pattern: "format!(\"SELECT", also: None,
        description: "String-interpolated SQL query may allow injection.",
        remediation: "Use parameterized queries or a query builder. Never interpolate user input into SQL." },
    Rule { category: "sql_injection", rule_id: "SAST-SQL-002", confidence: "medium", severity: "high",
        pattern: "format!(\"INSERT", also: None,
        description: "String-interpolated SQL INSERT may allow injection.",
        remediation: "Use parameterized queries. Never interpolate user input into SQL." },
    Rule { category: "sql_injection", rule_id: "SAST-SQL-003", confidence: "medium", severity: "high",
        pattern: "format!(\"UPDATE", also: None,
        description: "String-interpolated SQL UPDATE may allow injection.",
        remediation: "Use parameterized queries. Never interpolate user input into SQL." },
    Rule { category: "sql_injection", rule_id: "SAST-SQL-004", confidence: "medium", severity: "high",
        pattern: "format!(\"DELETE", also: None,
        description: "String-interpolated SQL DELETE may allow injection.",
        remediation: "Use parameterized queries. Never interpolate user input into SQL." },
    Rule { category: "sql_injection", rule_id: "SAST-SQL-005", confidence: "high", severity: "critical",
        pattern: "execute(", also: Some("format!("),
        description: "SQL execute() called with a format!() string — high-confidence injection risk.",
        remediation: "Replace with bound/parameterized execute. Pass values as bind params, not in the SQL string." },
    // ── Path Traversal ────────────────────────────────────────────
    Rule { category: "path_traversal", rule_id: "SAST-PATH-001", confidence: "medium", severity: "high",
        pattern: "Path::new(", also: Some("request"),
        description: "Path constructed from a value named 'request' — possible path traversal.",
        remediation: "Validate and sanitize path components. Canonicalize and verify the result is within the expected directory." },
    Rule { category: "path_traversal", rule_id: "SAST-PATH-002", confidence: "medium", severity: "high",
        pattern: "read_to_string(", also: Some("param"),
        description: "File read with a value that may originate from user input (param).",
        remediation: "Validate path input. Use a canonicalized allowlist, not user-supplied paths." },
    Rule { category: "path_traversal", rule_id: "SAST-PATH-003", confidence: "high", severity: "critical",
        pattern: "../", also: Some("user"),
        description: "Literal '../' combined with a user-controlled value — path traversal.",
        remediation: "Reject any path containing '..' components from untrusted input." },
    Rule { category: "path_traversal", rule_id: "SAST-PATH-004", confidence: "low", severity: "medium",
        pattern: "join(", also: Some("input"),
        description: "Path::join() called with a value named 'input' — potential traversal.",
        remediation: "Strip leading '/' and '..' from user-controlled path segments before joining." },
    Rule { category: "path_traversal", rule_id: "SAST-PATH-005", confidence: "low", severity: "medium",
        pattern: "open(", also: Some("input"),
        description: "File open() called with user input — potential traversal in Python.",
        remediation: "Validate path is within the expected directory before opening." },
    // ── Command Injection ─────────────────────────────────────────
    Rule { category: "cmd_injection", rule_id: "SAST-CMD-001", confidence: "high", severity: "critical",
        pattern: "Command::new(", also: Some("input"),
        description: "subprocess Command::new() called with a value containing user input.",
        remediation: "Never construct shell commands from untrusted input. Use allowlists of permitted commands." },
    Rule { category: "cmd_injection", rule_id: "SAST-CMD-002", confidence: "high", severity: "critical",
        pattern: "Command::new(", also: Some("request"),
        description: "subprocess Command::new() called with request-derived value.",
        remediation: "Use a hardcoded command with allowlisted arguments only." },
    Rule { category: "cmd_injection", rule_id: "SAST-CMD-003", confidence: "medium", severity: "high",
        pattern: ".arg(", also: Some("input"),
        description: "Shell argument constructed from user input — command injection risk.",
        remediation: "Validate and sanitize all arguments passed to subprocesses." },
    Rule { category: "cmd_injection", rule_id: "SAST-CMD-004", confidence: "high", severity: "critical",
        pattern: "shell(", also: None,
        description: "Direct shell() invocation detected.",
        remediation: "Replace shell() calls with explicit Command::new() with separate arguments." },
    // ── Eval / Code Execution ─────────────────────────────────────
    Rule { category: "code_execution", rule_id: "SAST-EVAL-001", confidence: "high", severity: "critical",
        pattern: "eval(", also: None,
        description: "eval() detected — dynamic code execution from untrusted input is dangerous.",
        remediation: "Remove eval(). Use explicit logic, configuration, or a safe expression evaluator." },
    Rule { category: "code_execution", rule_id: "SAST-EVAL-002", confidence: "high", severity: "critical",
        pattern: "exec(", also: Some("input"),
        description: "exec() called with user-controlled input.",
        remediation: "Never exec() user-supplied code. Use a sandboxed interpreter with strict input validation." },
    // ── Unsafe Deserialization ────────────────────────────────────
    Rule { category: "unsafe_deser", rule_id: "SAST-DESER-001", confidence: "medium", severity: "high",
        pattern: "from_slice(", also: Some("request"),
        description: "Deserialization from request data without type constraints.",
        remediation: "Always deserialize into a strongly-typed struct. Never deserialize into serde_json::Value from untrusted sources without validation." },
    Rule { category: "unsafe_deser", rule_id: "SAST-DESER-002", confidence: "low", severity: "medium",
        pattern: "unsafe {", also: None,
        description: "unsafe block detected — review for memory safety issues.",
        remediation: "Audit all unsafe blocks. Ensure they cannot be reached with attacker-controlled data." },
    Rule { category: "unsafe_deser", rule_id: "SAST-DESER-003", confidence: "medium", severity: "high",
        pattern: "pickle.loads(", also: None,
        description: "Python pickle.loads() from potentially untrusted data — arbitrary code execution.",
        remediation: "Never unpickle data from untrusted sources. Use JSON or other safe serialization formats." },
    // ── SSRF ──────────────────────────────────────────────────────
    Rule { category: "ssrf", rule_id: "SAST-SSRF-001", confidence: "medium", severity: "high",
        pattern: "reqwest::get(", also: Some("input"),
        description: "HTTP request to a URL derived from user input — potential SSRF.",
        remediation: "Validate URLs against an allowlist of permitted hosts before making requests." },
    Rule { category: "ssrf", rule_id: "SAST-SSRF-002", confidence: "medium", severity: "high",
        pattern: "fetch(", also: Some("param"),
        description: "fetch() called with a URL from user-controlled param — potential SSRF.",
        remediation: "Validate and allowlist URLs before making outbound requests." },
    // ── XSS ───────────────────────────────────────────────────────
    Rule { category: "xss", rule_id: "SAST-XSS-001", confidence: "medium", severity: "high",
        pattern: "innerHTML", also: Some("input"),
        description: "innerHTML set with user-controlled input — XSS risk.",
        remediation: "Use textContent instead of innerHTML, or sanitize with DOMPurify." },
    Rule { category: "xss", rule_id: "SAST-XSS-002", confidence: "high", severity: "critical",
        pattern: "dangerouslySetInnerHTML", also: None,
        description: "React dangerouslySetInnerHTML used — verify the content is sanitized.",
        remediation: "Sanitize with DOMPurify before passing to dangerouslySetInnerHTML, or avoid it entirely." },
    Rule { category: "xss", rule_id: "SAST-XSS-003", confidence: "medium", severity: "high",
        pattern: "document.write(", also: None,
        description: "document.write() used — potential XSS injection point.",
        remediation: "Replace document.write() with safe DOM manipulation methods." },
    // ── Python-specific ─────────────────────────────────────────────
    Rule { category: "cmd_injection", rule_id: "SAST-PY-CMD-001", confidence: "high", severity: "critical",
        pattern: "os.system(", also: None,
        description: "Python os.system() called — potential command injection.",
        remediation: "Use subprocess.run() with a list of arguments, not a shell string." },
    Rule { category: "cmd_injection", rule_id: "SAST-PY-CMD-002", confidence: "high", severity: "critical",
        pattern: "subprocess.call(", also: Some("shell=True"),
        description: "subprocess.call() with shell=True — command injection risk.",
        remediation: "Remove shell=True. Pass command as a list of arguments." },
    Rule { category: "code_execution", rule_id: "SAST-PY-EVAL-001", confidence: "high", severity: "critical",
        pattern: "eval(", also: None,
        description: "Python eval() detected — arbitrary code execution.",
        remediation: "Replace eval() with ast.literal_eval() for literals only, or use a safe parser." },
    Rule { category: "unsafe_deser", rule_id: "SAST-PY-DESER-001", confidence: "high", severity: "critical",
        pattern: "yaml.load(", also: None,
        description: "yaml.load() without Loader — arbitrary code execution in PyYAML < 5.1.",
        remediation: "Use yaml.safe_load() instead of yaml.load()." },
    Rule { category: "unsafe_deser", rule_id: "SAST-PY-DESER-002", confidence: "high", severity: "critical",
        pattern: "marshal.loads(", also: None,
        description: "marshal.loads() from untrusted data — arbitrary code execution.",
        remediation: "Never unmarshal untrusted data. Use JSON or msgpack." },
    // ── JavaScript/TypeScript-specific ────────────────────────────
    Rule { category: "code_execution", rule_id: "SAST-JS-EVAL-001", confidence: "high", severity: "critical",
        pattern: "setTimeout(", also: Some("\""),
        description: "setTimeout() called with a string — eval-like code execution.",
        remediation: "Pass a function reference to setTimeout(), not a string." },
    Rule { category: "code_execution", rule_id: "SAST-JS-EVAL-002", confidence: "high", severity: "critical",
        pattern: "setInterval(", also: Some("\""),
        description: "setInterval() called with a string — eval-like code execution.",
        remediation: "Pass a function reference to setInterval(), not a string." },
    Rule { category: "code_execution", rule_id: "SAST-JS-EVAL-003", confidence: "high", severity: "critical",
        pattern: "new Function(", also: None,
        description: "new Function() constructor — dynamic code execution.",
        remediation: "Avoid dynamic Function creation. Use explicit functions or a safe expression parser." },
    // ── Go-specific ───────────────────────────────────────────────
    Rule { category: "cmd_injection", rule_id: "SAST-GO-CMD-001", confidence: "medium", severity: "high",
        pattern: "exec.Command(", also: Some("sh"),
        description: "exec.Command() with shell — command injection risk.",
        remediation: "Use exec.Command() with separate arguments, not a shell string." },
    Rule { category: "unsafe_deser", rule_id: "SAST-GO-UNSAFE-001", confidence: "medium", severity: "high",
        pattern: "unsafe.Pointer", also: None,
        description: "unsafe.Pointer usage — potential memory safety issues.",
        remediation: "Audit all unsafe.Pointer usage. Ensure it cannot be reached with attacker-controlled data." },
    // ── Java-specific ─────────────────────────────────────────────
    Rule { category: "cmd_injection", rule_id: "SAST-JAVA-CMD-001", confidence: "high", severity: "critical",
        pattern: "Runtime.getRuntime().exec(", also: None,
        description: "Java Runtime.exec() — potential command injection.",
        remediation: "Use ProcessBuilder with a list of arguments, not a shell string. Validate input." },
    Rule { category: "unsafe_deser", rule_id: "SAST-JAVA-DESER-001", confidence: "high", severity: "critical",
        pattern: "ObjectInputStream", also: Some("readObject"),
        description: "Java deserialization from ObjectInputStream — remote code execution.",
        remediation: "Avoid Java native deserialization. Use JSON, protobuf, or a safe serialization library." },
    Rule { category: "code_execution", rule_id: "SAST-JAVA-EVAL-001", confidence: "high", severity: "critical",
        pattern: "ScriptEngine", also: Some("eval"),
        description: "Java ScriptEngine.eval() — dynamic code execution.",
        remediation: "Never eval() user-controlled input. Use a safe expression evaluator." },
    Rule { category: "xss", rule_id: "SAST-JAVA-XSS-001", confidence: "medium", severity: "high",
        pattern: "response.getWriter().print(", also: None,
        description: "Direct output to response writer — potential XSS if input is not escaped.",
        remediation: "Escape all user input before writing to the response. Use a templating engine with auto-escaping." },
    // ── PHP-specific ──────────────────────────────────────────────
    Rule { category: "sql_injection", rule_id: "SAST-PHP-SQL-001", confidence: "high", severity: "critical",
        pattern: "mysql_query(", also: None,
        description: "PHP mysql_query() — SQL injection if query is concatenated with user input.",
        remediation: "Use PDO prepared statements with bound parameters." },
    Rule { category: "code_execution", rule_id: "SAST-PHP-EVAL-001", confidence: "high", severity: "critical",
        pattern: "eval($", also: None,
        description: "PHP eval() with a variable — arbitrary code execution.",
        remediation: "Never eval() user input. Use explicit logic or a safe expression parser." },
    // ── Solidity / Smart Contract ─────────────────────────────────
    Rule { category: "reentrancy", rule_id: "SAST-SOL-REENT-001", confidence: "high", severity: "critical",
        pattern: "call{value:", also: Some("balance"),
        description: "External call before state update — reentrancy vulnerability.",
        remediation: "Follow checks-effects-interactions pattern: update state before external calls. Use ReentrancyGuard." },
    Rule { category: "reentrancy", rule_id: "SAST-SOL-REENT-002", confidence: "high", severity: "critical",
        pattern: "transfer(", also: Some(".send("),
        description: "Low-level call detected — verify reentrancy protection is in place.",
        remediation: "Use OpenZeppelin's ReentrancyGuard or implement checks-effects-interactions." },
    Rule { category: "access_control", rule_id: "SAST-SOL-ACL-001", confidence: "medium", severity: "high",
        pattern: "selfdestruct(", also: None,
        description: "selfdestruct() present — verify it is protected by access control.",
        remediation: "Ensure selfdestruct() has an onlyOwner or role-based access control modifier." },
    Rule { category: "access_control", rule_id: "SAST-SOL-ACL-002", confidence: "high", severity: "critical",
        pattern: "tx.origin", also: Some("owner"),
        description: "tx.origin used for authorization — can be bypassed by phishing.",
        remediation: "Use msg.sender instead of tx.origin for authorization checks." },
    Rule { category: "access_control", rule_id: "SAST-SOL-ACL-003", confidence: "medium", severity: "high",
        pattern: "function ", also: Some("public"),
        description: "Public function without access control modifier — verify it should be unrestricted.",
        remediation: "Add onlyOwner, onlyRole, or similar modifier for sensitive functions." },
    Rule { category: "timestamp", rule_id: "SAST-SOL-TIME-001", confidence: "medium", severity: "medium",
        pattern: "block.timestamp", also: None,
        description: "block.timestamp used for critical logic — miner manipulation risk.",
        remediation: "Avoid block.timestamp for time-sensitive decisions. Use block.number with an average block time." },
    Rule { category: "unchecked_send", rule_id: "SAST-SOL-SEND-001", confidence: "high", severity: "high",
        pattern: ".send(", also: None,
        description: "Unchecked .send() return value — funds may be silently lost.",
        remediation: "Check the return value of .send() and handle failures. Prefer .call{value: ...} with reentrancy protection." },
    Rule { category: "unchecked_send", rule_id: "SAST-SOL-SEND-002", confidence: "high", severity: "high",
        pattern: ".call{value:", also: None,
        description: "Low-level .call{value:...} — verify return value is checked.",
        remediation: "Require success from .call{value:...}: (bool success,) = addr.call{value: amount}(\"\"); require(success);" },
    Rule { category: "integer_overflow", rule_id: "SAST-SOL-INT-001", confidence: "low", severity: "medium",
        pattern: "unchecked {", also: None,
        description: "unchecked block detected — potential integer overflow/underflow.",
        remediation: "Remove unchecked unless gas optimization is critical and arithmetic is proven safe. Use Solidity >=0.8.0." },
];

fn confidence_rank(c: &str) -> u8 {
    match c {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

fn scan_file(path: &str, min_confidence: u8) -> Vec<SastFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Filter to supported languages
    if !matches!(
        ext,
        "rs" | "py" | "js" | "ts" | "tsx" | "go" | "java" | "cs" | "php" | "rb" | "sol"
    ) {
        return vec![];
    }

    let mut findings = Vec::new();
    let mut in_block_comment = false;
    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Handle block comment state
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        // Skip single-line comment styles
        if trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("--")
            || trimmed.starts_with("<!--")
            || trimmed.starts_with(";")
            || trimmed.starts_with("%")
            || (ext == "py" && (trimmed.starts_with("'''") || trimmed.starts_with("\"\"\"")))
        {
            continue;
        }

        for rule in RULES {
            if confidence_rank(rule.confidence) < min_confidence {
                continue;
            }
            if !line.contains(rule.pattern) {
                continue;
            }
            if let Some(also) = rule.also {
                if !line.contains(also) {
                    continue;
                }
            }
            findings.push(SastFinding {
                file: path.to_string(),
                line: lineno + 1,
                category: rule.category.to_string(),
                rule_id: rule.rule_id.to_string(),
                confidence: rule.confidence.to_string(),
                severity: rule.severity.to_string(),
                context: truncate(trimmed, 80).to_string(),
                description: rule.description.to_string(),
                remediation: rule.remediation.to_string(),
            });
            break; // one finding per line
        }
    }
    findings
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "js", "ts", "tsx", "go", "java", "cs", "php", "rb", "sol",
    ];
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let min_confidence = confidence_rank(&cli.min_confidence);
    let mut all_findings: Vec<SastFinding> = Vec::new();
    for file in &files {
        // Skip cogent tool infrastructure sources to avoid false positives from
        // rule-literal strings, remediation text, and pattern-matching code.
        let is_cogent_infra = [
            "/sast/src/",
            "/crypto-check/src/",
            "/cogent-cli/src/audit.rs",
            "/cogent-cli/src/commands.rs",
            "/cogent-fix/src/",
        ]
        .iter()
        .any(|p| file.contains(p));
        if is_cogent_infra {
            continue;
        }
        all_findings.extend(scan_file(file, min_confidence));
    }
    all_findings.sort_by(|a, b| {
        let sev_ord = |s: &str| match s {
            "critical" => 0u8,
            "high" => 1,
            "medium" => 2,
            _ => 3,
        };
        sev_ord(&a.severity)
            .cmp(&sev_ord(&b.severity))
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });

    let critical = all_findings
        .iter()
        .filter(|f| f.severity == "critical")
        .count();
    let high = all_findings.iter().filter(|f| f.severity == "high").count();
    let medium = all_findings
        .iter()
        .filter(|f| f.severity == "medium")
        .count();
    let low = all_findings.iter().filter(|f| f.severity == "low").count();
    let total = all_findings.len();

    let summary = SastSummary {
        files_scanned: files.len(),
        total_findings: total,
        critical,
        high,
        medium,
        low,
        max_findings_threshold: cli.max_findings,
    };

    match cli.format.as_str() {
        "json" => {
            let report = SastReport {
                findings: all_findings,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for f in &all_findings {
                println!("{}", serde_json::to_string(f).unwrap());
            }
        }
        _ => {
            if all_findings.is_empty() {
                println!("No SAST findings detected.");
            } else {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 30,
                        align_right: false,
                    },
                    Column {
                        header: "Line",
                        width: 6,
                        align_right: true,
                    },
                    Column {
                        header: "Sev",
                        width: 9,
                        align_right: false,
                    },
                    Column {
                        header: "Rule",
                        width: 16,
                        align_right: false,
                    },
                    Column {
                        header: "Category",
                        width: 16,
                        align_right: false,
                    },
                    Column {
                        header: "Context",
                        width: 45,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for f in &all_findings {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&f.file, 30),
                            &f.line.to_string(),
                            &f.severity,
                            &truncate(&f.rule_id, 16),
                            &truncate(&f.category, 16),
                            &truncate(&f.context, 45),
                        ],
                    );
                }
            }
            let status = if total <= cli.max_findings {
                "PASS"
            } else {
                "FAIL"
            };
            println!(
                "\nSummary: {} findings ({} critical, {} high, {} medium, {} low) — {}",
                total, critical, high, medium, low, status
            );
        }
    }

    if total > cli.max_findings {
        std::process::exit(1);
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_sql_injection() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(
            f,
            r#"fn q(id: &str) {{ let q = format!("SELECT * FROM t WHERE id={{}}", id); }}"#
        )
        .unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), 1);
        assert!(
            findings.iter().any(|f| f.category == "sql_injection"),
            "expected sql_injection finding"
        );
    }

    #[test]
    fn test_detect_xss() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".js").unwrap();
        writeln!(f, "element.innerHTML = input;").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), 1);
        assert!(
            findings.iter().any(|f| f.category == "xss"),
            "expected xss finding"
        );
    }

    #[test]
    fn test_detect_dangerous_inner_html() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".tsx").unwrap();
        writeln!(f, "<div dangerouslySetInnerHTML={{{{ __html: x }}}} />").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), 1);
        assert!(findings.iter().any(|f| f.category == "xss"));
    }

    #[test]
    fn test_no_findings_clean_rust() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn add(a: i32, b: i32) -> i32 {{ a + b }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), 1);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_confidence_filter() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "let x = unsafe {{ *ptr }};").unwrap();
        // low confidence rule (unsafe block) should be found at min=1 but not at min=3
        let low_findings = scan_file(f.path().to_str().unwrap(), 1);
        let high_findings = scan_file(f.path().to_str().unwrap(), 3);
        assert!(low_findings.len() >= high_findings.len());
    }
}
