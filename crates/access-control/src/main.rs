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
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    exclude: Vec<String>,
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

// ── Auth patterns for cross-file middleware detection ─────────────────────

/// Patterns that indicate auth middleware is being registered or configured.
/// When found ANYWHERE in the project, individual route-handler findings
/// are suppressed (auth is assumed to be app-wide).
const AUTH_MIDDLEWARE_PATTERNS: &[&str] = &[
    // Rust: Axum/Actix middleware layers
    ".layer(Auth",
    ".layer(auth",
    ".layer(require_auth",
    ".layer(RequireAuth",
    ".layer(Jwt",
    ".layer(jwt",
    ".layer(Token",
    ".layer(token",
    ".with_state(Auth",
    ".with_state(auth",
    "AuthLayer",
    "AuthMiddleware",
    "AuthenticationLayer",
    "AuthorizationLayer",
    "RequireAuthLayer",
    "JwtAuthLayer",
    "TokenAuthLayer",
    "RouteLayer::new(Auth",
    "route_layer(Auth",
    "route_layer(auth",
    "route_layer(RequireAuth",
    // Python: Flask decorators and app config
    "@login_required",
    "@authenticate",
    "@permission_required",
    "@auth.",
    "login_required",
    "flask_login",
    "flask_security",
    "flask_principal",
    "flask_httpauth",
    "flask_jwt",
    "flask_jwt_extended",
    // Python: Django
    "LoginRequiredMixin",
    "PermissionRequiredMixin",
    "UserPassesTestMixin",
    "login_required",
    "permission_required",
    "django.contrib.auth",
    "rest_framework.permissions",
    "IsAuthenticated",
    "IsAdminUser",
    "TokenAuthentication",
    // JS/TS: Express/Koa middleware
    "passport.authenticate",
    "passport.session",
    "express-jwt",
    "express-jwt-auth",
    "express-openid-connect",
    "authMiddleware",
    "auth_middleware",
    "authenticateToken",
    "verifyToken",
    "requireAuth",
    "isAuthenticated",
    "checkAuth",
    // NestJS
    "@AuthGuard",
    "@RolesGuard",
    "@ThrottlerGuard",
    "@UseGuards(Auth",
    "@UseGuards(Jwt",
    // Go
    "router.Use(auth",
    "router.Use(Auth",
    "r.Use(auth",
    "r.Use(Auth",
    "gin.BasicAuth",
    "gin.Auth",
    "middleware.BasicAuth",
    "middleware.JWT",
    // Java/Spring
    "@EnableWebSecurity",
    "@EnableMethodSecurity",
    "SecurityFilterChain",
    ".authenticated()",
    ".permitAll()",
    ".hasRole(",
    ".hasAuthority(",
    "@PreAuthorize",
    "@Secured",
    "@RolesAllowed",
    // C# / ASP.NET
    "[Authorize]",
    "AddAuthentication(",
    "AddJwtBearer(",
    "UseAuthentication(",
    "UseAuthorization(",
];

/// Keywords in handler names or inline middleware args that indicate
/// auth protection is applied at the route level.
#[expect(dead_code)]
const INLINE_AUTH_KEYWORDS: &[&str] = &[
    "auth",
    "Auth",
    "jwt",
    "JWT",
    "Jwt",
    "token",
    "Token",
    "login",
    "Login",
    "secure",
    "Secure",
    "authenticate",
    "Authenticate",
    "authorize",
    "Authorize",
];

/// Route patterns that are explicitly public (health checks, status, etc.)
/// and should NOT be flagged even without auth middleware.
const PUBLIC_ROUTE_KEYWORDS: &[&str] = &[
    "health",
    "Health",
    "ping",
    "Ping",
    "status",
    "Status",
    "metrics",
    "Metrics",
    "favicon",
    "robots.txt",
    "openapi",
    "OpenAPI",
    "swagger",
    "Swagger",
    "docs",
    "Docs",
    ".well-known",
];

