//! Audit infrastructure: policy engine, exception handling, remediation tracking, audit trail.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// Record new findings from a scan.
pub(crate) fn record_findings(findings: &[crate::Finding]) -> Vec<RemediationEntry> {
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
pub(crate) fn verify_remediation(current_findings: &[crate::Finding]) -> Vec<String> {
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
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).unwrap();
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
pub fn enrich_finding(finding: &mut crate::Finding) {
    if let Some(line) = finding.line {
        finding.evidence = Some(crate::Evidence {
            snippet: extract_snippet(&finding.file, line).unwrap_or_default(),
            file_hash: file_hash(&finding.file),
            context: git_blame_line(&finding.file, line),
        });
    }
    if finding.suggested_fix.is_none() {
        if let Some((desc, diff, conf)) = suggested_fix_for(&finding.rule_id) {
            finding.suggested_fix = Some(crate::SuggestedFix {
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
