//! Fix engine: auto-fix infrastructure for mechanical code issues.
//!
//! Provides patch-based fixers for error handling, dead code, tech debt,
//! secrets, crypto, and documentation coverage.

use quote::ToTokens;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════
// REPLACER ENGINE — auto-fix infrastructure
// ═══════════════════════════════════════════

/// A single atomic code replacement produced by a fixer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixPatch {
    pub file: String,
    pub line: usize,
    /// Original text at this location (for diff and validation).
    pub old_text: String,
    /// Replacement text.
    pub new_text: String,
    /// Which rule triggered this fix.
    pub rule_id: String,
    /// "high" | "medium" | "low" — confidence the fix is correct.
    pub confidence: String,
    /// Human-readable description of what was changed.
    pub description: String,
}

impl FixPatch {
    /// Produce a unified-diff fragment for this patch.
    pub fn to_diff(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("--- {}\n", self.file));
        out.push_str(&format!("+++ {}\n", self.file));
        out.push_str(&format!("@@ -{} +{} @@\n", self.line, self.line));
        for old_line in self.old_text.lines() {
            out.push_str(&format!("-{}\n", old_line));
        }
        for new_line in self.new_text.lines() {
            out.push_str(&format!("+{}\n", new_line));
        }
        out
    }
}

/// Result of applying a set of patches to a single file.
#[derive(Debug)]
pub struct ApplyResult {
    pub file: String,
    pub patches_applied: usize,
    pub patches_rejected: usize,
    pub rejected: Vec<FixPatch>,
}

/// Validation result: does the modified source still parse?
pub fn validate_rust_source(source: &str) -> bool {
    syn::parse_file(source).is_ok()
}

// ─────────────────────────────────────────────
// Patch application
// ─────────────────────────────────────────────

/// Group patches by file, sort by line descending (so offsets don't shift),
/// apply old_text match check, optionally validate with syn, and write.
pub fn apply_patches(patches: &[FixPatch], dry_run: bool, validate: bool) -> Vec<ApplyResult> {
    let mut by_file: std::collections::HashMap<String, Vec<&FixPatch>> =
        std::collections::HashMap::new();
    for p in patches {
        by_file
            .entry(p.file.clone())
            .or_default()
            .push(p);
    }

    let mut results = Vec::new();

    for (file, mut file_patches) in by_file {
        // Sort descending by line so earlier insertions don't shift later ones.
        file_patches.sort_by_key(|b| std::cmp::Reverse(b.line));

        let Ok(src) = std::fs::read_to_string(&file) else {
            results.push(ApplyResult {
                file,
                patches_applied: 0,
                patches_rejected: file_patches.len(),
                rejected: file_patches.into_iter().cloned().collect(),
            });
            continue;
        };
        let mut lines: Vec<String> = src.lines().map(|l| l.to_string()).collect();
        let mut applied = 0usize;
        let mut rejected: Vec<FixPatch> = Vec::new();

        for patch in &file_patches {
            let idx = if patch.line == 0 {
                0
            } else {
                patch.line.saturating_sub(1)
            };

            // Determine how many lines old_text spans.
            let old_lines: Vec<&str> = patch.old_text.lines().collect();
            let span = old_lines.len().max(1);

            // Verify the source at [idx..idx+span] matches old_text.
            let actual: String = lines
                .get(idx..idx + span)
                .map(|slice| slice.join("\n"))
                .unwrap_or_default();

            if actual != patch.old_text {
                rejected.push((*patch).clone());
                continue;
            }

            // Replace the span with new_text.
            let new_lines: Vec<String> =
                patch.new_text.lines().map(|l| l.to_string()).collect();
            lines.splice(idx..idx + span, new_lines);
            applied += 1;
        }

        if applied > 0 && !dry_run {
            let new_src = lines.join("\n");
            if validate && !validate_rust_source(&new_src) {
                // Rollback — don't write broken code
                results.push(ApplyResult {
                    file: file.clone(),
                    patches_applied: 0,
                    patches_rejected: applied + rejected.len(),
                    rejected: file_patches.into_iter().cloned().collect(),
                });
                continue;
            }
            let _ = std::fs::write(&file, new_src);
        }

        results.push(ApplyResult {
            file,
            patches_applied: applied,
            patches_rejected: rejected.len(),
            rejected,
        });
    }

    results
}

// ═══════════════════════════════════════════
// FIXERS — each returns Vec<FixPatch>
// ═══════════════════════════════════════════

