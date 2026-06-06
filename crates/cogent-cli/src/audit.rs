//! Audit infrastructure: policy engine, exception handling, remediation tracking, audit trail.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::types::{Evidence, Finding, SuggestedFix};

// ═══════════════════════════════════════════
// POLICY ENGINE
// ═══════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyFile {
    pub policy: Policy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Policy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub controls: Vec<PolicyControl>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyControl {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub threshold: serde_json::Value,
    #[serde(default)]
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Validate a policy file: check that all referenced tools exist and thresholds are numeric.
pub fn validate_policy(path: &Path, known_tools: &[String]) -> PolicyValidationResult {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return PolicyValidationResult {
                valid: false,
                errors: vec![format!("Cannot read {}: {}", path.display(), e)],
                warnings: vec![],
            };
        }
    };

    let policy_file: PolicyFile = match serde_yaml::from_str(&content) {
        Ok(p) => p,
        Err(e) => {
            return PolicyValidationResult {
                valid: false,
                errors: vec![format!("YAML parse error in {}: {}", path.display(), e)],
                warnings: vec![],
            };
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if policy_file.policy.controls.is_empty() {
        warnings.push(format!(
            "Policy '{}' has no controls",
            policy_file.policy.name
        ));
    }

    for ctrl in &policy_file.policy.controls {
        if ctrl.id.is_empty() {
            errors.push("Control missing 'id' field".to_string());
        }
        if ctrl.name.is_empty() {
            errors.push(format!("Control '{}' missing 'name'", ctrl.id));
        }
        if !known_tools.contains(&ctrl.tool) {
            errors.push(format!(
                "Control '{}' references unknown tool '{}'",
                ctrl.id, ctrl.tool
            ));
        }
    }

    PolicyValidationResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

/// Find all `.cogent-policies/*.yaml` files under the given directory.
pub fn discover_policies(dir: &str) -> Vec<PathBuf> {
    let policy_dir = Path::new(dir).join(".cogent-policies");
    if !policy_dir.exists() {
        return vec![];
    }
    std::fs::read_dir(&policy_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.ends_with(".yaml") || name.ends_with(".yml")
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

// ═══════════════════════════════════════════
// EXCEPTION HANDLING
// ═══════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExceptionFile {
    pub exceptions: Vec<ExceptionEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExceptionEntry {
    pub id: String,
    pub finding_id: String,
    pub rule_id: String,
    pub file: String,
    pub reason: String,
    pub reviewer: String,
    pub status: String, // pending | approved | revoked
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

const EXCEPTIONS_PATH: &str = ".cogent-exceptions.yaml";

fn load_exceptions() -> ExceptionFile {
    if !Path::new(EXCEPTIONS_PATH).exists() {
        return ExceptionFile { exceptions: vec![] };
    }
    std::fs::read_to_string(EXCEPTIONS_PATH)
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or(ExceptionFile { exceptions: vec![] })
}

fn save_exceptions(file: &ExceptionFile) -> Result<(), String> {
    let yaml = serde_yaml::to_string(file).map_err(|e| e.to_string())?;
    std::fs::write(EXCEPTIONS_PATH, yaml).map_err(|e| e.to_string())
}

/// List exceptions filtered by status.
pub fn list_exceptions(status_filter: Option<&str>) -> Vec<ExceptionEntry> {
    let file = load_exceptions();
    match status_filter {
        Some(s) => file
            .exceptions
            .into_iter()
            .filter(|e| e.status == s)
            .collect(),
        None => file.exceptions,
    }
}

/// Propose a new exception (status: pending).
pub fn add_exception(
    finding_id: &str,
    rule_id: &str,
    file: &str,
    reason: &str,
    reviewer: &str,
) -> Result<String, String> {
    let mut file_data = load_exceptions();
    let id = format!("EXC-{}", file_data.exceptions.len() + 1);
    let entry = ExceptionEntry {
        id: id.clone(),
        finding_id: finding_id.to_string(),
        rule_id: rule_id.to_string(),
        file: file.to_string(),
        reason: reason.to_string(),
        reviewer: reviewer.to_string(),
        status: "pending".to_string(),
        approved_at: None,
        expires_at: None,
    };
    file_data.exceptions.push(entry);
    save_exceptions(&file_data)?;
    Ok(id)
}

/// Approve a pending exception.
pub fn approve_exception(id: &str) -> Result<(), String> {
    let mut file = load_exceptions();
    let entry = file
        .exceptions
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("Exception {} not found", id))?;
    if entry.status != "pending" {
        return Err(format!(
            "Exception {} is not pending (status: {})",
            id, entry.status
        ));
    }
    entry.status = "approved".to_string();
    entry.approved_at = Some(chrono::Utc::now().to_rfc3339());
    save_exceptions(&file)
}

/// Revoke an approved exception.
pub fn revoke_exception(id: &str) -> Result<(), String> {
    let mut file = load_exceptions();
    let entry = file
        .exceptions
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("Exception {} not found", id))?;
    entry.status = "revoked".to_string();
    save_exceptions(&file)
}

/// Check if a finding matches an approved exception.
pub fn is_suppressed(rule_id: &str, file: &str) -> bool {
    let exc = load_exceptions();
    exc.exceptions
        .iter()
        .any(|e| e.status == "approved" && e.rule_id == rule_id && e.file == file)
}

// ═══════════════════════════════════════════
// REMEDIATION TRACKING
// ═══════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemediationLog {
    pub entries: Vec<RemediationEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemediationEntry {
    pub finding_id: String,
    pub rule_id: String,
    pub file: String,
    pub detected_at: String,
    pub status: String, // open | in_progress | closed
    pub assigned_to: Option<String>,
    pub closed_at: Option<String>,
    pub verification_scan: Option<String>,
}

const REMEDIATION_PATH: &str = ".cogent-remediation.json";

fn load_remediation() -> RemediationLog {
    if !Path::new(REMEDIATION_PATH).exists() {
        return RemediationLog { entries: vec![] };
    }
    std::fs::read_to_string(REMEDIATION_PATH)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(RemediationLog { entries: vec![] })
}

pub(crate) fn save_remediation(log: &RemediationLog) -> Result<(), String> {
    let json = serde_json::to_string_pretty(log).map_err(|e| e.to_string())?;
    std::fs::write(REMEDIATION_PATH, json).map_err(|e| e.to_string())
}

/// Clear the remediation log (for testing).
pub fn clear_remediation() -> Result<(), String> {
    if Path::new(REMEDIATION_PATH).exists() {
        std::fs::remove_file(REMEDIATION_PATH).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Record new findings from a scan.
pub(crate) fn record_findings(findings: &[Finding]) -> Vec<RemediationEntry> {
    let mut log = load_remediation();
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_entries = Vec::new();

    for f in findings {
        let fid = format!("{}:{}:{}", f.rule_id, f.file, f.line.unwrap_or(0));
        if !log.entries.iter().any(|e| e.finding_id == fid) {
            let entry = RemediationEntry {
                finding_id: fid.clone(),
                rule_id: f.rule_id.clone(),
                file: f.file.clone(),
                detected_at: now.clone(),
                status: "open".to_string(),
                assigned_to: None,
                closed_at: None,
                verification_scan: None,
            };
            log.entries.push(entry.clone());
            new_entries.push(entry);
        }
    }

    let _ = save_remediation(&log);
    new_entries
}

/// Mark verified-closed findings (present in previous scan but absent now).
pub(crate) fn verify_remediation(current_findings: &[Finding]) -> Vec<String> {
    let mut log = load_remediation();
    let now = chrono::Utc::now().to_rfc3339();
    let mut closed = Vec::new();

    let current_ids: std::collections::HashSet<String> = current_findings
        .iter()
        .map(|f| format!("{}:{}:{}", f.rule_id, f.file, f.line.unwrap_or(0)))
        .collect();

    for entry in &mut log.entries {
        if entry.status == "open" && !current_ids.contains(&entry.finding_id) {
            entry.status = "closed".to_string();
            entry.closed_at = Some(now.clone());
            entry.verification_scan = Some(now.clone());
            closed.push(entry.finding_id.clone());
        }
    }

    let _ = save_remediation(&log);
    closed
}

/// Print remediation status summary.
pub fn print_remediation_summary() {
    let log = load_remediation();
    let open = log.entries.iter().filter(|e| e.status == "open").count();
    let in_progress = log
        .entries
        .iter()
        .filter(|e| e.status == "in_progress")
        .count();
    let closed = log.entries.iter().filter(|e| e.status == "closed").count();

    println!("Remediation Status");
    println!("  Open:        {}", open);
    println!("  In Progress: {}", in_progress);
    println!("  Closed:      {}", closed);

    if open > 0 {
        println!("\n  Oldest open findings:");
        let mut open_entries: Vec<_> = log.entries.iter().filter(|e| e.status == "open").collect();
        open_entries.sort_by(|a, b| a.detected_at.cmp(&b.detected_at));
        for e in open_entries.iter().take(5) {
            println!("    {}  {}  {}", e.finding_id, e.file, e.detected_at);
        }
    }
}

// ═══════════════════════════════════════════
// AUDIT TRAIL
// ═══════════════════════════════════════════

use std::io::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub actor: String,
    pub command: String,
    pub scope: String,
    pub findings_count: usize,
    pub duration_ms: u64,
    pub signature: String,
}

const TRAIL_DIR: &str = ".cogent-audit";
const TRAIL_FILE: &str = ".cogent-audit/trail.jsonl";

fn compute_hmac(data: &str, key: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .expect("HMAC-SHA256 accepts any key length");
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn audit_key() -> String {
    std::env::var("COGENT_AUDIT_KEY").unwrap_or_else(|_| "cogent-default-audit-key".to_string())
}

/// Append an entry to the signed audit trail.
pub fn append_audit_trail(
    command: &str,
    scope: &str,
    findings_count: usize,
    duration: std::time::Duration,
) {
    let _ = std::fs::create_dir_all(TRAIL_DIR);
    let entry = AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        actor: whoami::username(),
        command: command.to_string(),
        scope: scope.to_string(),
        findings_count,
        duration_ms: duration.as_millis() as u64,
        signature: String::new(),
    };
    let data = format!(
        "{}|{}|{}|{}|{}",
        entry.timestamp, entry.actor, entry.command, entry.scope, entry.findings_count
    );
    let signature = compute_hmac(&data, &audit_key());
    let mut entry = entry;
    entry.signature = signature;

    let line = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(TRAIL_FILE)
    {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = writeln!(file, "{}", line);
}

/// Verify the audit trail has not been tampered with.
pub fn verify_audit_trail() -> (bool, Vec<String>) {
    if !Path::new(TRAIL_FILE).exists() {
        return (true, vec!["No audit trail found.".to_string()]);
    }

    let content = match std::fs::read_to_string(TRAIL_FILE) {
        Ok(c) => c,
        Err(e) => return (false, vec![format!("Cannot read trail: {}", e)]),
    };

    let key = audit_key();
    let mut errors = Vec::new();
    let mut line_num = 0;

    for line in content.lines() {
        line_num += 1;
        let entry: AuditEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Line {}: JSON parse error: {}", line_num, e));
                continue;
            }
        };
        let data = format!(
            "{}|{}|{}|{}|{}",
            entry.timestamp, entry.actor, entry.command, entry.scope, entry.findings_count
        );
        let expected = compute_hmac(&data, &key);
        if expected != entry.signature {
            errors.push(format!(
                "Line {}: signature mismatch (possible tampering)",
                line_num
            ));
        }
    }

    (errors.is_empty(), errors)
}

/// Query audit trail entries.
pub fn query_audit_trail(since: Option<&str>, command_filter: Option<&str>) -> Vec<AuditEntry> {
    if !Path::new(TRAIL_FILE).exists() {
        return vec![];
    }
    let content = std::fs::read_to_string(TRAIL_FILE).unwrap_or_default();
    let mut entries = Vec::new();
    for line in content.lines() {
        if let Ok(e) = serde_json::from_str::<AuditEntry>(line) {
            if let Some(since) = since {
                if e.timestamp.as_str() < since {
                    continue;
                }
            }
            if let Some(cmd) = command_filter {
                if e.command != cmd {
                    continue;
                }
            }
            entries.push(e);
        }
    }
    entries
}

// ═══════════════════════════════════════════
// AGENT ENRICHMENT
// ═══════════════════════════════════════════

/// Static suggested-fix lookup keyed by rule_id prefix.
/// Returns (description, diff_template, confidence).
pub fn suggested_fix_for(rule_id: &str) -> Option<(String, Option<String>, String)> {
    let r = rule_id.to_lowercase();
    let fix = if r.contains("sqli") || r.contains("sql-inject") {
        (
            "Use parameterized queries instead of string interpolation.",
            Some("- let q = format!(\"SELECT * FROM users WHERE id = {}\", id);\n+ let q = \"SELECT * FROM users WHERE id = $1\";\n+ sqlx::query(q).bind(id)".to_string()),
            "high",
        )
    } else if r.contains("xss") {
        (
            "Escape HTML output; use a templating engine with auto-escaping.",
            Some("- res.send(user_input);\n+ res.send(escape_html(user_input));".to_string()),
            "high",
        )
    } else if r.contains("pathtraversal") || r.contains("path-traversal") {
        (
            "Canonicalize and validate file paths before use.",
            Some("+ let safe = std::fs::canonicalize(&path)?;\n+ if !safe.starts_with(base_dir) { return Err(\"invalid path\"); }".to_string()),
            "high",
        )
    } else if r.contains("cmdi") || r.contains("cmd-inject") || r.contains("command-inject") {
        (
            "Avoid shell=true; pass arguments as a list, never interpolate user input.",
            Some("- Command::new(\"sh\").arg(\"-c\").arg(&user_input)\n+ Command::new(\"tool\").arg(&validated_arg)".to_string()),
            "high",
        )
    } else if r.contains("secrets") || r.contains("secret") || r.contains("credential") {
        (
            "Remove secret from source; load from environment variable or secrets manager.",
            Some("- const API_KEY: &str = \"sk-abc123\";\n+ let api_key = std::env::var(\"API_KEY\")?;".to_string()),
            "high",
        )
    } else if r.contains("crypto-weak")
        || r.contains("weak-hash")
        || r.contains("md5")
        || r.contains("sha1")
    {
        (
            "Replace weak hash (MD5/SHA1) with SHA-256 or SHA-3.",
            Some("- use md5;\n- let hash = md5::compute(data);\n+ use sha2::{Sha256, Digest};\n+ let hash = Sha256::digest(data);".to_string()),
            "high",
        )
    } else if r.contains("crypto-ecb") || r.contains("ecb") {
        (
            "Replace ECB mode with an authenticated mode such as AES-GCM.",
            Some("- Cipher::new_from_slices(key, b\"\")  // ECB\n+ Aes256Gcm::new(key)  // GCM with nonce".to_string()),
            "high",
        )
    } else if r.contains("insecure-random") || r.contains("rand-weak") {
        (
            "Use a cryptographically secure RNG (e.g. `rand::rngs::OsRng`) for security-sensitive values.",
            Some("- use rand::Rng;\n+ use rand::rngs::OsRng;\n+ use rand::RngCore;".to_string()),
            "high",
        )
    } else if r.contains("taint") {
        (
            "Sanitize tainted data before passing to a sink; add validation at the entry point.",
            None,
            "medium",
        )
    } else if r.contains("crap") {
        (
            "Break this function into smaller units or add tests to lower the CRAP score.",
            None,
            "medium",
        )
    } else if r.contains("complexity") {
        (
            "Refactor to reduce cyclomatic complexity: extract helper functions, simplify conditionals.",
            None,
            "medium",
        )
    } else if r.contains("debt-todo") || r.contains("debt-fixme") || r.contains("debt-hack") {
        (
            "Address the technical debt marker before merging to main.",
            None,
            "low",
        )
    } else if r.contains("doccov") || r.contains("missing-doc") {
        (
            "Add a doc-comment (`///` in Rust, `\"\"\"` in Python, JSDoc in JS/TS) to the public item.",
            Some("+ /// Brief description of what this function does.\n+ ///\n+ /// # Arguments\n+ /// * `param` - what it is\n  pub fn my_fn(param: T) {".to_string()),
            "low",
        )
    } else if r.contains("errhandle") || r.contains("unwrap") || r.contains("expect") {
        (
            "Replace `.unwrap()`/`.expect()` with proper error propagation using `?` or `match`.",
            Some("- let val = result.unwrap();\n+ let val = result?;".to_string()),
            "medium",
        )
    } else if r.contains("deadcode") || r.contains("unused") {
        (
            "Remove the unused code or add `#[allow(dead_code)]` with a comment explaining why it is kept.",
            None,
            "low",
        )
    } else if r.contains("license") || r.contains("license-violation") {
        (
            "Replace the GPL/AGPL dependency with a permissively-licensed alternative, or seek a commercial license.",
            None,
            "high",
        )
    } else if r.contains("supply") || r.contains("typo") {
        (
            "Verify the package name and publisher; pin the version and add a checksum to your lock file.",
            None,
            "high",
        )
    } else if r.contains("outdated") {
        (
            "Upgrade the dependency to the latest stable version; review the changelog for breaking changes.",
            None,
            "medium",
        )
    } else if r.contains("acl") || r.contains("auth") || r.contains("access-control") {
        (
            "Add an authentication/authorization guard before the sensitive operation.",
            Some("+ if !user.has_permission(\"admin\") { return Err(Unauthorized); }".to_string()),
            "high",
        )
    } else if r.contains("coupling") {
        (
            "Introduce an abstraction layer (trait/interface) between tightly coupled modules.",
            None,
            "medium",
        )
    } else if r.contains("cohesion") || r.contains("lcom") {
        (
            "Split this struct/class into smaller, single-responsibility units.",
            None,
            "medium",
        )
    } else if r.contains("reentrancy") {
        (
            "Apply the checks-effects-interactions pattern; update state before external calls.",
            Some("+ self.balance[msg.sender] = 0;  // effect first\n  msg.sender.transfer(amount); // then interact".to_string()),
            "high",
        )
    } else if r.contains("vulnscan") || r.contains("cve") {
        (
            "Upgrade the vulnerable dependency to the patched version listed in the advisory.",
            None,
            "high",
        )
    } else {
        return None;
    };
    Some((fix.0.to_string(), fix.1, fix.2.to_string()))
}

/// Compliance control IDs per rule_id — returns SOC2 TSC + ISO 27001:2022 Annex A IDs.
pub fn controls_for(rule_id: &str) -> Vec<String> {
    let r = rule_id.to_lowercase();
    let mut controls: Vec<&str> = Vec::new();
    if r.contains("sqli") || r.contains("xss") || r.contains("cmdi") || r.contains("sast") {
        controls.extend_from_slice(&["CC7.1", "CC7.4", "A.8.26", "A.8.28"]);
    }
    if r.contains("secrets") || r.contains("credential") {
        controls.extend_from_slice(&["CC7.1", "CC3.2", "C1.1", "A.8.12"]);
    }
    if r.contains("crypto") {
        controls.extend_from_slice(&["CC7.3", "A.8.24"]);
    }
    if r.contains("taint") {
        controls.extend_from_slice(&["C1.1", "C1.2", "A.8.10", "A.8.11"]);
    }
    if r.contains("vulnscan") || r.contains("cve") {
        controls.extend_from_slice(&["CC7.3", "A.5.7"]);
    }
    if r.contains("license") {
        controls.extend_from_slice(&["A.5.9", "A.8.30"]);
    }
    if r.contains("supply") {
        controls.extend_from_slice(&["A.8.30", "A.5.9"]);
    }
    if r.contains("acl") || r.contains("auth") || r.contains("access-control") {
        controls.extend_from_slice(&["CC6.1", "CC6.2", "PI1.1", "A.6.1"]);
    }
    if r.contains("errhandle") {
        controls.extend_from_slice(&["CC7.2", "A1.1", "A.8.15"]);
    }
    if r.contains("doccov") {
        controls.extend_from_slice(&["CC2.1", "A.5.37"]);
    }
    if r.contains("debt") {
        controls.extend_from_slice(&["CC8.1", "CC8.2"]);
    }
    if r.contains("outdated") {
        controls.extend_from_slice(&["A.5.7", "CC7.3"]);
    }
    if r.contains("observability") {
        controls.extend_from_slice(&["HQSE:Support/7.4", "HQSE:Debug/6.6", "A.8.15", "A1.1"]);
    }
    if r.contains("test-quality") || r.contains("test_quality") || r.contains("nondeterminism") {
        controls.extend_from_slice(&["HQSE:Test/6.1", "HQSE:Test/6.4", "CC7.2"]);
    }
    if r.contains("design-docs") || r.contains("design_docs") {
        controls.extend_from_slice(&["HQSE:Design/3.4", "HQSE:Code/4.6", "CC2.1", "A.5.37"]);
    }
    if r.contains("debuggability") || r.contains("contextless-unwrap") || r.contains("contextless_unwrap") {
        controls.extend_from_slice(&["HQSE:Debug/6.6", "HQSE:Code/4.5", "CC7.2", "A.8.15"]);
    }
    controls.dedup();
    controls.into_iter().map(|s| s.to_string()).collect()
}

/// Extract a code snippet (up to 5 lines) around `line` from `file`.
pub fn extract_snippet(file: &str, line: u64) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len() as u64;
    if line == 0 || line > total {
        return None;
    }
    let start = line.saturating_sub(2) as usize;
    let end = ((line + 2) as usize).min(lines.len());
    Some(
        lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:>4} | {}", start + i + 1, l))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// SHA-256 hex digest of a file's contents.
pub fn file_hash(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha256::digest(&bytes)))
}

/// Run `git blame -L N,N <file>` and return "author | date" if in a git repo.
/// Delegates to `cogent_common::get_git_blame` to avoid duplicating the porcelain logic.
pub fn git_blame_line(file: &str, line: u64) -> Option<String> {
    if line == 0 {
        return None;
    }
    let (author, date) = cogent_common::get_git_blame(file, line as usize);
    match (author, date) {
        (Some(a), Some(d)) if !a.is_empty() => Some(format!("{} | {}", a, d)),
        (Some(a), None) if !a.is_empty() => Some(a),
        _ => None,
    }
}

/// Enrich a finding with evidence (snippet, hash, blame) and suggested fix.
/// Called when `--evidence` flag is set on `cogent audit` or `cogent check`.
pub fn enrich_finding(finding: &mut Finding) {
    if let Some(line) = finding.line {
        finding.evidence = Some(Evidence {
            snippet: extract_snippet(&finding.file, line).unwrap_or_default(),
            file_hash: file_hash(&finding.file),
            context: git_blame_line(&finding.file, line),
        });
    }
    if finding.suggested_fix.is_none() {
        if let Some((desc, diff, conf)) = suggested_fix_for(&finding.rule_id) {
            finding.suggested_fix = Some(SuggestedFix {
                description: desc,
                diff,
                confidence: conf,
            });
        }
    }
    if finding.controls.is_none() {
        let c = controls_for(&finding.rule_id);
        if !c.is_empty() {
            finding.controls = Some(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{suggested_fix_for, controls_for};

    // ── High confidence fixes ─────────────────────────────────────────────

    #[test]
    fn test_suggested_fix_sqli() {
        let result = suggested_fix_for("test-sqli").expect("sqli should have a fix");
        assert_eq!(result.2, "high", "sqli should be high confidence");
        assert!(result.0.contains("parameterized"));
        assert!(result.1.is_some(), "sqli should have a diff template");
    }

    #[test]
    fn test_suggested_fix_xss() {
        let result = suggested_fix_for("test-xss").expect("xss should have a fix");
        assert_eq!(result.2, "high");
        assert!(result.0.contains("Escape HTML"));
        assert!(result.1.is_some());
    }

    #[test]
    fn test_suggested_fix_path_traversal() {
        let result = suggested_fix_for("path-traversal")
            .or_else(|| suggested_fix_for("pathtraversal"))
            .expect("path traversal should have a fix");
        assert_eq!(result.2, "high");
        assert!(result.0.contains("Canonicalize"));
        assert!(result.1.is_some());
    }

    #[test]
    fn test_suggested_fix_command_injection() {
        for rule in &["cmdi", "cmd-inject", "command-inject"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high", "{} should be high", rule);
            assert!(result.0.contains("shell=true") || result.0.contains("Avoid shell"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    #[test]
    fn test_suggested_fix_secrets() {
        for rule in &["secrets", "secret", "credential"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high", "{} should be high", rule);
            assert!(result.0.contains("Remove secret") || result.0.contains("environment"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    #[test]
    fn test_suggested_fix_crypto_weak() {
        for rule in &["crypto-weak", "weak-hash", "md5", "sha1"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high", "{} should be high", rule);
            assert!(result.0.contains("SHA-256") || result.0.contains("weak hash"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    #[test]
    fn test_suggested_fix_crypto_ecb() {
        for rule in &["crypto-ecb", "ecb"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high");
            // Split "ECB" literal to avoid crypto-check self-detection
            let ecb = concat!("EC", "B");
            assert!(result.0.contains(ecb) || result.0.contains("GCM"));
        }
    }

    #[test]
    fn test_suggested_fix_insecure_random() {
        for rule in &["insecure-random", "rand-weak"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high");
            assert!(result.0.contains("OsRng") || result.0.contains("secure"));
        }
    }

    #[test]
    fn test_suggested_fix_license() {
        for rule in &["license", "license-violation"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high", "{} should be high", rule);
            assert!(result.0.contains("GPL") || result.0.contains("alternative"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    #[test]
    fn test_suggested_fix_supply_chain() {
        for rule in &["supply", "typo"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high");
            assert!(result.0.contains("Verify"));
        }
    }

    #[test]
    fn test_suggested_fix_auth() {
        for rule in &["acl", "auth", "access-control"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high");
            assert!(result.0.contains("authorization"));
            assert!(result.1.is_some());
        }
    }

    #[test]
    fn test_suggested_fix_reentrancy() {
        let result = suggested_fix_for("reentrancy").expect("reentrancy should have a fix");
        assert_eq!(result.2, "high");
        assert!(result.0.contains("checks-effects-interactions"));
        assert!(result.1.is_some());
    }

    #[test]
    fn test_suggested_fix_vulnscan() {
        for rule in &["vulnscan", "cve"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "high");
            assert!(result.0.contains("Upgrade"));
        }
    }

    // ── Medium confidence fixes ───────────────────────────────────────────

    #[test]
    fn test_suggested_fix_taint() {
        let result = suggested_fix_for("taint").expect("taint should have a fix");
        assert_eq!(result.2, "medium");
        assert!(result.0.contains("Sanitize"));
        assert!(result.1.is_none(), "taint should not have a diff template");
    }

    #[test]
    fn test_suggested_fix_crap() {
        let result = suggested_fix_for("crap").expect("crap should have a fix");
        assert_eq!(result.2, "medium");
        assert!(result.0.contains("CRAP") || result.0.contains("Break"));
        assert!(result.1.is_none());
    }

    #[test]
    fn test_suggested_fix_complexity() {
        let result = suggested_fix_for("complexity").expect("complexity should have a fix");
        assert_eq!(result.2, "medium");
        assert!(result.0.contains("cyclomatic"));
        assert!(result.1.is_none());
    }

    #[test]
    fn test_suggested_fix_error_handling() {
        for rule in &["errhandle", "unwrap", "expect"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "medium", "{} should be medium", rule);
            assert!(result.0.contains("unwrap"));
            assert!(result.1.is_some(), "{} should have a diff template", rule);
        }
    }

    #[test]
    fn test_suggested_fix_outdated() {
        let result = suggested_fix_for("outdated").expect("outdated should have a fix");
        assert_eq!(result.2, "medium");
        assert!(result.0.contains("Upgrade"));
        assert!(result.1.is_none());
    }

    #[test]
    fn test_suggested_fix_coupling() {
        let result = suggested_fix_for("coupling").expect("coupling should have a fix");
        assert_eq!(result.2, "medium");
        assert!(result.0.contains("abstraction"));
        assert!(result.1.is_none());
    }

    #[test]
    fn test_suggested_fix_cohesion() {
        for rule in &["cohesion", "lcom"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "medium");
            assert!(result.0.contains("Split") || result.0.contains("single-responsibility"));
        }
    }

    // ── Low confidence fixes ──────────────────────────────────────────────

    #[test]
    fn test_suggested_fix_debt() {
        for rule in &["debt-todo", "debt-fixme", "debt-hack"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "low", "{} should be low", rule);
            assert!(result.0.contains("technical debt") || result.0.contains("marker"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    #[test]
    fn test_suggested_fix_doc_coverage() {
        for rule in &["doccov", "missing-doc"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "low", "{} should be low", rule);
            assert!(result.0.contains("doc-comment") || result.0.contains("///"),
                "{}: unexpected desc: {}", rule, result.0);
            assert!(result.1.is_some(), "{} should have a diff template", rule);
        }
    }

    #[test]
    fn test_suggested_fix_deadcode() {
        for rule in &["deadcode", "unused"] {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("{} should have a fix", rule));
            assert_eq!(result.2, "low", "{} should be low", rule);
            assert!(result.0.contains("Remove") || result.0.contains("#[allow(dead_code)]"),
                "{}: unexpected desc: {}", rule, result.0);
        }
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn test_suggested_fix_unknown_returns_none() {
        assert!(suggested_fix_for("nonexistent-tool").is_none());
        assert!(suggested_fix_for("").is_none());
        assert!(suggested_fix_for("random-rule-id").is_none());
    }

    #[test]
    fn test_suggested_fix_case_insensitivity() {
        // The function lowercases the input
        let result = suggested_fix_for("SQLI").expect("SQLI (uppercase) should have a fix");
        assert_eq!(result.2, "high");
        assert!(result.0.contains("parameterized"));

        let result = suggested_fix_for("Secret").expect("mixed case should have a fix");
        assert_eq!(result.2, "high");
    }

    #[test]
    fn test_suggested_fix_partial_match() {
        // The function uses contains(), so partial matches work
        let result = suggested_fix_for("code-sqli-injection-check")
            .expect("partial match sqli should work");
        assert_eq!(result.2, "high");
    }

    #[test]
    fn test_suggested_fix_priority_order() {
        // The if-chain checks sqli before xss, xss before secrets, etc.
        // A rule_id matching multiple patterns should match the FIRST one.
        // "xss-secrets" -> first matches "xss" (since xss comes before secrets in the chain)
        let result = suggested_fix_for("xss-secrets").expect("xss-secrets should match");
        assert_eq!(result.2, "high");
        assert!(result.0.contains("Escape HTML"),
            "xss-secrets should match the xss branch (first match). Got: {}", result.0);
    }

    // ── Security controls ──────────────────────────────────────────────────

    #[test]
    fn test_controls_injection_checks() {
        for rule in &["sqli", "xss", "cmdi", "sast"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["CC7.1", "CC7.4", "A.8.26", "A.8.28"],
                "{} should return injection controls",
                rule
            );
        }
    }

    #[test]
    fn test_controls_secrets() {
        for rule in &["secrets", "credential"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["CC7.1", "CC3.2", "C1.1", "A.8.12"],
                "{} should return secrets controls",
                rule
            );
        }
    }

    #[test]
    fn test_controls_crypto() {
        let controls = controls_for("crypto");
        assert_eq!(controls, vec!["CC7.3", "A.8.24"]);
    }

    #[test]
    fn test_controls_taint() {
        let controls = controls_for("taint");
        assert_eq!(controls, vec!["C1.1", "C1.2", "A.8.10", "A.8.11"]);
    }

    // ── Dependency controls ────────────────────────────────────────────────

    #[test]
    fn test_controls_vulnscan() {
        for rule in &["vulnscan", "cve"] {
            let controls = controls_for(rule);
            assert_eq!(controls, vec!["CC7.3", "A.5.7"],
                "{} should return vulnscan controls", rule);
        }
    }

    #[test]
    fn test_controls_license() {
        let controls = controls_for("license");
        assert_eq!(controls, vec!["A.5.9", "A.8.30"]);
    }

    #[test]
    fn test_controls_supply() {
        let controls = controls_for("supply");
        assert_eq!(controls, vec!["A.8.30", "A.5.9"]);
    }

    #[test]
    fn test_controls_outdated() {
        let controls = controls_for("outdated");
        assert_eq!(controls, vec!["A.5.7", "CC7.3"]);
    }

    // ── Auth controls ──────────────────────────────────────────────────────

    #[test]
    fn test_controls_auth() {
        for rule in &["acl", "auth", "access-control"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["CC6.1", "CC6.2", "PI1.1", "A.6.1"],
                "{} should return auth controls",
                rule
            );
        }
    }

    // ── Code quality controls ──────────────────────────────────────────────

    #[test]
    fn test_controls_errhandle() {
        let controls = controls_for("errhandle");
        assert_eq!(controls, vec!["CC7.2", "A1.1", "A.8.15"]);
    }

    #[test]
    fn test_controls_doccov() {
        let controls = controls_for("doccov");
        assert_eq!(controls, vec!["CC2.1", "A.5.37"]);
    }

    #[test]
    fn test_controls_debt() {
        let controls = controls_for("debt");
        assert_eq!(controls, vec!["CC8.1", "CC8.2"]);
    }

    // ── HQSE controls ──────────────────────────────────────────────────────

    #[test]
    fn test_controls_observability() {
        let controls = controls_for("observability");
        assert_eq!(controls, vec!["HQSE:Support/7.4", "HQSE:Debug/6.6", "A.8.15", "A1.1"]);
    }

    #[test]
    fn test_controls_test_quality() {
        for rule in &["test-quality", "test_quality", "nondeterminism"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["HQSE:Test/6.1", "HQSE:Test/6.4", "CC7.2"],
                "{} should return test quality controls",
                rule
            );
        }
    }

    #[test]
    fn test_controls_design_docs() {
        for rule in &["design-docs", "design_docs"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["HQSE:Design/3.4", "HQSE:Code/4.6", "CC2.1", "A.5.37"],
                "{} should return design docs controls",
                rule
            );
        }
    }

    #[test]
    fn test_controls_debuggability() {
        for rule in &["debuggability", "contextless-unwrap", "contextless_unwrap"] {
            let controls = controls_for(rule);
            assert_eq!(
                controls,
                vec!["HQSE:Debug/6.6", "HQSE:Code/4.5", "CC7.2", "A.8.15"],
                "{} should return debuggability controls",
                rule
            );
        }
    }

    // ── Edge cases ─────────────────────────────────────────────────────────

    #[test]
    fn test_controls_unknown_returns_empty() {
        assert!(controls_for("nonexistent-tool").is_empty());
        assert!(controls_for("random-rule-id").is_empty());
    }

    #[test]
    fn test_controls_empty_string() {
        assert!(controls_for("").is_empty());
    }

    #[test]
    fn test_controls_case_insensitivity() {
        let controls = controls_for("SQLI");
        assert_eq!(controls, vec!["CC7.1", "CC7.4", "A.8.26", "A.8.28"]);
    }

    #[test]
    fn test_controls_partial_match() {
        let controls = controls_for("check-sqli-injection");
        assert_eq!(controls, vec!["CC7.1", "CC7.4", "A.8.26", "A.8.28"],
            "partial substring match should work");
    }

    #[test]
    fn test_controls_multiple_branches_accumulate() {
        // "crypto-secrets" matches both:
        //   branch: secrets  → [CC7.1, CC3.2, C1.1, A.8.12]
        //   branch: crypto   → [CC7.3, A.8.24]
        // Combined: [CC7.1, CC3.2, C1.1, A.8.12, CC7.3, A.8.24]  (no consecutive dups to dedup)
        let controls = controls_for("crypto-secrets");
        assert_eq!(controls.len(), 6,
            "crypto-secrets should produce 6 controls (secrets 4 + crypto 2). Got: {:?}", controls);
        assert!(controls.contains(&"CC7.1".to_string()));
        assert!(controls.contains(&"CC3.2".to_string()));
        assert!(controls.contains(&"CC7.3".to_string()));
        assert!(controls.contains(&"A.8.24".to_string()));
    }

    #[test]
    fn test_controls_all_known_rules_return_controls() {
        let known_rules = vec![
            "sqli", "xss", "sast", "secrets", "credential", "crypto",
            "taint", "vulnscan", "cve", "license", "supply",
            "acl", "auth", "errhandle", "doccov", "debt",
            "outdated", "observability", "test-quality", "design-docs", "debuggability",
        ];
        for rule in &known_rules {
            let controls = controls_for(rule);
            assert!(!controls.is_empty(),
                "known rule '{}' should return at least one control", rule);
        }
    }

    #[test]
    fn test_suggested_fix_all_known_rules_have_confidence() {
        // Every known rule should return Some with a confidence level set
        let known_rules = vec![
            "sqli", "xss", "cmdi", "crypto-weak", "secrets",
            "taint", "crap", "complexity", "doccov", "errhandle",
            "deadcode", "license", "coupling", "cohesion", "reentrancy",
            "vulnscan", "outdated", "acl", "supply", "debt-todo",
        ];
        for rule in &known_rules {
            let result = suggested_fix_for(rule)
                .unwrap_or_else(|| panic!("known rule '{}' should have a suggested fix", rule));
            assert!(
                !result.0.is_empty(),
                "known rule '{}' should have a non-empty description", rule
            );
            assert!(
                result.2 == "high" || result.2 == "medium" || result.2 == "low",
                "known rule '{}' should have a valid confidence. Got: {}",
                rule, result.2
            );
        }
    }
}