// ── Existing rules ─────────────────────────────────────────────────────────

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
    // ── hardcoded_creds ────────────────────────────────────────────────
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
    // ── iam_policy ──────────────────────────────────────────────────
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
    // ── cors ───────────────────────────────────────────────────────
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
    // ── dangerous_shell ────────────────────────────────────────────
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

// ── Cross-file middleware context ─────────────────────────────────────────

#[derive(Default, Debug)]
struct ProjectContext {
    /// True if any auth middleware registration was found across all files.
    has_app_wide_auth: bool,
    /// Specific middleware names found (for debugging/reporting).
    middleware_names: Vec<String>,
    /// Route path patterns that are explicitly public (health checks, etc.).
    public_routes: Vec<String>,
}

/// Scan a single source line for auth middleware and public-route markers.
fn scan_context_line(line: &str, ctx: &mut ProjectContext) {
    let line_lower = line.to_lowercase();

    // Check for auth middleware registration patterns
    for pattern in AUTH_MIDDLEWARE_PATTERNS {
        if line.contains(pattern) && !line.trim_start().starts_with("//") {
            ctx.has_app_wide_auth = true;
            ctx.middleware_names.push(pattern.to_string());
            break;
        }
    }

    // Check for public route markers
    for keyword in PUBLIC_ROUTE_KEYWORDS {
        if line_lower.contains(&keyword.to_lowercase()) {
            // Only collect if it looks like a route definition
            if line.contains("route")
                || line.contains("get(")
                || line.contains("post(")
                || line.contains("put(")
                || line.contains("delete(")
            {
                ctx.public_routes.push(keyword.to_string());
            }
            break;
        }
    }
}

/// First pass: scan ALL source files to collect auth middleware context.
fn collect_project_context(files: &[String]) -> ProjectContext {
    let mut ctx = ProjectContext::default();

    for file in files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in source.lines() {
            scan_context_line(line, &mut ctx);
        }
    }

    // Deduplicate middleware names
    ctx.middleware_names.sort();
    ctx.middleware_names.dedup();
    ctx.public_routes.sort();
    ctx.public_routes.dedup();

    ctx
}

/// Check if a line of code has inline auth protection on a route handler.
/// Uses two strategies:
/// 1. Counts comma-separated arguments in route method calls (3+ args = middleware inline)
/// 2. Detects NestJS/Flask decorators and Axum per-route middleware
fn has_inline_auth(line: &str) -> bool {
    let lower = line.to_lowercase();

    // Check for NestJS guard decorators
    if line.contains("@UseGuards(") {
        return true;
    }

    // Check for .route_layer() patterns (Rust Axum per-route auth)
    if line.contains("route_layer(") && lower.contains("auth") {
        return true;
    }

    // Strategy: detect inline middleware by counting arguments in route calls.
    // Patterns like router.get('/path', authMiddleware, handler) have 3+ args.
    let route_methods = [
        "get(", "post(", "put(", "delete(", "patch(", "head(", "options(",
    ];
    for method in &route_methods {
        if let Some(pos) = lower.find(method) {
            // Find the argument list after the method name
            let after_call = &lower[pos + method.len()..];
            let mut depth = 0i32;
            let mut top_level_commas = 0i32;
            let mut in_string = false;
            let mut string_char = '\0';

            for c in after_call.chars() {
                if in_string {
                    if c == string_char {
                        in_string = false;
                    }
                    continue;
                }
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth < 0 {
                            break;
                        }
                    }
                    '"' | '\'' => {
                        in_string = true;
                        string_char = c;
                    }
                    ',' if depth == 0 => top_level_commas += 1,
                    _ => {}
                }
            }

            // 2+ commas = 3+ args = inline middleware present
            if top_level_commas >= 2 {
                return true;
            }
        }
    }

    false
}

/// Check if a line looks like a public/health endpoint that should be exempt.
fn is_exempt_route(line: &str) -> bool {
    let lower = line.to_lowercase();
    PUBLIC_ROUTE_KEYWORDS
        .iter()
        .any(|kw| lower.contains(&kw.to_lowercase()))
}