/// Run the requested fixer(s) and return all patches (unapplied).
/// `check` is "all" or a specific check name like "errhandle", "deadcode", etc.
pub fn collect_patches(path: &str, check: &str) -> Vec<FixPatch> {
    let mut patches = Vec::new();

    let files = cogent_common::find_source_files(path, true, &["rs"]);

    if check == "all" || check == "errhandle" {
        patches.extend(fixer_errhandle(&files));
    }
    if check == "all" || check == "deadcode" {
        patches.extend(fixer_deadcode(&files));
    }
    if check == "all" || check == "debt" {
        patches.extend(fixer_debt(&files));
    }
    if check == "all" || check == "secrets" {
        patches.extend(fixer_secrets(&files));
    }
    if check == "all" || check == "crypto" {
        patches.extend(fixer_crypto(&files));
    }
    if check == "all" || check == "doccov" {
        patches.extend(fixer_doccov(&files));
    }

    patches
}

// ── errhandle: .unwrap() → .ok_or(..)?, .expect("msg") → .map_err(..)? ──

fn fixer_errhandle(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();
    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            let ln = i + 1;
            let trimmed = line.trim();

            // Skip if already inside a test module or test function
            if trimmed.starts_with('#') {
                continue;
            }

            // .unwrap();
            if let Some(pos) = line.find(".unwrap()") {
                // Check this is a method call, not a comment or string
                if is_code_position(line, pos) {
                    let before = &line[..pos];
                    let after = &line[pos + 9..]; // len of ".unwrap()"

                    // Heuristic: if the expression looks like Result-ish
                    // We replace .unwrap() with ? operator
                    let indent = line.len() - line.trim_start().len();
                    let indent_str = &line[..indent];

                    // Determine context for the error message
                    let expr_name = extract_expr_name(before);
                    let new_line = format!(
                        "{}{}?{}",
                        indent_str,
                        before.trim(),
                        after
                    );
                    let old_line = line.to_string();
                    let desc = format!(
                        "Replace `.unwrap()` with `?` operator on `{}`",
                        expr_name
                    );
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: old_line,
                        new_text: new_line,
                        rule_id: "errhandle-unwrap".into(),
                        confidence: "medium".into(),
                        description: desc,
                    });
                    continue; // Don't double-match expect on same line
                }
            }

            // .expect("msg")
            if let Some(pos) = line.find(".expect(") {
                if is_code_position(line, pos) {
                    let before = &line[..pos];
                    // Find the closing paren of .expect(...)
                    let expect_start = pos + 8; // ".expect(" len
                    if let Some(close) = find_closing_paren(line, expect_start) {
                        let msg_content = &line[expect_start + 1..close];
                        let after = &line[close + 1..];
                        let indent = line.len() - line.trim_start().len();
                        let indent_str = &line[..indent];

                        let expr_name = extract_expr_name(before);
                        // .expect("msg") → .map_err(|e| format!("msg: {e}"))?
                        let new_line = if msg_content.contains('"') {
                            let msg_inner = msg_content.trim_matches('"');
                            format!(
                                "{}{}.map_err(|e| format!(\"{}: {{e}}\"))?{}",
                                indent_str,
                                before.trim(),
                                msg_inner,
                                after
                            )
                        } else {
                            format!(
                                "{}{}.map_err(|e| {{ \"{}: {{e}}\".to_string() }})?{}",
                                indent_str,
                                before.trim(),
                                msg_content,
                                after
                            )
                        };
                        patches.push(FixPatch {
                            file: file.clone(),
                            line: ln,
                            old_text: line.to_string(),
                            new_text: new_line,
                            rule_id: "errhandle-expect".into(),
                            confidence: "medium".into(),
                            description: format!(
                                "Replace `.expect()` with `.map_err(..)?` on `{}`",
                                expr_name
                            ),
                        });
                    }
                }
            }
        }
    }
    patches
}

/// Check that a position in a line is actual code (not in a comment or string literal).
fn is_code_position(line: &str, pos: usize) -> bool {
    let before = &line[..pos];
    let trimmed_before = before.trim_start();
    // If line starts with // or /// it's a comment
    if trimmed_before.starts_with("//") {
        return false;
    }
    // Simple heuristic: count unescaped quotes before position
    let in_string = before.chars().filter(|c| *c == '"').count() % 2 == 1;
    !in_string
}

/// Extract a short expression name from the part before .unwrap()/.expect()
fn extract_expr_name(before: &str) -> String {
    let trimmed = before.trim_end();
    // Walk backwards to find the expression
    let mut end = trimmed.len();
    // Skip trailing whitespace
    while end > 0 && trimmed.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    // Find the start of the last "word" segment
    let start = trimmed[..end]
        .rfind(|c: char| c.is_whitespace() || c == '=' || c == '(' || c == '{' || c == '>')
        .map(|i| i + 1)
        .unwrap_or(0);
    let name = trimmed[start..end].to_string();
    if name.len() > 30 {
        name[..30].to_string()
    } else {
        name
    }
}

