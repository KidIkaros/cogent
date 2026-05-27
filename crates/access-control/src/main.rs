#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "access-control",
    about = "Access control checker — detect missing auth guards, hardcoded credentials, overly permissive IAM policies, and dangerous CORS settings"
)]
struct Cli {
    path: String,
    #[arg(short, long)]
    recursive: bool,
    #[arg(short, long, default_value = "table")]
    format: String,
    #[arg(long, default_value = "0")]
    max_violations: usize,
}

#[derive(Debug, Clone, Serialize)]
struct AccessFinding {
    file: String,
    line: usize,
    category: String,
    rule_id: String,
    severity: String,
    context: String,
    description: String,
    remediation: String,
}

#[derive(Serialize)]
struct AccessReport {
    findings: Vec<AccessFinding>,
    summary: AccessSummary,
}

#[derive(Serialize)]
struct AccessSummary {
    files_scanned: usize,
    total_findings: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    max_violations_threshold: usize,
}

struct Rule {
    category: &'static str,
    rule_id: &'static str,
    severity: &'static str,
    pattern: &'static str,
    also: Option<&'static str>,
    description: &'static str,
    remediation: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-001",
        severity: "high",
        pattern: "#[get(",
        also: None,
        description: "Rust HTTP route handler may be missing an auth guard.",
        remediation:
            "Add an authentication/authorization middleware or attribute to the route handler.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-002",
        severity: "high",
        pattern: "app.route(",
        also: None,
        description: "Axum/Actix route registration without visible auth middleware.",
        remediation: "Wrap the route with authentication middleware or use a protected router.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-003",
        severity: "high",
        pattern: "@app.route(",
        also: None,
        description: "Flask route without login_required or auth decorator.",
        remediation: "Add @login_required or a custom auth decorator to sensitive routes.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-004",
        severity: "high",
        pattern: "router.get(",
        also: None,
        description: "Express router endpoint may lack authentication middleware.",
        remediation: "Add passport.authenticate or auth middleware before the route.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-005",
        severity: "high",
        pattern: "app.get(",
        also: None,
        description: "Express app endpoint may lack authentication middleware.",
        remediation: "Apply authentication middleware to sensitive endpoints.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-006",
        severity: "high",
        pattern: "r.GET(",
        also: None,
        description: "Go Gin/Echo route without auth middleware.",
        remediation:
            "Use router.Use(authMiddleware()) or group routes under an auth-protected group.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-007",
        severity: "high",
        pattern: "@RequestMapping",
        also: None,
        description: "Spring endpoint without visible method-level security annotation.",
        remediation: "Add @PreAuthorize or @Secured annotation to the endpoint method.",
    },
    Rule {
        category: "missing_auth",
        rule_id: "ACL-AUTH-008",
        severity: "high",
        pattern: "@Path(",
        also: None,
        description: "JAX-RS endpoint without security annotation.",
        remediation: "Add @RolesAllowed or a security filter for the endpoint.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-001",
        severity: "critical",
        pattern: "password = \"",
        also: None,
        description: "Hardcoded password detected in source or config.",
        remediation: "Move credentials to environment variables or a secrets manager.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-002",
        severity: "critical",
        pattern: "passwd = \"",
        also: None,
        description: "Hardcoded password detected.",
        remediation: "Use environment variables or a secrets manager.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-003",
        severity: "critical",
        pattern: "secret = \"",
        also: None,
        description: "Hardcoded secret detected.",
        remediation: "Store secrets in a dedicated secrets manager, never in source code.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-004",
        severity: "critical",
        pattern: "api_key = \"",
        also: None,
        description: "Hardcoded API key detected.",
        remediation: "Load API keys from environment variables or a secure vault at runtime.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-005",
        severity: "critical",
        pattern: "token = \"",
        also: None,
        description: "Hardcoded token detected.",
        remediation: "Store tokens in environment variables or a secrets manager.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-006",
        severity: "critical",
        pattern: "admin:admin",
        also: None,
        description: "Default admin credentials detected.",
        remediation:
            "Remove default credentials. Enforce strong password policies and secrets management.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-007",
        severity: "critical",
        pattern: "root:password",
        also: None,
        description: "Default root password detected.",
        remediation: "Remove default credentials immediately. Use a secrets manager.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-008",
        severity: "high",
        pattern: "password = \"password\"",
        also: None,
        description: "Literal 'password' used as a password value.",
        remediation: "Never use literal strings as passwords. Use environment variables.",
    },
    Rule {
        category: "hardcoded_creds",
        rule_id: "ACL-CRED-009",
        severity: "high",
        pattern: "secret = \"secret\"",
        also: None,
        description: "Literal 'secret' used as a secret value.",
        remediation: "Never hardcode secrets. Use a secrets manager or environment variables.",
    },
    Rule {
        category: "iam_policy",
        rule_id: "ACL-IAM-001",
        severity: "critical",
        pattern: "\"Effect\": \"Allow\"",
        also: Some("\"Resource\": \"*\""),
        description: "Overly permissive IAM policy: Allow + Resource:* detected.",
        remediation:
            "Scope the Resource to specific ARNs or resources. Avoid wildcard permissions.",
    },
    Rule {
        category: "iam_policy",
        rule_id: "ACL-IAM-002",
        severity: "critical",
        pattern: "Effect: Allow",
        also: Some("Resource: *"),
        description: "Overly permissive IAM policy in YAML format.",
        remediation: "Restrict Resource to specific resources. Use least-privilege principle.",
    },
    Rule {
        category: "iam_policy",
        rule_id: "ACL-IAM-003",
        severity: "high",
        pattern: "\"Action\": \"*\"",
        also: None,
        description: "IAM policy allows all actions (Action:*).",
        remediation: "Restrict Action to only the specific API operations required.",
    },
    Rule {
        category: "iam_policy",
        rule_id: "ACL-IAM-004",
        severity: "high",
        pattern: "Action: *",
        also: None,
        description: "IAM policy allows all actions in YAML format.",
        remediation: "List only required actions explicitly.",
    },
    Rule {
        category: "cors",
        rule_id: "ACL-CORS-001",
        severity: "high",
        pattern: "Access-Control-Allow-Origin: *",
        also: None,
        description: "CORS allows all origins — potential security risk.",
        remediation: "Restrict Access-Control-Allow-Origin to specific trusted domains.",
    },
    Rule {
        category: "cors",
        rule_id: "ACL-CORS-002",
        severity: "high",
        pattern: "cors(allow_all=True)",
        also: None,
        description: "CORS configured to allow all origins.",
        remediation: "Set allow_all=False and specify an explicit allowlist of origins.",
    },
    Rule {
        category: "cors",
        rule_id: "ACL-CORS-003",
        severity: "medium",
        pattern: "@cross_origin(",
        also: None,
        description: "Flask-CORS decorator without origin restrictions.",
        remediation: "Specify origins= parameter to restrict cross-origin access.",
    },
    Rule {
        category: "cors",
        rule_id: "ACL-CORS-004",
        severity: "medium",
        pattern: "CORS(app",
        also: None,
        description: "Flask-CORS applied to entire app without origin restrictions.",
        remediation: "Configure CORS with a specific origins list, not globally open.",
    },
    Rule {
        category: "dangerous_shell",
        rule_id: "ACL-SUDO-001",
        severity: "high",
        pattern: "ALL=(ALL) NOPASSWD: ALL",
        also: None,
        description: "Sudoers file allows any user to run any command without a password.",
        remediation:
            "Restrict sudo privileges to specific users, commands, and require a password.",
    },
    Rule {
        category: "dangerous_shell",
        rule_id: "ACL-SUDO-002",
        severity: "medium",
        pattern: "sudo su",
        also: None,
        description: "Direct root escalation via sudo su without restrictions.",
        remediation: "Use sudo with specific commands only. Avoid blanket root access.",
    },
];