/// True if this is a `missing_auth` rule (suppressible via middleware context).
fn is_missing_auth_rule(rule: &Rule) -> bool {
    rule.category == "missing_auth"
}

fn scan_file(path: &str, ctx: &ProjectContext) -> Vec<AccessFinding> {
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
            || (trimmed.starts_with('#')
                && !trimmed.starts_with("#[")
                && !trimmed.starts_with("#!")
                && !trimmed.starts_with("##"))
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
            // Skip rule definition lines to avoid self-detection
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

            // ── Cross-file auth analysis ──────────────────────────────────
            // If this is a missing_auth rule, check whether the project has
            // app-wide auth middleware that would protect this route.
            if is_missing_auth_rule(rule) {
                // If the project has app-wide auth middleware AND this route
                // isn't explicitly unprotected, suppress the finding.
                if ctx.has_app_wide_auth && !is_exempt_route(line) {
                    continue;
                }
                // If this specific route handler has inline auth, suppress.
                if has_inline_auth(line) {
                    continue;
                }
                // If this is a clearly public/exempt route, suppress.
                if is_exempt_route(line) {
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

    // Apply exclude patterns
    let files: Vec<String> = files
        .into_iter()
        .filter(|f| cli.exclude.is_empty() || !cli.exclude.iter().any(|ex| f.contains(ex)))
        .collect();

    // First pass: collect auth middleware context across all files
    let ctx = collect_project_context(&files);

    // Second pass: scan each file with middleware context
    let mut all_findings: Vec<AccessFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file, &ctx));
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

            // Print middleware context summary
            if ctx.has_app_wide_auth {
                println!(
                    "\n  Auth middleware detected: {} pattern(s) — see log for details",
                    ctx.middleware_names.len()
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Middleware detection tests ──────────────────────────────────

    #[test]
    fn test_detect_axum_auth_layer() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        f.write_all(
            b"Router::new()\n    .route(\"/api/users\", get(list_users))\n    .layer(AuthLayer::new(config))\n",
        )
        .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(ctx.has_app_wide_auth, "should detect AuthLayer");
    }

    #[test]
    fn test_detect_express_auth_middleware() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".js").unwrap();
        f.write_all(b"app.use(authMiddleware);\napp.get('/users', listUsers);\n")
            .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(ctx.has_app_wide_auth, "should detect authMiddleware");
    }

    #[test]
    fn test_no_middleware_detected() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        f.write_all(b"fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n")
            .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(
            !ctx.has_app_wide_auth,
            "should not detect auth in clean code"
        );
    }

    #[test]
    fn test_detect_flask_login_required() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".py").unwrap();
        f.write_all(
            b"from flask_login import login_required\n\n@app.route('/admin')\n@login_required\ndef admin():\n    pass\n",
        )
        .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(ctx.has_app_wide_auth, "should detect login_required");
    }

    #[test]
    fn test_detect_spring_security() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".java").unwrap();
        f.write_all(
            b"import org.springframework.security.config.annotation.web.builders.HttpSecurity;\n\n@EnableWebSecurity\npublic class SecurityConfig {\n    @Bean\n    public SecurityFilterChain filterChain(HttpSecurity http) {\n        return http.authenticated().build();\n    }\n}\n",
        )
        .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(ctx.has_app_wide_auth, "should detect Spring Security");
    }

    #[test]
    fn test_detect_go_gin_auth() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".go").unwrap();
        f.write_all(
            b"r := gin.Default()\nr.Use(gin.BasicAuth(accounts))\nr.GET(\"/admin\", adminHandler)\n",
        )
        .unwrap();
        let files = vec![f.path().to_str().unwrap().to_string()];
        let ctx = collect_project_context(&files);
        assert!(ctx.has_app_wide_auth, "should detect gin.BasicAuth");
    }

    // ── Inline auth detection tests ─────────────────────────────────

    #[test]
    fn test_detect_inline_auth_middleware() {
        assert!(
            has_inline_auth("router.get('/admin', authMiddleware, handler)"),
            "should detect inline authMiddleware"
        );
    }

    #[test]
    fn test_detect_inline_auth_no_match() {
        assert!(
            !has_inline_auth("router.get('/users', listUsers)"),
            "should not flag route without inline auth"
        );
    }

    #[test]
    fn test_detect_nestjs_guard() {
        assert!(
            has_inline_auth("@UseGuards(AuthGuard)"),
            "should detect NestJS guard"
        );
    }

    // ── Exempt route detection ──────────────────────────────────────

    #[test]
    fn test_detect_health_endpoint() {
        assert!(
            is_exempt_route("app.get('/health', healthHandler)"),
            "health should be exempt"
        );
        assert!(
            is_exempt_route("router.get('/ping', pingHandler)"),
            "ping should be exempt"
        );
    }

    #[test]
    fn test_detect_non_exempt_route() {
        assert!(
            !is_exempt_route("app.get('/admin', adminHandler)"),
            "admin should not be exempt"
        );
    }

    // ── Scan suppression tests ──────────────────────────────────────

    #[test]
    fn test_suppress_auth_finding_when_middleware_exists() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "use axum::prelude::*;").unwrap();
        writeln!(f, "#[get(\"/users\")]").unwrap();
        writeln!(f, "fn list_users() -> Json<Vec<User>> {{").unwrap();
        writeln!(f, "    Json(vec![])").unwrap();
        writeln!(f, "}}").unwrap();

        let ctx = ProjectContext {
            has_app_wide_auth: true,
            middleware_names: vec!["AuthLayer".to_string()],
            public_routes: vec![],
        };
        let findings = scan_file(f.path().to_str().unwrap(), &ctx);
        // Should NOT find missing_auth since AuthLayer exists project-wide
        assert!(
            !findings.iter().any(|f| f.category == "missing_auth"),
            "should suppress auth finding when middleware is registered"
        );
    }

    #[test]
    fn test_report_auth_finding_when_no_middleware() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "#[get(\"/users\")]").unwrap();
        writeln!(f, "fn list_users() -> Json<Vec<User>> {{").unwrap();
        writeln!(f, "    Json(vec![])").unwrap();
        writeln!(f, "}}").unwrap();

        let ctx = ProjectContext::default(); // no middleware
        let findings = scan_file(f.path().to_str().unwrap(), &ctx);
        assert!(
            findings.iter().any(|f| f.category == "missing_auth"),
            "should report auth finding when no middleware is registered"
        );
    }

    #[test]
    fn test_health_route_not_flagged() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "#[get(\"/health\")]").unwrap();
        writeln!(f, "fn health() -> Json<HealthStatus> {{").unwrap();
        writeln!(f, "    Json(HealthStatus::ok())").unwrap();
        writeln!(f, "}}").unwrap();

        let ctx = ProjectContext::default();
        let findings = scan_file(f.path().to_str().unwrap(), &ctx);
        assert!(
            !findings.iter().any(|f| f.category == "missing_auth"),
            "should not flag health endpoint as missing auth"
        );
    }

    // ── Non-auth rules still fire ──
    #[test]
    fn test_hardcoded_creds_still_detected() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        // Split to avoid self-detection by the tool.
        let cred_line = format!("let {}supersecret123\";", "password = \"");
        writeln!(f, "{}", cred_line).unwrap();

        let ctx = ProjectContext::default();
        let findings = scan_file(f.path().to_str().unwrap(), &ctx);
        assert!(
            findings.iter().any(|f| f.category == "hardcoded_creds"),
            "hardcoded creds should still be detected regardless of auth middleware"
        );
    }
    #[test]
    fn test_cors_still_detected() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        // Split the CORS header across two parts to avoid self-detection by the tool.
        let cors_header = format!("Access-Control-Allow-{}", "Origin: *");
        writeln!(f, "let headers = vec![\"{}\"];", cors_header).unwrap();

        let ctx = ProjectContext::default();
        let findings = scan_file(f.path().to_str().unwrap(), &ctx);
        assert!(
            findings.iter().any(|f| f.category == "cors"),
            "CORS issues should still be detected regardless of auth middleware"
        );
    }
}