/// Find the closing ) matching the ( at `open_pos`.
fn find_closing_paren(line: &str, open_pos: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'(' {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open_pos + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'"' => {
                // Skip string literal
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth == 0 {
        Some(i - 1)
    } else {
        None
    }
}

// ── deadcode: prepend #[allow(dead_code)] ──

fn fixer_deadcode(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();
    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let ln = i + 1;

            // Look for fn items that are non-pub and don't already have #[allow(dead_code)]
            if !trimmed.starts_with("fn ") && !trimmed.starts_with("async fn ") {
                continue;
            }
            // Already has the attribute? Skip.
            if i > 0 && lines[i - 1].trim().contains("#[allow(dead_code)]") {
                continue;
            }
            // Check if there's already an attribute block above
            if i > 0 && lines[i - 1].trim().starts_with("#[") {
                continue;
            }

            let indent = line.len() - line.trim_start().len();
            let indent_str = &line[..indent];

            patches.push(FixPatch {
                file: file.clone(),
                line: ln,
                old_text: line.to_string(),
                new_text: format!("{}#[allow(dead_code)]\n{}", indent_str, line),
                rule_id: "deadcode-annotate".into(),                    confidence: "high".into(),
                    description: "Add `#[allow(dead_code)]` to suppress dead code warning".to_string(),
            });
        }
    }
    patches
}

// ── debt: convert TODO/FIXME/HACK/XXX to tracked format ──

fn fixer_debt(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();
    let markers = ["TODO:", "FIXME:", "HACK:", "XXX:", "todo:", "fixme:", "hack:", "xxx:"];

    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            let ln = i + 1;
            for marker in &markers {
                if let Some(pos) = line.find(marker) {
                    if !is_code_position(line, pos) && !line.trim().starts_with("//") {
                        continue;
                    }
                    // Check if it already has a (#N) tracking reference
                    let after_marker = &line[pos + marker.len()..];
                    if after_marker.trim_start().starts_with('#')
                        || after_marker.trim_start().starts_with('(')
                        || after_marker.trim_start().starts_with('[')
                    {
                        continue; // Already tracked
                    }

                    // Get the comment text after the marker
                    let comment_text = after_marker.trim();
                    let upper_marker = marker.to_uppercase();

                    // Replace with tracked format: MARKER: description → MARKER(#0): description
                    // #0 means "issue not yet filed" — the developer replaces with real number
                    let new_line = line.replace(
                        &format!("{}{}", marker, after_marker),
                        &format!("{}(#0): {}", upper_marker, comment_text),
                    );

                    // Only emit if actually different
                    if new_line != line {
                        patches.push(FixPatch {
                            file: file.clone(),
                            line: ln,
                            old_text: line.to_string(),
                            new_text: new_line,
                            rule_id: "debt-track".into(),
                            confidence: "high".into(),
                            description: format!(
                                "Convert `{}` to tracked `{}` format for issue tracking",
                                marker.trim_end_matches(':'),
                                upper_marker.trim_end_matches(':')
                            ),
                        });
                    }
                    break; // One marker per line
                }
            }
        }
    }
    patches
}

// ── secrets: replace hardcoded strings with env::var() / dotenv!() ──