fn scan_file(path: &str) -> Vec<AccessFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut findings = Vec::new();
    let mut in_block_comment = false;

    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();
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
            if !line.contains(rule.pattern) {
                continue;
            }
            // Skip rule definition lines to avoid self-detection in the RULES array
            if trimmed.starts_with("pattern:")
                || trimmed.starts_with("description:")
                || trimmed.starts_with("remediation:")
                || trimmed.starts_with("rule_id:")
                || trimmed.starts_with("severity:")
                || trimmed.starts_with("also:")
            {
                continue;
            }
            if let Some(also) = rule.also {
                if !line.contains(also) {
                    continue;
                }
            }
            findings.push(AccessFinding {
                file: path.to_string(),
                line: lineno + 1,
                category: rule.category.to_string(),
                rule_id: rule.rule_id.to_string(),
                severity: rule.severity.to_string(),
                context: truncate(trimmed, 80).to_string(),
                description: rule.description.to_string(),
                remediation: rule.remediation.to_string(),
            });
            break;
        }
    }
    findings
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "js", "ts", "tsx", "go", "java", "cs", "php", "rb", "sol", "sh", "yaml", "yml",
        "json", "toml",
    ];
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };
    let mut all_findings: Vec<AccessFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file));
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
    let summary = AccessSummary {
        files_scanned: files.len(),
        total_findings: all_findings.len(),
        critical,
        high,
        medium,
        low,
        max_violations_threshold: cli.max_violations,
    };
    let exceeds_threshold = summary.total_findings > cli.max_violations;
    match cli.format.as_str() {
        "json" => {
            let report = AccessReport {
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
            let cols = &[
                Column::left("Rule", 14),
                Column::left("Severity", 10),
                Column::left("File", 30),
                Column::left("Line", 6),
                Column::left("Context", 40),
            ];
            print_table_header(cols);
            for f in &all_findings {
                print_table_row(
                    cols,
                    &[
                        &f.rule_id,
                        &f.severity,
                        &truncate(&f.file, 30),
                        &f.line.to_string(),
                        &truncate(&f.context, 40),
                    ],
                );
            }
            println!(
                "\n  Total: {} findings ({} critical, {} high, {} medium, {} low) in {} files",
                summary.total_findings,
                critical,
                high,
                medium,
                low,
                files.len()
            );
            if exceeds_threshold {
                println!("  Exceeds threshold of {} violations", cli.max_violations);
            }
        }
    }
    if exceeds_threshold {
        std::process::exit(1);
    }
}

fn main() {
    let cli = Cli::parse();
    run(cli);
}