fn fixer_secrets(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();
    // Patterns that look like hardcoded secrets
    let secret_patterns = [
        ("api_key", "API_KEY"),
        ("apikey", "API_KEY"),
        ("API_KEY", "API_KEY"),
        ("secret", "SECRET"),
        ("SECRET", "SECRET"),
        ("password", "PASSWORD"),
        ("PASSWORD", "PASSWORD"),
        ("token", "TOKEN"),
        ("TOKEN", "TOKEN"),
        ("auth_token", "AUTH_TOKEN"),
        ("AUTH_TOKEN", "AUTH_TOKEN"),
        ("access_key", "ACCESS_KEY"),
        ("ACCESS_KEY", "ACCESS_KEY"),
        ("private_key", "PRIVATE_KEY"),
        ("PRIVATE_KEY", "PRIVATE_KEY"),
    ];

    // Suspicious string patterns
    let suspicious_prefixes = [
        "sk-", "sk_live_", "sk_test_",       // Stripe
        "ghp_", "gho_", "ghu_", "ghs_",      // GitHub
        "AKIA",                                // AWS
        "xoxb-", "xoxp-", "xoxa-",            // Slack
        "eyJ",                                 // JWT
        "key=\"", "key = \"",                  // Generic key assignment
    ];

    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            let ln = i + 1;
            let trimmed = line.trim();

            // Skip comments and imports
            if trimmed.starts_with("//") || trimmed.starts_with("use ") || trimmed.starts_with('#') {
                continue;
            }

            // Pattern 1: const/static assignments with secret-like names
            for (pattern_name, env_name) in &secret_patterns {
                let assignments = [
                    format!("const {}: &str = ", pattern_name),
                    format!("static {}: &str = ", pattern_name),
                    format!("let {} = ", pattern_name),
                    format!("let mut {} = ", pattern_name),
                ];
                for assignment in &assignments {
                    if let Some(pos) = line.find(assignment.as_str()) {
                        // Find the string literal value
                        if let Some(val) = extract_string_literal(&line[pos + assignment.len()..]) {
                            // Skip empty or placeholder values
                            if val.len() < 4 || val.contains("TODO") || val.contains("your") || val.contains("xxx") {
                                continue;
                            }
                            // Check if it actually looks like a secret (not a URL or format string)
                            if val.starts_with("http") || val.starts_with('/') || val.contains("{}") {
                                continue;
                            }

                            let indent = line.len() - line.trim_start().len();
                            let indent_str = &line[..indent];
                            // For const we can't use ?, use expect for const context
                            let env_msg = format_args!("{} must be set", env_name);
                            let keyword = line.split_whitespace().next().unwrap_or("let");
                            let new_line = if line.trim().starts_with("const") || line.trim().starts_with("static") {
                                format!(
                                    "{}{}std::env::var(\"{}\").expect(\"{}\");",
                                    indent_str, keyword, env_name, env_msg
                                )
                            } else {
                                format!(
                                    "{}{}std::env::var(\"{}\")?;",
                                    indent_str,
                                    if line.trim().starts_with("let mut") {
                                        "let mut "
                                    } else if line.trim().starts_with("let") {
                                        "let "
                                    } else {
                                        ""
                                    },
                                    env_name
                                )
                            };

                            patches.push(FixPatch {
                                file: file.clone(),
                                line: ln,
                                old_text: line.to_string(),
                                new_text: new_line,
                                rule_id: "secrets-env".into(),
                                confidence: "low".into(),
                                description: format!(
                                    "Replace hardcoded `{}` with `std::env::var(\"{}\")`",
                                    pattern_name, env_name
                                ),
                            });
                        }
                    }
                }
            }

            // Pattern 2: Suspicious string literals (API keys, tokens)
            for prefix in &suspicious_prefixes {
                if line.contains(prefix) && is_code_position(line, line.find(prefix).unwrap_or(0)) {
                    // Don't flag if it's already env::var or dotenv
                    if line.contains("env::") || line.contains("dotenv!") || line.contains("var(") {
                        continue;
                    }
                    // Extract the variable name being assigned to
                    let var_name = extract_var_name(line);
                    let env_name = if var_name.is_empty() {
                        "SECRET".to_string()
                    } else {
                        var_name.to_uppercase()
                    };

                    // Only emit one patch per line
                    if patches.iter().any(|p| p.file == *file && p.line == ln) {
                        continue;
                    }

                    let indent = line.len() - line.trim_start().len();
                    let new_line = format!(
                        "{}let {} = std::env::var(\"{}\")?;",
                        &line[..indent],
                        var_name,
                        env_name,
                    );
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: line.to_string(),
                        new_text: new_line,
                        rule_id: "secrets-literal".into(),
                        confidence: "low".into(),
                        description: format!(
                            "Replace hardcoded secret literal with `std::env::var(\"{}\")`",
                            env_name
                        ),
                    });
                    break;
                }
            }
        }
    }
    patches
}

fn extract_string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(s[1..end + 1].to_string())
    } else {
        None
    }
}

fn extract_var_name(line: &str) -> String {
    let trimmed = line.trim();
    // Try "let name = " or "const name = " or "let mut name = "
    for prefix in &["let mut ", "let ", "const ", "static "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            return name;
        }
    }
    String::new()
}

// ── crypto: md5→sha2, sha1→sha2, weak rand→OsRng ──

fn fixer_crypto(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();

    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let ln = i + 1;

            // Skip comments
            if line.trim().starts_with("//") {
                continue;
            }

            // md5::compute(data) → sha2::Sha256::digest(data)
            if line.contains("md5::compute(") || line.contains("md5::Md5") {
                let new_line = line
                    .replace("md5::compute(", "sha2::Sha256::digest(")
                    .replace("md5::Md5", "sha2::Sha256");
                if new_line != *line {
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: line.to_string(),
                        new_text: new_line,
                        rule_id: "crypto-weak-hash".into(),
                        confidence: "medium".into(),
                        description: "Replace MD5 with SHA-256".into(),
                    });
                    continue;
                }
            }

            // sha1::Sha1 / sha1::digest → sha2::Sha256
            if line.contains("sha1::") {
                let new_line = line
                    .replace("sha1::Sha1", "sha2::Sha256")
                    .replace("sha1::digest(", "sha2::Sha256::digest(");
                if new_line != *line {
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: line.to_string(),
                        new_text: new_line,
                        rule_id: "crypto-weak-hash".into(),
                        confidence: "medium".into(),
                        description: "Replace SHA-1 with SHA-256".into(),
                    });
                    continue;
                }
            }

            // rand::thread_rng() → rand::rngs::OsRng (for security contexts)
            if line.contains("rand::thread_rng()") {
                let new_line = line.replace("rand::thread_rng()", "rand::rngs::OsRng");
                if new_line != *line {
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: line.to_string(),
                        new_text: new_line,
                        rule_id: "crypto-weak-rng".into(),
                        confidence: "medium".into(),
                        description: "Replace `thread_rng()` with `OsRng` for cryptographic use".into(),
                    });
                    continue;
                }
            }

            // rand::random() for security-sensitive contexts → OsRng
            if line.contains("rand::random()") {
                // Check context: key/secret/token/nonce/salt/iv
                let sensitive_context = [
                    "key", "secret", "token", "nonce", "salt", "iv",
                    "Key", "Secret", "Token", "Nonce", "Salt", "IV",
                    "password", "Password",
                ];
                let is_sensitive = sensitive_context.iter().any(|kw| line.contains(kw));
                if is_sensitive {
                    let new_line = line.replace(
                        "rand::random()",
                        "rand::rngs::OsRng.gen()",
                    );
                    patches.push(FixPatch {
                        file: file.clone(),
                        line: ln,
                        old_text: line.to_string(),
                        new_text: new_line,
                        rule_id: "crypto-weak-rng".into(),
                        confidence: "low".into(),
                        description: "Replace `rand::random()` with `OsRng` in security context".into(),
                    });
                }
            }
        }

        // Import-level fixes: use md5; → use sha2::{Sha256, Digest};
        let mut import_patches = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let ln = i + 1;
            if line.trim() == "use md5;" {
                import_patches.push(FixPatch {
                    file: file.clone(),
                    line: ln,
                    old_text: line.to_string(),
                    new_text: "use sha2::{Sha256, Digest};".into(),
                    rule_id: "crypto-weak-hash".into(),
                    confidence: "high".into(),
                    description: "Replace `use md5` with `use sha2::{Sha256, Digest}`".into(),
                });
            }
            if line.trim() == "use sha1;" || line.trim() == "use sha1::Sha1;" {
                import_patches.push(FixPatch {
                    file: file.clone(),
                    line: ln,
                    old_text: line.to_string(),
                    new_text: "use sha2::{Sha256, Digest};".into(),
                    rule_id: "crypto-weak-hash".into(),
                    confidence: "high".into(),
                    description: "Replace `use sha1` with `use sha2::{Sha256, Digest}`".into(),
                });
            }
            if line.contains("use rand;") {
                import_patches.push(FixPatch {
                    file: file.clone(),
                    line: ln,
                    old_text: line.to_string(),
                    new_text: "use rand::rngs::OsRng;\nuse rand::RngCore;".into(),
                    rule_id: "crypto-weak-rng".into(),
                    confidence: "high".into(),
                    description: "Replace `use rand` with `use rand::rngs::OsRng`".into(),
                });
            }
        }
        patches.extend(import_patches);
    }

    patches
}

// ── doccov: smarter syn-based doc stubs ──

fn fixer_doccov(files: &[String]) -> Vec<FixPatch> {
    let mut patches = Vec::new();

    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };

        // Try syn-based parsing for rich stubs
        if let Ok(syntax) = syn::parse_file(&src) {
            use syn::visit::Visit;
            let mut visitor = DocCovVisitor {
                file: file.clone(),
                src: &src,
                patches: Vec::new(),
            };
            visitor.visit_file(&syntax);
            patches.extend(visitor.patches);
        } else {
            // Fallback: regex-style (existing scaffold_doc_stubs logic)
            let lines: Vec<&str> = src.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                let ln = i + 1;
                let is_public = trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("pub struct ")
                    || trimmed.starts_with("pub enum ")
                    || trimmed.starts_with("pub trait ");
                if !is_public {
                    continue;
                }
                let has_doc = i > 0 && lines[i - 1].trim().starts_with("///");
                if has_doc {
                    continue;
                }
                let indent = line.len() - line.trim_start().len();
                let item_name = trimmed
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("item")
                    .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
                patches.push(FixPatch {
                    file: file.clone(),
                    line: ln,
                    old_text: line.to_string(),
                    new_text: format!(
                        "{}/// TODO: document `{}`\n{}",
                        &line[..indent],
                        item_name,
                        line
                    ),
                    rule_id: "doccov-stub".into(),
                    confidence: "high".into(),
                    description: format!("Add doc stub for `{}`", item_name),
                });
            }
        }
    }

    patches
}

/// Syn visitor that finds undocumented public items and generates rich doc stubs.
struct DocCovVisitor<'a> {
    file: String,
    src: &'a str,
    patches: Vec<FixPatch>,
}

impl<'a> syn::visit::Visit<'a> for DocCovVisitor<'a> {
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        // Check if public
        if node.vis.to_token_stream().to_string().starts_with("pub") {
            let has_doc = node.attrs.iter().any(|a| {
                a.path().to_token_stream().to_string() == "doc"
            });
            if !has_doc {
                let line = node.sig.fn_token.span.start().line;
                let fn_name = node.sig.ident.to_string();

                // Build param list for doc
                let mut params = Vec::new();
                for arg in &node.sig.inputs {
                    match arg {
                        syn::FnArg::Typed(pt) => {
                            let name = pt.pat.to_token_stream().to_string();
                            let ty = pt.ty.to_token_stream().to_string();
                            params.push(format!("* `{}` - {}", name, ty));
                        }
                        syn::FnArg::Receiver(_) => {
                            params.push("* `&self` - ".into());
                        }
                    }
                }

                // Build return type
                let returns = match &node.sig.output {
                    syn::ReturnType::Default => String::new(),
                    syn::ReturnType::Type(_, ty) => {
                        let ty_str = ty.to_token_stream().to_string();
                        format!("\n///\n/// # Returns\n/// `{}`", ty_str)
                    }
                };

                let param_section = if params.is_empty() {
                    String::new()
                } else {
                    format!("\n///\n/// # Arguments\n/// {}", params.join("\n/// "))
                };

                let indent = get_indent_at_line(self.src, line);
                let stub = format!(
                    "{}/// `{}` — TODO: describe\n{}{}{}",
                    indent,
                    fn_name,
                    indent,
                    param_section,
                    returns
                );

                self.patches.push(FixPatch {
                    file: self.file.clone(),
                    line,
                    old_text: get_line(self.src, line),
                    new_text: format!("{}\n{}", stub, get_line(self.src, line)),
                    rule_id: "doccov-stub".into(),
                    confidence: "high".into(),
                    description: format!("Add rich doc stub for `fn {}`", fn_name),
                });
            }
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'a syn::ItemStruct) {
        if node.vis.to_token_stream().to_string().starts_with("pub") {
            let has_doc = node.attrs.iter().any(|a| {
                a.path().to_token_stream().to_string() == "doc"
            });
            if !has_doc {
                let line = node.struct_token.span.start().line;
                let name = node.ident.to_string();
                let indent = get_indent_at_line(self.src, line);
                let stub = format!("{}/// `{}` — TODO: describe this struct", indent, name);

                self.patches.push(FixPatch {
                    file: self.file.clone(),
                    line,
                    old_text: get_line(self.src, line),
                    new_text: format!("{}\n{}", stub, get_line(self.src, line)),
                    rule_id: "doccov-stub".into(),
                    confidence: "high".into(),
                    description: format!("Add doc stub for `struct {}`", name),
                });
            }
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'a syn::ItemEnum) {
        if node.vis.to_token_stream().to_string().starts_with("pub") {
            let has_doc = node.attrs.iter().any(|a| {
                a.path().to_token_stream().to_string() == "doc"
            });
            if !has_doc {
                let line = node.enum_token.span.start().line;
                let name = node.ident.to_string();
                let indent = get_indent_at_line(self.src, line);
                let stub = format!("{}/// `{}` — TODO: describe this enum", indent, name);

                self.patches.push(FixPatch {
                    file: self.file.clone(),
                    line,
                    old_text: get_line(self.src, line),
                    new_text: format!("{}\n{}", stub, get_line(self.src, line)),
                    rule_id: "doccov-stub".into(),
                    confidence: "high".into(),
                    description: format!("Add doc stub for `enum {}`", name),
                });
            }
        }
        syn::visit::visit_item_enum(self, node);
    }
}

fn get_line(src: &str, line: usize) -> String {
    src.lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .to_string()
}

fn get_indent_at_line(src: &str, line: usize) -> String {
    let l = get_line(src, line);
    let indent = l.len() - l.trim_start().len();
    " ".repeat(indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── FixPatch tests ──────────────────────────────────────────────

    #[test]
    fn test_fix_patch_to_diff() {
        let patch = FixPatch {
            file: "src/main.rs".into(),
            line: 10,
            old_text: "let x = 1;".into(),
            new_text: "let x = 2;".into(),
            rule_id: "test-rule".into(),
            confidence: "high".into(),
            description: "test patch".into(),
        };
        let diff = patch.to_diff();
        assert!(diff.contains("--- src/main.rs"));
        assert!(diff.contains("+++ src/main.rs"));
        assert!(diff.contains("-let x = 1;"));
        assert!(diff.contains("+let x = 2;"));
    }

    // ── validate_rust_source ────────────────────────────────────────

    #[test]
    fn test_validate_rust_source_valid() {
        assert!(validate_rust_source("fn main() { println!(\"hello\"); }"));
    }

    #[test]
    fn test_validate_rust_source_invalid() {
        assert!(!validate_rust_source("fn main() { println!(\"hello\" "));
    }

    // ── apply_patches ───────────────────────────────────────────────

    #[test]
    fn test_apply_patches_dry_run() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "let x = 1;").unwrap();
        writeln!(f, "let y = 2;").unwrap();

        let patches = vec![FixPatch {
            file: f.path().to_str().unwrap().to_string(),
            line: 1,
            old_text: "let x = 1;".into(),
            new_text: "let x = 10;".into(),
            rule_id: "test".into(),
            confidence: "high".into(),
            description: "test".into(),
        }];

        let results = apply_patches(&patches, true, true);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].patches_applied, 1);
        assert_eq!(results[0].patches_rejected, 0);
    }

    #[test]
    fn test_apply_patches_rejected_when_old_text_mismatch() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "let x = 1;").unwrap();

        let patches = vec![FixPatch {
            file: f.path().to_str().unwrap().to_string(),
            line: 1,
            old_text: "let x = 999;".into(),  // doesn't match
            new_text: "let x = 10;".into(),
            rule_id: "test".into(),
            confidence: "high".into(),
            description: "test".into(),
        }];

        let results = apply_patches(&patches, false, false);
        assert_eq!(results[0].patches_applied, 0);
        assert_eq!(results[0].patches_rejected, 1);
    }

    // ── is_code_position ────────────────────────────────────────────

    #[test]
    fn test_is_code_position_true() {
        assert!(is_code_position("let x = result.unwrap();", 15));
    }

    #[test]
    fn test_is_code_position_false_comment() {
        assert!(!is_code_position("// let x = result.unwrap();", 5));
    }

    // ── extract_expr_name ───────────────────────────────────────────

    #[test]
    fn test_extract_expr_name_simple() {
        assert_eq!(extract_expr_name("let x = result"), "result");
    }

    #[test]
    fn test_extract_expr_name_chained() {
        let name = extract_expr_name("let x = get_data()");
        assert!(!name.is_empty(), "should extract a non-empty expression name");
        assert!(!name.contains('('), "should not include opening paren: got '{}'", name);
    }

    #[test]
    fn test_extract_expr_name_truncated() {
        let long = "a".repeat(40);
        let result = extract_expr_name(&format!("let x = {}", long));
        assert_eq!(result.len(), 30);
    }

    // ── find_closing_paren ──────────────────────────────────────────

    #[test]
    fn test_find_closing_paren_basic() {
        assert_eq!(find_closing_paren("foo(bar)", 3), Some(7));
    }

    #[test]
    fn test_find_closing_paren_nested() {
        assert_eq!(find_closing_paren("foo(bar(baz))", 3), Some(12));
    }

    #[test]
    fn test_find_closing_paren_no_match() {
        assert_eq!(find_closing_paren("foo(bar", 3), None);
    }

    #[test]
    fn test_find_closing_paren_with_string() {
        // String literal with parens inside should not confuse the matcher
        assert_eq!(find_closing_paren("foo(\"(\")", 3), Some(7));
    }

    // ── extract_string_literal ──────────────────────────────────────

    #[test]
    fn test_extract_string_literal_valid() {
        assert_eq!(
            extract_string_literal("\"hello\"").as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn test_extract_string_literal_no_quote() {
        assert!(extract_string_literal("hello").is_none());
    }

    // ── extract_var_name ────────────────────────────────────────────

    #[test]
    fn test_extract_var_name_let() {
        assert_eq!(extract_var_name("let api_key = \"value\";"), "api_key");
    }

    #[test]
    fn test_extract_var_name_const() {
        assert_eq!(extract_var_name("const SECRET: &str = \"value\";"), "SECRET");
    }

    #[test]
    fn test_extract_var_name_empty() {
        assert_eq!(extract_var_name("println!(\"hello\");"), "");
    }

    // ── fixer_errhandle: .unwrap() → ? ──────────────────────────────

    #[test]
    fn test_fixer_errhandle_detects_unwrap() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "fn test() {{ let x = get_value().unwrap(); }}").unwrap();
        let patches = fixer_errhandle(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should detect .unwrap()");
        assert!(patches[0].rule_id.contains("unwrap"));
    }

    #[test]
    fn test_fixer_errhandle_skips_comments() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "// let x = get_value().unwrap();").unwrap();
        let patches = fixer_errhandle(&[f.path().to_str().unwrap().to_string()]);
        assert!(patches.is_empty(), "should skip comment lines");
    }

    // ── fixer_debt: TODO → TODO(#0) ────────────────────────────────

    #[test]
    fn test_fixer_debt_converts_todo() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "// TODO: implement this").unwrap();
        let patches = fixer_debt(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should convert TODO");
        // The format uses TODO:(#0): — colon before parens
        assert!(patches[0].new_text.contains("TODO:(#0):") || patches[0].new_text.contains("(#0):"),
            "new_text should contain issue tracker reference, got: {}", patches[0].new_text);
    }

    #[test]
    fn test_fixer_debt_skips_tracked() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "// TODO(#42): implement this").unwrap();
        let patches = fixer_debt(&[f.path().to_str().unwrap().to_string()]);
        assert!(patches.is_empty(), "should skip already-tracked TODO");
    }

    // ── fixer_deadcode: #[allow(dead_code)] ─────────────────────────

    #[test]
    fn test_fixer_deadcode_adds_allow() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "fn unused_function() {{}}").unwrap();
        let patches = fixer_deadcode(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should add #[allow(dead_code)]");
        assert!(patches[0].new_text.contains("#[allow(dead_code)]"));
    }

    #[test]
    fn test_fixer_deadcode_skips_pub() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "pub fn used_function() {{}}").unwrap();
        let patches = fixer_deadcode(&[f.path().to_str().unwrap().to_string()]);
        assert!(patches.is_empty(), "should skip pub functions");
    }

    // ── fixer_crypto: MD5 → SHA-256 ────────────────────────────────

    #[test]
    fn test_fixer_crypto_replaces_md5() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "let hash = md5::compute(data);").unwrap();
        let patches = fixer_crypto(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should replace md5");
        assert!(patches[0].new_text.contains("sha2::Sha256::digest"));
    }

    #[test]
    fn test_fixer_crypto_replaces_sha1() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "use sha1::Sha1;").unwrap();
        writeln!(f, "let hash = sha1::Sha1::digest(data);").unwrap();
        let patches = fixer_crypto(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should replace sha1");
    }

    #[test]
    fn test_fixer_crypto_replaces_import() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "use md5;").unwrap();
        let patches = fixer_crypto(&[f.path().to_str().unwrap().to_string()]);
        assert!(!patches.is_empty(), "should replace import");
        assert!(patches[0].new_text.contains("sha2::"));
    }

    // ── fixer_doccov: doc stubs ─────────────────────────────────────

    #[test]
    fn test_fixer_doccov_adds_stub_for_pub_fn() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "/// existing doc").unwrap();
        writeln!(f, "pub fn documented() {{}}").unwrap();
        writeln!(f, "pub fn undocumented() {{}}").unwrap();
        let patches = fixer_doccov(&[f.path().to_str().unwrap().to_string()]);
        // Only the undocumented function should get a stub
        let undocumented_patches: Vec<&FixPatch> = patches.iter().filter(|p| p.description.contains("undocumented")).collect();
        assert_eq!(undocumented_patches.len(), 1);
    }

    #[test]
    fn test_fixer_doccov_skips_private() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "fn private_fn() {{}}").unwrap();
        let patches = fixer_doccov(&[f.path().to_str().unwrap().to_string()]);
        assert!(patches.is_empty(), "should skip private functions");
    }

    // ── collect_patches ─────────────────────────────────────────────

    #[test]
    fn test_collect_patches_all() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "// TODO: fix this").unwrap();
        f.flush().unwrap();
        let patches = collect_patches(f.path().to_str().unwrap(), "all");
        // Should find patches for at least the debt fixer
        assert!(!patches.is_empty(), "should find patches, got {}", patches.len());
        assert!(patches.iter().any(|p| p.rule_id == "debt-track"),
            "should find deb-track patch, got rules: {:?}",
            patches.iter().map(|p| &p.rule_id).collect::<Vec<_>>());
    }

    #[test]
    fn test_collect_patches_specific_check() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "let x = get_value().unwrap();").unwrap();
        f.flush().unwrap();
        let patches = collect_patches(f.path().to_str().unwrap(), "errhandle");
        assert!(!patches.is_empty(), "should find errhandle patches, got {}", patches.len());
    }
}

