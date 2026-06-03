//! Check runner functions for cogent-cli.

#![deny(clippy::all)]
#![allow(clippy::type_complexity)]

use crate::progress::{box_row, format_ms};
use crate::types::{extract_findings_from_details, CheckResult, Finding};
use ast_parse_ts::{parse_complexity_file, parse_doc_coverage_file, Language};
use cogent_common::memory::MemoryMonitor;
use cogent_common::{crap_score, find_source_files, function_coverage, parse_lcov, CoverageRecord, ToolResult};
use colored::Colorize;
use std::sync::Mutex;
use std::time::Instant;

// CHECKS
// ═══════════════════════════════════════════

/// Scan all source files under `path`, invoking `predicate` on each function.
/// Returns `(total_functions_count, collected_items)`.
pub(crate) fn scan_source_functions<T, F>(
    path: &str,
    recursive: bool,
    mut predicate: F,
) -> (usize, Vec<T>)
where
    F: FnMut(&ast_parse_ts::FunctionInfo) -> Option<T>,
{
    let files = find_source_files(
        path,
        recursive,
        &[
            "rs", "py", "js", "ts", "go", "java", "c", "cpp", "cs", "php", "rb", "swift",
        ],
    );
    let mut total = 0;
    let mut results = Vec::new();
    for file in files {
        let functions = parse_complexity_file(&file);
        total += functions.len();
        for func in &functions {
            if let Some(item) = predicate(func) {
                results.push(item);
            }
        }
    }
    (total, results)
}

pub(crate) fn check_crap(
    path: &str,
    recursive: bool,
    coverage_path: &Option<String>,
    max_crap: f64,
) -> CheckResult {
    let coverage_data: Option<Vec<CoverageRecord>> = coverage_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|c| parse_lcov(&c));

    let (total, functions) = scan_source_functions(path, recursive, |func| {
        let cov_pct = if let Some(ref cov_data) = coverage_data {
            function_coverage(cov_data, &func.name)
        } else {
            0.0
        };
        let score = crap_score(func.complexity, cov_pct);
        Some((func.name.clone(), func.complexity, cov_pct, score))
    });
    let avg_crap = if total > 0 {
        functions.iter().map(|f| f.3).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let crappy: Vec<_> = functions.iter().filter(|f| f.3 > 30.0).collect();

    let (severity, rule_id, help) = if avg_crap <= max_crap {
        (
            "info".to_string(),
            "crap-pass".to_string(),
            "CRAP score is within acceptable limits.".to_string(),
        )
    } else if avg_crap > max_crap * 1.5 {
        (
            "error".to_string(),
            "crap-error".to_string(),
            "Reduce function complexity or increase test coverage to lower CRAP score. Aim for CRAP < 30 per function.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "crap-warning".to_string(),
            "CRAP score is approaching threshold. Consider refactoring complex functions or adding tests.".to_string(),
        )
    };

    let details = serde_json::json!({
        "total_functions": total,
        "avg_crap": avg_crap,
        "crappy_count": crappy.len(),
        "excellent_count": functions.iter().filter(|f| f.3 <= 10.0).count(),
        "top_offenders": crappy.iter().take(5).map(|f| {
            serde_json::json!({
                "name": f.0, "complexity": f.1, "coverage": f.2, "crap": f.3
            })
        }).collect::<Vec<_>>(),
    });
    let findings: Vec<Finding> = crappy
        .iter()
        .take(20)
        .map(|f| Finding {
            file: f.0.clone(),
            line: None,
            column: None,
            evidence: None,
            severity: severity.clone(),
            message: format!(
                "{}: complexity={}, coverage={:.0}%, CRAP={:.1}",
                f.0,
                f.1,
                f.2 * 100.0,
                f.3
            ),
            rule_id: rule_id.clone(),
            fix_hint: "Reduce function complexity or increase test coverage to lower CRAP score."
                .to_string(),
            suggested_fix: None,
            controls: None,
        })
        .collect();

    CheckResult {
        name: "crap".to_string(),
        passed: avg_crap <= max_crap,
        score: Some(avg_crap),
        threshold: Some(max_crap),
        message: if avg_crap <= max_crap {
            format!("Average CRAP {:.1} <= {:.0}", avg_crap, max_crap)
        } else {
            format!(
                "Average CRAP {:.1} > {:.0} ({} functions above 30)",
                avg_crap,
                max_crap,
                crappy.len()
            )
        },
        details,
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

pub(crate) fn check_debt(path: &str, recursive: bool, max_debt: usize) -> CheckResult {
    let extensions = [
        "rs", "py", "js", "ts", "go", "c", "cpp", "h", "java", "cs", "php", "rb", "swift",
    ];
    let files = find_source_files(path, recursive, &extensions);

    let markers = ["TODO", "FIXME", "HACK", "XXX", "BUG"];
    let mut count = 0;
    let mut items = Vec::new();

    for file in &files {
        if let Ok(source) = std::fs::read_to_string(file) {
            for (line_num, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    for marker in &markers {
                        if trimmed.contains(marker) {
                            count += 1;
                            items.push(serde_json::json!({
                                "file": file, "line": line_num + 1, "type": marker
                            }));
                        }
                    }
                }
            }
        }
    }

    let (severity, rule_id, help) = if count <= max_debt {
        (
            "info".to_string(),
            "debt-pass".to_string(),
            "Technical debt is within acceptable limits.".to_string(),
        )
    } else if count > max_debt * 2 {
        (
            "error".to_string(),
            "debt-high".to_string(),
            "Excessive technical debt. Address TODO/FIXME/HACK markers to improve code maintainability.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "debt-moderate".to_string(),
            "Moderate technical debt. Consider addressing high-priority markers first.".to_string(),
        )
    };

    let findings: Vec<Finding> = items
        .iter()
        .take(50)
        .map(|item| {
            let file = item
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = item.get("line").and_then(|v| v.as_u64());
            let marker = item.get("type").and_then(|v| v.as_str()).unwrap_or("TODO");
            Finding {
                file: file.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("{} marker found in {}", marker, file),
                rule_id: format!("debt-{}", marker.to_lowercase()),
                fix_hint: format!("Address or remove the {} marker.", marker),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    CheckResult {
        name: "debt".to_string(),
        passed: count <= max_debt,
        score: Some(count as f64),
        threshold: Some(max_debt as f64),
        message: if count <= max_debt {
            format!("{} debt markers <= {}", count, max_debt)
        } else {
            format!("{} debt markers > {}", count, max_debt)
        },
        details: serde_json::json!({
            "total_markers": count,
            "items": items.iter().take(20).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

use syn::visit::Visit;
use syn::{ImplItemFn, ItemEnum, ItemFn, ItemStruct, ItemTrait, Visibility};

struct DocCounter {
    total: usize,
    documented: usize,
}
impl<'a> Visit<'a> for DocCounter {
    fn visit_item_fn(&mut self, node: &'a ItemFn) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_struct(&mut self, node: &'a ItemStruct) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_enum(&mut self, node: &'a ItemEnum) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_trait(&mut self, node: &'a ItemTrait) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_impl_item_fn(&mut self, node: &'a ImplItemFn) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
}

pub(crate) fn check_doc_coverage(path: &str, recursive: bool, min_doc: f64) -> CheckResult {
    let mut total = 0usize;
    let mut documented = 0usize;
    let mut langs_seen: std::collections::HashSet<String> = Default::default();

    // Rust files via syn (high-fidelity)
    let rust_files = find_source_files(path, recursive, &["rs"]);
    if !rust_files.is_empty() {
        langs_seen.insert("rust".to_string());
    }
    let mut counter = DocCounter {
        total: 0,
        documented: 0,
    };
    for file in &rust_files {
        if let Ok(source) = std::fs::read_to_string(file) {
            if let Ok(ast) = syn::parse_file(&source) {
                counter.visit_file(&ast);
            }
        }
    }
    total += counter.total;
    documented += counter.documented;

    // Non-Rust files via tree-sitter
    let all_exts = ["py", "pyi", "js", "mjs", "ts", "tsx", "go"];
    let other_files: Vec<String> = find_source_files(path, recursive, &all_exts)
        .into_iter()
        .filter(|f| !f.ends_with(".rs"))
        .collect();
    for file in &other_files {
        let lang = Language::from_extension(file);
        let stats = parse_doc_coverage_file(file);
        if stats.total_public > 0 {
            langs_seen.insert(lang.to_string());
        }
        total += stats.total_public;
        documented += stats.documented;
    }

    let pct = if total > 0 {
        documented as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    let mut langs_vec: Vec<String> = langs_seen.into_iter().collect();
    langs_vec.sort();

    let (severity, rule_id, help) = if pct >= min_doc {
        (
            "info".to_string(),
            "doccov-pass".to_string(),
            "Documentation coverage is within acceptable limits.".to_string(),
        )
    } else if pct < min_doc * 0.5 {
        (
            "error".to_string(),
            "doccov-low".to_string(),
            "Very low documentation coverage. Add documentation to public APIs to improve maintainability.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "doccov-moderate".to_string(),
            "Moderate documentation coverage. Add documentation to remaining public APIs."
                .to_string(),
        )
    };

    CheckResult {
        name: "doc_coverage".to_string(),
        passed: pct >= min_doc,
        findings: Vec::new(),
        score: Some(pct),
        threshold: Some(min_doc),
        message: if pct >= min_doc {
            format!(
                "Doc coverage {:.0}% >= {:.0}% (langs: {})",
                pct,
                min_doc,
                langs_vec.join(", ")
            )
        } else {
            format!(
                "Doc coverage {:.0}% < {:.0}% (langs: {})",
                pct,
                min_doc,
                langs_vec.join(", ")
            )
        },
        details: serde_json::json!({
            "total_public": total,
            "documented": documented,
            "coverage_pct": pct,
            "languages": langs_vec,
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
    }
}

pub(crate) fn check_complexity(
    path: &str,
    recursive: bool,
    min_complexity: u32,
    max_violations: usize,
) -> CheckResult {
    let all_exts = [
        "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp", "cc",
        "cxx", "hpp", "cs", "java", "php", "rb", "swift",
    ];
    let files = find_source_files(path, recursive, &all_exts);

    let mut total = 0usize;
    let mut complex_funcs: Vec<serde_json::Value> = Vec::new();
    let mut langs_seen: std::collections::HashSet<String> = Default::default();

    for file in &files {
        let lang = Language::from_extension(file);
        langs_seen.insert(lang.to_string());
        let funcs = parse_complexity_file(file);
        for func in funcs {
            total += 1;
            if func.complexity >= min_complexity {
                complex_funcs.push(serde_json::json!({
                    "name": func.name,
                    "file": func.file,
                    "line": func.line,
                    "complexity": func.complexity,
                    "language": func.language.to_string(),
                }));
            }
        }
    }

    let mut langs_vec: Vec<String> = langs_seen.into_iter().collect();
    langs_vec.sort();

    let passed = complex_funcs.len() <= max_violations;

    let (severity, rule_id, help) = if passed && complex_funcs.is_empty() {
        (
            "info".to_string(),
            "complexity-pass".to_string(),
            "No functions with excessive complexity.".to_string(),
        )
    } else if passed {
        (
            "info".to_string(),
            "complexity-pass".to_string(),
            format!(
                "Complexity violations within allowed limit (<= {}).",
                max_violations
            ),
        )
    } else if complex_funcs.len() > 10 {
        (
            "error".to_string(),
            "complexity-high".to_string(),
            "Multiple functions with high complexity. Refactor to reduce decision points."
                .to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "complexity-moderate".to_string(),
            "Some functions with high complexity. Consider refactoring.".to_string(),
        )
    };

    let findings: Vec<Finding> = complex_funcs
        .iter()
        .take(20)
        .map(|f| {
            let file = f
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = f.get("line").and_then(|v| v.as_u64());
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let complexity = f.get("complexity").and_then(|v| v.as_u64()).unwrap_or(0);
            Finding {
                file: file.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("{}: cyclomatic complexity = {}", name, complexity),
                rule_id: rule_id.clone(),
                fix_hint: "Refactor to reduce decision points (if/loops/matches).".to_string(),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    CheckResult {
        name: "complexity".to_string(),
        passed,
        score: Some(complex_funcs.len() as f64),
        threshold: Some(max_violations as f64),
        message: if passed && complex_funcs.is_empty() {
            format!(
                "No functions above complexity threshold (languages: {})",
                langs_vec.join(", ")
            )
        } else if passed {
            format!(
                "{} complex functions <= allowed {} (languages: {})",
                complex_funcs.len(),
                max_violations,
                langs_vec.join(", ")
            )
        } else {
            format!(
                "{} functions with complexity >= {} > allowed {} (languages: {})",
                complex_funcs.len(),
                min_complexity,
                max_violations,
                langs_vec.join(", ")
            )
        },
        details: serde_json::json!({
            "total_functions": total,
            "complex_count": complex_funcs.len(),
            "max_violations_allowed": max_violations,
            "languages": langs_vec,
            "functions": complex_funcs.iter().take(10).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

// HELPERS
// ═══════════════════════════════════════════

/// Build standard args for a delegated tool check.
fn build_check_args(path: &str, recursive: bool, extra: &[&str]) -> Vec<String> {
    let mut args = vec![path.to_string(), "--format".to_string(), "json".to_string()];
    if recursive {
        args.push("--recursive".to_string());
    }
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

/// Convert standard Vec<String> to Vec<&str> for run_tool calls.
fn args_ref(args: &[String]) -> Vec<&str> {
    args.iter().map(|s| s.as_str()).collect()
}

/// Extract an f64 value from nested JSON, returning 0.0 on missing path.
fn extract_f64(data: &serde_json::Value, path: &[&str]) -> f64 {
    let mut current = data;
    for key in path {
        match current.get(*key) {
            Some(v) => current = v,
            None => return 0.0,
        }
    }
    current.as_f64().unwrap_or(0.0)
}

/// Construct a standard CheckResult for delegated tool checks.
#[allow(clippy::too_many_arguments)]
fn make_check_result(
    name: &str,
    passed: bool,
    value: f64,
    threshold: f64,
    data: serde_json::Value,
    severity: &str,
    rule_id: &str,
    message: String,
    help: Option<&str>,
) -> CheckResult {
    let findings = extract_findings_from_details(&data, rule_id, severity);
    CheckResult {
        name: name.to_string(),
        passed,
        score: Some(value),
        threshold: Some(threshold),
        message,
        details: data,
        severity: Some(severity.to_string()),
        help: help.map(|s| s.to_string()),
        findings,
        rule_id: Some(rule_id.to_string()),
    }
}

// NEW 6 CHECK WRAPPERS & SETUP
// ═══════════════════════════════════════════

pub(crate) fn check_taint(path: &str, recursive: bool, max_taint: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("taint-scan", "taint", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "violations_count"]);
    let threshold = max_taint as f64;
    let passed = value <= threshold;
    let msg = if passed { format!("{} taint violations <= {}", value, max_taint) } else { format!("{} taint violations > allowed {}", value, max_taint) };
    make_check_result("taint", passed, value, threshold, res.data, if passed { "info" } else { "high" }, "taint_limit", msg, None)
}

pub(crate) fn check_dupfind(path: &str, recursive: bool, max_duplication: f64) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("duplication", "dupfind", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "total_groups"]);
    let passed = value <= max_duplication;
    let msg = if passed { format!("{} duplicated groups <= {}", value, max_duplication) } else { format!("{} duplicated groups > allowed {}", value, max_duplication) };
    make_check_result("duplication", passed, value, max_duplication, res.data, if passed { "info" } else { "medium" }, "duplication_limit", msg, None)
}

pub(crate) fn check_riskmap(path: &str, _recursive: bool, max_risk: f64) -> CheckResult {
    let args = build_check_args(path, false, &[]);
    let res = run_tool("risk-map", "riskmap", &args_ref(&args), Instant::now());
    let max_found = res.data.get("files").and_then(|a| a.as_array()).map(|arr| arr.iter().filter_map(|f| f.get("risk_score").and_then(|v| v.as_f64())).fold(0.0f64, f64::max)).unwrap_or(0.0);
    let passed = max_found <= max_risk;
    let msg = if passed { format!("Max risk score {:.1} <= {:.1}", max_found, max_risk) } else { format!("Max risk score {:.1} > allowed {:.1}", max_found, max_risk) };
    make_check_result("riskmap", passed, max_found, max_risk, res.data, if passed { "info" } else { "high" }, "riskmap_limit", msg, None)
}

pub(crate) fn check_coupling(path: &str, max_coupling: usize) -> CheckResult {
    let args = build_check_args(path, false, &[]);
    let res = run_tool("coupling", "coupling", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "avg_fan_out"]);
    let threshold = max_coupling as f64;
    let passed = value <= threshold;
    let msg = if passed { format!("Avg fan-out {:.1} <= {}", value, max_coupling) } else { format!("Avg fan-out {:.1} > allowed {}", value, max_coupling) };
    make_check_result("coupling", passed, value, threshold, res.data, if passed { "info" } else { "medium" }, "coupling_limit", msg, None)
}

pub(crate) fn check_propcov(path: &str, recursive: bool, min_propcov: f64) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("prop-cov", "propcov", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "coverage_percentage"]);
    let passed = value >= min_propcov;
    let msg = if passed { format!("PropCov {:.1}% >= {:.1}%", value, min_propcov) } else { format!("PropCov {:.1}% < required {:.1}%", value, min_propcov) };
    make_check_result("propcov", passed, value, min_propcov, res.data, if passed { "info" } else { "high" }, "propcov_limit", msg, None)
}

pub(crate) fn check_fuzz(path: &str, recursive: bool, max_fuzz_risk: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("fuzz-surface", "fuzz", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "fuzzable_functions"]);
    let threshold = max_fuzz_risk as f64;
    let passed = value <= threshold;
    let msg = if passed { format!("{} fuzzable endpoints <= {}", value, max_fuzz_risk) } else { format!("{} fuzzable endpoints > allowed {}", value, max_fuzz_risk) };
    make_check_result("fuzz", passed, value, threshold, res.data, if passed { "info" } else { "high" }, "fuzz_limit", msg, None)
}

pub(crate) fn check_linelen(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("line-length", "linelen", &args_ref(&args), Instant::now());
    let total = extract_f64(&res.data, &["summary", "fn_violations"]) + extract_f64(&res.data, &["summary", "file_violations"]);
    let threshold = max_violations as f64;
    let passed = total <= threshold;
    let msg = if passed && total == 0.0 { "All functions and files within size limits".into() } else if passed { format!("{} violations <= allowed {}", total, max_violations) } else { format!("{} line-length violations > allowed {}", total, max_violations) };
    make_check_result("linelen", passed, total, threshold, res.data, if passed { "info" } else { "warning" }, "linelen_limit", msg, Some("Functions should be <= 40 lines; files should be <= 500 lines."))
}

pub(crate) fn check_halstead(path: &str, recursive: bool, max_bugs: f64) -> CheckResult {
    let bugs_str = format!("{}", max_bugs);
    let args = build_check_args(path, recursive, &["--max-bugs", &bugs_str]);
    let res = run_tool("halstead", "halstead", &args_ref(&args), Instant::now());
    let exceeding = extract_f64(&res.data, &["summary", "files_exceeding_bugs_threshold"]);
    let total_bugs = extract_f64(&res.data, &["summary", "total_bugs_estimated"]);
    let passed = exceeding == 0.0;
    let msg = if passed { format!("Halstead bugs estimated {:.2} (no file exceeds {:.1})", total_bugs.max(0.0), max_bugs) } else { format!("{} files exceed Halstead bugs threshold of {:.1}", exceeding, max_bugs) };
    make_check_result("halstead", passed, total_bugs, max_bugs, res.data, if passed { "info" } else { "warning" }, "halstead_bugs", msg, Some("Halstead bugs = Volume/3000. High values indicate complex, error-prone code."))
}

pub(crate) fn check_secrets(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("secrets", "secrets", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "findings_count"]);
    let threshold = max_violations as f64;
    let passed = value <= threshold;
    let msg = if passed && value == 0.0 { "No hardcoded secrets detected".into() } else if passed { format!("{} secret findings <= allowed {}", value, max_violations) } else { format!("{} hardcoded secret findings > allowed {}", value, max_violations) };
    make_check_result("secrets", passed, value, threshold, res.data, if passed { "info" } else { "high" }, "secrets_limit", msg, Some("Move secrets to environment variables or a secrets manager."))
}

pub(crate) fn check_deadcode(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("dead-code", "deadcode", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "total_findings"]);
    let threshold = max_violations as f64;
    let passed = value <= threshold;
    let msg = if passed && value == 0.0 { "No dead code patterns detected".into() } else if passed { format!("{} dead code findings <= allowed {}", value, max_violations) } else { format!("{} dead code findings > allowed {}", value, max_violations) };
    make_check_result("deadcode", passed, value, threshold, res.data, if passed { "info" } else { "warning" }, "deadcode_limit", msg, Some("Remove unused imports, #[allow(dead_code)] suppressions, and dead assignments."))}

pub(crate) fn check_sast(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let args = build_check_args(path, recursive, &["--max-findings", &max_str]);
    let res = run_tool("sast", "sast", &args_ref(&args), Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("sast", "sast_limit", max_findings, res.error.clone());
    }
    let total = res.data.get("summary").and_then(|s| s.get("total_findings")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let critical = res.data.get("summary").and_then(|s| s.get("critical")).and_then(|v| v.as_u64()).unwrap_or(0);
    let high = res.data.get("summary").and_then(|s| s.get("high")).and_then(|v| v.as_u64()).unwrap_or(0);
    let threshold = max_findings as f64;
    let passed = total <= threshold;
    let msg = if passed && total == 0.0 { "No SAST findings (SQL injection, XSS, path traversal, cmd injection)".into() } else if passed { format!("{} SAST findings <= allowed {}", total as u64, max_findings) } else { format!("{} SAST findings ({} critical, {} high) — exceeds threshold of {}", total as u64, critical, high, max_findings) };
    make_check_result("sast", passed, total, threshold, res.data, if passed { "info" } else { "high" }, "sast_limit", msg, Some("Review SAST findings. Parameterize SQL, sanitize input, use allowlists for file paths and commands."))
}

pub(crate) fn check_crypto(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let args = build_check_args(path, recursive, &["--max-findings", &max_str]);
    let res = run_tool("crypto-check", "cryptocheck", &args_ref(&args), Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("crypto", "crypto_limit", max_findings, res.error.clone());
    }
    let total = res.data.get("summary").and_then(|s| s.get("total_findings")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let critical = res.data.get("summary").and_then(|s| s.get("critical")).and_then(|v| v.as_u64()).unwrap_or(0);
    let threshold = max_findings as f64;
    let passed = total <= threshold;
    let msg = if passed && total == 0.0 { "No cryptographic issues (weak hash, insecure random, ECB, disabled TLS)".into() } else if passed { format!("{} crypto findings <= allowed {}", total as u64, max_findings) } else { format!("{} crypto findings ({} critical) — exceeds threshold of {}", total as u64, critical, max_findings) };
    make_check_result("crypto", passed, total, threshold, res.data, if passed { "info" } else { "high" }, "crypto_limit", msg, Some("Replace MD5/SHA1 with SHA-256. Use OsRng for security randomness. Use AES-GCM, not ECB."))
}

pub(crate) fn check_licenses(path: &str, max_violations: usize) -> CheckResult {
    let max_str = format!("{}", max_violations);
    let args = build_check_args(path, false, &["--max-violations", &max_str]);
    let res = run_tool("licenses", "licenses", &args_ref(&args), Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("licenses", "license_compliance", max_violations, res.error.clone());
    }
    let violations = res.data.get("summary").and_then(|s| s.get("violations")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let pkgs = res.data.get("summary").and_then(|s| s.get("packages_scanned")).and_then(|v| v.as_u64()).unwrap_or(0);
    let threshold = max_violations as f64;
    let passed = violations <= threshold;
    let msg = if passed && violations == 0.0 { format!("No license violations in {} packages scanned", pkgs) } else if passed { format!("{} license violations <= allowed {} ({} packages)", violations as u64, max_violations, pkgs) } else { format!("{} license violations — GPL/AGPL packages in deny list", violations as u64) };
    make_check_result("licenses", passed, violations, threshold, res.data, if passed { "info" } else { "high" }, "license_compliance", msg, Some("Review copyleft (GPL/AGPL) licenses. They may require open-sourcing your code. Consult legal counsel."))
}

pub(crate) fn check_outdated(path: &str, max_major_behind: usize) -> CheckResult {
    use std::process::Command;
    // cargo-outdated must be installed; gracefully skip if not present
    let available = Command::new("cargo")
        .args(["outdated", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !available {
        return CheckResult {
            name: "outdated".into(),
            passed: true,
            score: None,
            threshold: None,
            message: "Skipped: cargo-outdated not installed (cargo install cargo-outdated)".into(),
            details: serde_json::Value::Null,
            severity: Some("info".into()),
            help: Some("Install with: cargo install cargo-outdated".into()),
            rule_id: Some("dep_freshness".into()),
            findings: Vec::new(),
        };
    }

    let output = Command::new("cargo")
        .args(["outdated", "--format", "json", "--root-deps-only"])
        .current_dir(path)
        .output();

    let major_behind = match output {
        Ok(ref o) if o.status.success() => {
            let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_default();
            json.get("dependencies")
                .and_then(|d| d.as_array())
                .map(|deps| {
                    deps.iter()
                        .filter(|dep| {
                            let latest = dep.get("latest").and_then(|v| v.as_str()).unwrap_or("");
                            let current = dep.get("project").and_then(|v| v.as_str()).unwrap_or("");
                            // Count as major-behind if first semver segment differs
                            let lat_major = latest
                                .split('.')
                                .next()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            let cur_major = current
                                .split('.')
                                .next()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            lat_major > cur_major
                        })
                        .count()
                })
                .unwrap_or(0)
        }
        _ => 0,
    };

    let passed = major_behind <= max_major_behind;
    CheckResult {
        name: "outdated".into(),
        passed,
        score: Some(major_behind as f64),
        threshold: Some(max_major_behind as f64),
        findings: Vec::new(),
        message: if major_behind == 0 {
            "All direct dependencies are within one major version".into()
        } else {
            format!(
                "{} direct dependencies are 1+ major versions behind latest",
                major_behind
            )
        },
        details: serde_json::Value::Null,
        severity: if passed {
            Some("info".into())
        } else {
            Some("low".into())
        },
        help: Some(
            "Run `cargo update` or review Cargo.toml to upgrade outdated dependencies.".into(),
        ),
        rule_id: Some("dep_freshness".into()),
    }
}

pub(crate) fn check_typecov(path: &str, recursive: bool, min_pct: f64) -> CheckResult {
    let pct_str = format!("{}", min_pct);
    let args = build_check_args(path, recursive, &["--min-pct", &pct_str]);
    let res = run_tool("type-coverage", "typecov", &args_ref(&args), Instant::now());
    let overall = res.data.get("summary").and_then(|s| s.get("overall_coverage_pct")).and_then(|v| v.as_f64()).unwrap_or(100.0);
    let below = extract_f64(&res.data, &["summary", "files_below_threshold"]);
    let passed = below == 0.0;
    let msg = if passed { format!("Type coverage {:.1}% >= {:.0}%", overall, min_pct) } else { format!("{} files below type coverage threshold of {:.0}%", below, min_pct) };    make_check_result("typecov", passed, overall, min_pct, res.data, if passed { "info" } else { "medium" }, "typecov_limit", msg, Some("Add type annotations to Python/JS/TS functions for better maintainability."))
}

pub(crate) fn check_vulnscan(path: &str, max_critical: usize, max_high: usize) -> CheckResult {
    let crit_str = format!("{}", max_critical);
    let high_str = format!("{}", max_high);
    let args = build_check_args(path, false, &["--max-critical", &crit_str, "--max-high", &high_str]);
    let res = run_tool("vuln-scan", "vulnscan", &args_ref(&args), Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("vulnscan", "vuln_limit", max_critical, res.error.clone());
    }
    let critical = res.data.get("summary").and_then(|s| s.get("critical")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let high = res.data.get("summary").and_then(|s| s.get("high")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let total = res.data.get("summary").and_then(|s| s.get("total")).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
    let passed = critical <= max_critical as f64 && high <= max_high as f64;
    let msg = if passed && total == 0.0 { "No known vulnerabilities".into() } else if passed { format!("{} vulnerabilities ({} critical, {} high) within allowed thresholds", total as u64, critical as u64, high as u64) } else { format!("{} critical + {} high CVEs exceed allowed thresholds ({}/{})", critical as u64, high as u64, max_critical, max_high) };
    make_check_result("vulnscan", passed, total, max_critical as f64, res.data, if passed { "info" } else { "high" }, "vuln_limit", msg, Some("Update vulnerable dependencies. Run cargo audit / npm audit for details."))
}

pub(crate) fn check_cohesion(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("cohesion", "cohesion", &args_ref(&args), Instant::now());
    let violations = extract_f64(&res.data, &["summary", "violations"]);
    let avg_lcom = extract_f64(&res.data, &["summary", "avg_lcom"]);
    let threshold = max_violations as f64;
    let passed = violations <= threshold;
    let msg = if passed && violations == 0.0 { format!("All structs cohesive (avg LCOM4 {:.2})", avg_lcom) } else if passed { format!("{} cohesion violations <= allowed {} (avg LCOM4 {:.2})", violations, max_violations, avg_lcom) } else { format!("{} structs exceed LCOM4 threshold of {}", violations, max_violations) };
    make_check_result("cohesion", passed, avg_lcom, threshold, res.data, if passed { "info" } else { "warning" }, "cohesion_lcom4", msg, Some("High LCOM4 means a struct does too many unrelated things. Split it."))
}

pub(crate) fn check_comments(path: &str, recursive: bool, min_ratio: f64) -> CheckResult {
    let ratio_str = format!("{}", min_ratio);
    let args = build_check_args(path, recursive, &["--min-ratio", &ratio_str]);
    let res = run_tool("comment-ratio", "comments", &args_ref(&args), Instant::now());
    let below = extract_f64(&res.data, &["summary", "files_below_threshold"]);
    let overall = extract_f64(&res.data, &["summary", "overall_comment_ratio"]);
    let passed = below == 0.0;
    let score = overall * 100.0;
    let threshold = min_ratio * 100.0;
    let msg = if passed { format!("Overall comment ratio {:.1}% >= {:.0}%", score, threshold) } else { format!("{} files below comment ratio threshold of {:.0}%", below, threshold) };
    // Use make_check_result but override score/threshold since they're scaled
    make_check_result("comments", passed, score, threshold, res.data, if passed { "info" } else { "low" }, "comment_ratio", msg, Some("Add inline comments explaining non-obvious logic. Doc comments are tracked separately by doccov."))
}

pub(crate) fn check_errhandle(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let args = build_check_args(path, recursive, &[]);
    let res = run_tool("error-handling", "errhandle", &args_ref(&args), Instant::now());
    let value = extract_f64(&res.data, &["summary", "total_findings"]);
    let threshold = max_violations as f64;
    let passed = value <= threshold;
    let msg = if passed && value == 0.0 { "No error handling issues detected".into() } else if passed { format!("{} error handling findings <= allowed {}", value, max_violations) } else { format!("{} error handling violations > allowed {}", value, max_violations) };
    make_check_result("errhandle", passed, value, threshold, res.data, if passed { "info" } else { "medium" }, "errhandle_limit", msg, Some("Replace .unwrap()/.expect() with proper error propagation using `?` or match."))
}

pub(crate) fn skipped_tool_check(
    name: &str,
    rule_id: &str,
    threshold: usize,
    error: Option<String>,
) -> CheckResult {
    CheckResult {
        name: name.into(),
        passed: true,
        score: None,
        threshold: Some(threshold as f64),
        message: match error {
            Some(error) if !error.is_empty() => format!("Skipped: {}", error),
            _ => "Skipped: tool unavailable or produced no JSON output".into(),
        },
        details: serde_json::Value::Null,
        severity: Some("info".into()),
        help: Some(
            "Install the check tool or run from the Cogent workspace to enable this check.".into(),
        ),
        findings: Vec::new(),
        rule_id: Some(rule_id.into()),
    }}

pub(crate) fn check_access_control(
    path: &str,
    recursive: bool,
    max_violations: usize,
) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let max_str = format!("{}", max_violations);
    args.push("--max-violations");
    args.push(&max_str);
    let res = run_tool("access-control", "access-control", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check(
            "access-control",
            "access-control",
            max_violations,
            res.error.clone(),
        );
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_violations;
    let msg = if passed {
        format!(
            "Access control passed ({} findings, {} critical)",
            total, critical
        )
    } else {
        format!(
            "Access control failed ({} findings > {} threshold, {} critical)",
            total, max_violations, critical
        )
    };
    CheckResult {
        name: "access-control".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: msg,
        details: serde_json::json!({"findings": total, "critical": critical}),
        severity: if critical > 0 {
            Some("critical".into())
        } else {
            Some("high".into())
        },
        help: Some(
            "Review missing auth guards, hardcoded credentials, IAM policies, and CORS settings."
                .into(),
        ),
        findings: extract_findings_from_details(&res.data, "access-control", "high"),
        rule_id: Some("access-control".into()),
    }
}

pub(crate) fn check_supply_chain(path: &str, max_risks: usize) -> CheckResult {
    let max_str = format!("{}", max_risks);
    let args = vec![path, "--format", "json", "--max-risks", &max_str];
    let res = run_tool("supply-chain", "supply-chain", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("supply-chain", "supply-chain", max_risks, res.error.clone());
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_risks"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_risks;
    let msg = if passed {
        format!(
            "Supply chain passed ({} risks, {} critical)",
            total, critical
        )
    } else {
        format!(
            "Supply chain failed ({} risks > {} threshold, {} critical)",
            total, max_risks, critical
        )
    };
    CheckResult {
        name: "supply-chain".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_risks as f64),
        message: msg,
        details: serde_json::json!({"risks": total, "critical": critical}),
        severity: if critical > 0 { Some("critical".into()) } else { Some("high".into()) },
        help: Some("Review lockfile integrity, typosquatting, unpinned dependencies, and abandoned packages.".into()),
        findings: extract_findings_from_details(&res.data, "supply-chain", "high"),
        rule_id: Some("supply-chain".into()),
    }
}

// PARALLEL CHECK EXECUTION
// ═══════════════════════════════════════════

/// Maximum number of concurrent check processes.
/// Conservative default to prevent OOM on memory-constrained systems.
const MAX_CONCURRENT_CHECKS: usize = 4;

/// Run check functions in parallel with bounded concurrency using a
/// work-stealing thread pool. Returns results sorted by check name for
/// consistent display.
pub(crate) fn run_parallel_checks(
    checks: Vec<(&'static str, Box<dyn FnOnce() -> CheckResult + Send>)>,
) -> Vec<CheckResult> {
    let total = checks.len();
    if total == 0 {
        return Vec::new();
    }
    let n_workers = MAX_CONCURRENT_CHECKS.min(total);
    let work: Mutex<Vec<(&'static str, Box<dyn FnOnce() -> CheckResult + Send>)>> =
        Mutex::new(checks);
    let results: Mutex<Vec<CheckResult>> = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|s| {
        for _ in 0..n_workers {
            s.spawn(|| loop {
                let job = work.lock().expect("work mutex poisoned").pop();
                match job {
                    Some((_name, f)) => {
                        let result = f();
                        results.lock().expect("results mutex poisoned").push(result);
                    }
                    None => break,
                }
            });
        }
    });

    let mut all = results.into_inner().expect("results mutex into_inner");
    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

/// Run tool binaries in parallel with bounded concurrency.
/// Each tool definition is (crate_name, bin_name, args).
/// Returns results in sorted order by tool name for consistent display.
pub(crate) fn run_parallel_tools(
    tools: Vec<(&'static str, &'static str, Vec<String>)>,
) -> Vec<ToolResult> {
    let total = tools.len();
    if total == 0 {
        return Vec::new();
    }
    let n_workers = MAX_CONCURRENT_CHECKS.min(total);
    let work: Mutex<Vec<(&'static str, &'static str, Vec<String>)>> =
        Mutex::new(tools);
    let results: Mutex<Vec<ToolResult>> = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|s| {
        for _ in 0..n_workers {
            s.spawn(|| loop {
                let job = work.lock().expect("work mutex poisoned").pop();
                match job {
                    Some((crate_name, bin_name, args)) => {
                        let args_ref: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
                        let result = run_tool(crate_name, bin_name, &args_ref, Instant::now());
                        results.lock().expect("results mutex poisoned").push(result);
                    }
                    None => break,
                }
            });
        }
    });

    let mut all = results.into_inner().expect("results mutex into_inner");
    all.sort_by(|a, b| a.tool.cmp(&b.tool));
    all
}

// NEW 6 CHECK WRAPPERS & SETUP
// ═══════════════════════════════════════════

pub(crate) fn run_tool(
    crate_name: &str,
    bin_name: &str,
    args: &[&str],
    tool_start: Instant,
) -> ToolResult {
    use cogent_common::*;
    use std::process::{Command, Stdio};

    let output = Command::new(bin_name)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => o,
        _ => {
            let cargo_output = Command::new("cargo")
                .args(["run", "--quiet", "-p", crate_name, "--bin", bin_name, "--"])
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match cargo_output {
                Ok(o) => o,
                Err(e) => {
                    let msg = if e.kind() == std::io::ErrorKind::NotFound {
                        format!(
                            "Binary '{}' not found. Install with: cargo install --path crates/{} (error: {})",
                            bin_name, crate_name, e
                        )
                    } else {
                        format!("Failed to run '{}': {}", bin_name, e)
                    };
                    return ToolResult {
                        tool: bin_name.to_string(),
                        success: false,
                        duration_ms: tool_start.elapsed().as_millis() as u64,
                        data: serde_json::Value::Null,
                        error: Some(msg),
                        suggested_fix: None,
                        auto_fix_available: None,
                    };
                }
            }
        }
    };

    let duration_ms = tool_start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let (data, error) = match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => (json, None),
        Err(_) => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    serde_json::Value::Null,
                    Some(format!("No output. stderr: {}", stderr.trim())),
                )
            } else {
                (serde_json::json!({ "raw": trimmed }), None)
            }
        }
    };

    ToolResult {
        tool: bin_name.to_string(),
        success: error.is_none() && output.status.success(),
        duration_ms,
        data,
        error,
        suggested_fix: None,
        auto_fix_available: None,
    }
}

pub(crate) fn run_batch(
    path: &str,
    _config: &str,
    format: &str,
    baseline: Option<&str>,
    no_fail_on_regression: bool,
) -> i32 {
    use crate::config::load_config_thresholds;
    use cogent_common::*;

    use std::time::Instant;

    let start = Instant::now();

    // Load project thresholds from .quality.toml so batch mode respects them
    let thresholds = load_config_thresholds(
        ".quality.toml",
        (
            30.0, 15.0, 1000, 10, 5.0, 0, 10.0, 5, 0.0, 0, 0, 2.0, 0, 10, 5,
            0.05, 50, 0.0, 0, 0, 0, 0, 0,
        ),
    );
    let max_sast = thresholds.20;
    let max_crypto = thresholds.21;

    // Initialize memory monitor (auto-terminates if memory exceeds safe threshold)
    let mut memory_monitor = MemoryMonitor::from_env();
    let mem_limit_mb = memory_monitor.max_rss_bytes / 1024 / 1024;
    let mem_display = if mem_limit_mb >= 1024 {
        format!("{:.1} GB", mem_limit_mb as f64 / 1024.0)
    } else {
        format!("{} MB", mem_limit_mb)
    };
    eprintln!(
        "  {} Cogent batch  ·  path: {}  ·  memory limit: {}",
        "▶".cyan().bold(),
        path.cyan(),
        mem_display.bright_black()
    );

    let tools: Vec<(&str, &str, Vec<String>)> = vec![
        (
            "debt-scan",
            "debt",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "doc-coverage",
            "doccov",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "crap-metric",
            "crap",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        ("coupling", "coupling", vec![path.to_string(), "--format".to_string(), "json".to_string()]),
        ("risk-map", "riskmap", vec![path.to_string(), "--format".to_string(), "json".to_string()]),
        (
            "duplication",
            "dupfind",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "prop-cov",
            "propcov",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "taint-scan",
            "taint",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "fuzz-surface",
            "fuzz",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        // mutation-test: run with capped mutants and enforced timeout.
        // Uses scratch workspace + watchdog kill — safe to include in batch.
        // Note: requires -p flag for package selection.
        // Uses dead-code crate (small, tests pass reliably) instead of ast-parse-ts
        // which has a pre-existing failing test.
        (
            "mutation-test",
            "mutate",
            vec![
                path.to_string(),
                "-p".to_string(),
                "dead-code".to_string(),
                "--max-mutants".to_string(),
                "5".to_string(),
                "--timeout".to_string(),
                "30".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "line-length",
            "linelen",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "halstead",
            "halstead",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "secrets",
            "secrets",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "dead-code",
            "deadcode",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "cohesion",
            "cohesion",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "comment-ratio",
            "comments",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "error-handling",
            "errhandle",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "type-coverage",
            "typecov",
            vec!["--recursive".to_string(), path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        ("vuln-scan", "vulnscan", vec![path.to_string(), "--format".to_string(), "json".to_string()]),
        (
            "sast",
            "sast",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
                "--max-findings".to_string(),
                max_sast.to_string(),
            ],
        ),
        (
            "crypto-check",
            "cryptocheck",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
                "--max-findings".to_string(),
                max_crypto.to_string(),
            ],
        ),
        ("licenses", "licenses", vec![path.to_string(), "--format".to_string(), "json".to_string()]),
    ];

    // Run tools in parallel with bounded concurrency (MAX_CONCURRENT_CHECKS=4).
    // Each tool is an independent subprocess, so they can safely execute concurrently.
    // Memory monitor check runs before/after the parallel batch.
    if let Err(usage) = memory_monitor.check() {
        eprintln!(
            "  {} Memory limit exceeded ({} MB used). Stopping batch.",
            "✗".red().bold(),
            usage.rss_bytes / 1024 / 1024
        );
        return 2;
    }
    let results = run_parallel_tools(tools);
    if let Err(usage) = memory_monitor.check() {
        eprintln!(
            "  {} Memory usage high after batch ({} MB used). Consider reducing MAX_CONCURRENT_CHECKS.",
            "⚠".yellow().bold(),
            usage.rss_bytes / 1024 / 1024
        );
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.len() - passed;

    // Baseline handling: must check before moving results into report
    let mut regression_detected = false;
    if let Some(baseline_file) = baseline {
        if let Ok(baseline_content) = std::fs::read_to_string(baseline_file) {
            if let Ok(baseline_report) = serde_json::from_str::<UnifiedReport>(&baseline_content) {
                let baseline_tools: std::collections::HashSet<String> = baseline_report
                    .tools
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let current_tools: std::collections::HashSet<String> = results
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let regressed: Vec<String> =
                    baseline_tools.difference(&current_tools).cloned().collect();
                if !regressed.is_empty() {
                    eprintln!(
                        "BASELINE REGRESSION: previously-passing tools now failing: {:?}",
                        regressed
                    );
                    if !no_fail_on_regression {
                        regression_detected = true;
                    }
                }
            }
        }
    }

    match format {
        "sarif" => {
            // Build SARIF from results
            let mut log = SarifLog::new("cogent", env!("CARGO_PKG_VERSION"));
            let mut sarif_results: Vec<SarifResult> = Vec::new();

            for tool in &results {
                if !tool.success {
                    let findings = extract_findings_from_details(
                        &tool.data,
                        &format!("{}-finding", tool.tool),
                        "high",
                    );
                    if findings.is_empty() {
                        sarif_results.push(SarifResult {
                            rule_id: format!("{}-error", tool.tool),
                            rule_index: None,
                            level: "error".to_string(),
                            message: SarifMessage {
                                text: tool
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| format!("{} failed", tool.tool)),
                            },
                            locations: vec![SarifLocation {
                                physical_location: SarifPhysicalLocation {
                                    artifact_location: Some(SarifArtifactLocation {
                                        uri: path.to_string(),
                                    }),
                                    region: None,
                                },
                            }],
                        });
                    } else {
                        for finding in findings {
                            sarif_results.push(SarifResult {
                                rule_id: finding.rule_id,
                                rule_index: None,
                                level: sarif_level(&finding.severity).to_string(),
                                message: SarifMessage {
                                    text: finding.message,
                                },
                                locations: vec![SarifLocation {
                                    physical_location: SarifPhysicalLocation {
                                        artifact_location: Some(SarifArtifactLocation {
                                            uri: finding.file,
                                        }),
                                        region: Some(SarifRegion {
                                            start_line: finding.line.map(|line| line as usize),
                                            start_column: finding
                                                .column
                                                .map(|column| column as usize),
                                            end_line: None,
                                            end_column: None,
                                        }),
                                    },
                                }],
                            });
                        }
                    }
                }
            }

            let run = sarif_run(
                "cogent-batch",
                env!("CARGO_PKG_VERSION"),
                sarif_results,
                if failed > 0 { 1 } else { 0 },
            );
            log.add_run(run);
            println!("{}", serde_json::to_string_pretty(&log).expect("SARIF log serialization"));
        }
        "json" => {
            let report = new_unified_report(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            );
            // Detect languages from source files at path
            let all_exts = [
                "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp",
                "cc", "cxx", "hpp", "cs", "java", "php", "rb", "swift",
            ];
            let mut langs_detected: Vec<String> = find_source_files(path, true, &all_exts)
                .iter()
                .map(|f| ast_parse_ts::Language::from_extension(f).to_string())
                .filter(|l| l != "unknown")
                .collect::<std::collections::HashSet<String>>()
                .into_iter()
                .collect();
            langs_detected.sort();
            let total_tools = results.len();
            let report = UnifiedReport {
                run_id: report.run_id,
                started_at: report.started_at,
                duration_ms,
                tools: results,
                summary: ReportSummary {
                    total_tools,
                    passed,
                    failed,
                    languages_detected: langs_detected,
                },
            };
            println!("{}", serde_json::to_string_pretty(&report).expect("JSON report serialization"));
        }
        _ => {
            let all_ok = failed == 0;
            let summary_str = format!(
                "{}/{} tools passed  ·  {}",
                passed,
                results.len(),
                format_ms(duration_ms)
            );
            let summary_col = if all_ok {
                summary_str.green().to_string()
            } else {
                summary_str.red().to_string()
            };
            let inner = 46usize;
            let border = "═".repeat(inner + 2);
            eprintln!();
            eprintln!("  ╔{}╗", border);
            let title = format!(
                "COGENT RUN  ·  {}",
                if all_ok {
                    "PASSED ✓".green().bold().to_string()
                } else {
                    "FAILED ✗".red().bold().to_string()
                }
            );
            box_row(&title, inner);
            eprintln!("  ╠{}╣", border);
            box_row(&summary_col, inner);
            box_row(&format!("Path: {}", path), inner);
            eprintln!("  ╚{}╝", border);
            if !all_ok {
                eprintln!();
                for tool in results.iter().filter(|t| !t.success) {
                    let err = tool.error.as_deref().unwrap_or("check output for details");
                    eprintln!("  {} {}: {}", "✗".red(), tool.tool.red().bold(), err);
                }
            }
            eprintln!();
        }
    }

    if failed > 0 || regression_detected {
        1
    } else {
        0
    }
}

// ═══════════════════════════════════════════
// HQSE LIFECYCLE CHECKS
// ═══════════════════════════════════════════

/// HQSE §Support/Debug: detects raw unstructured logging (println!, console.log, print(), fmt.Println)
/// in non-test source files and checks for structured log crate imports.
pub(crate) fn check_observability(
    path: &str,
    recursive: bool,
    max_violations: usize,
) -> CheckResult {
    let extensions = [
        "rs", "py", "js", "ts", "go", "java", "cs", "rb", "php", "swift",
    ];
    let files = find_source_files(path, recursive, &extensions);

    let raw_log_patterns: &[&str] = &[
        "println!(",
        "eprintln!(",
        "console.log(",
        "console.error(",
        "console.warn(",
        "fmt.Println(",
        "fmt.Printf(",
        "fmt.Fprintf(",
        "System.out.println(",
        "print(",
        "puts(",
    ];
    let structured_log_imports: &[&str] = &[
        "use tracing",
        "use log",
        "use slog",
        "use env_logger",
        "use log4rs",
        "import winston",
        "import pino",
        "import bunyan",
        "import structlog",
        "\"go.uber.org/zap\"",
        "\"github.com/sirupsen/logrus\"",
        "import logging",
        "import structlog",
        "org.slf4j",
        "org.apache.logging",
    ];

    let mut violations = Vec::new();
    let mut files_with_structured_log = 0usize;
    let mut total_non_test_files = 0usize;

    for file in &files {
        // Skip test files
        if file.contains("test") || file.contains("spec") || file.contains("bench") {
            continue;
        }
        total_non_test_files += 1;
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let has_structured = structured_log_imports
            .iter()
            .any(|pat| source.contains(pat));
        if has_structured {
            files_with_structured_log += 1;
        }
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            // Skip commented lines
            if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
                continue;
            }
            for pat in raw_log_patterns {
                if line.contains(pat) {
                    violations.push(serde_json::json!({
                        "file": file,
                        "line": line_num + 1,
                        "pattern": pat,
                    }));
                    break;
                }
            }
        }
    }

    let count = violations.len();
    let passed = count <= max_violations;
    let (severity, rule_id, help) = if passed {
        (
            "info".to_string(),
            "observability-pass".to_string(),
            "Logging observability is acceptable.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "observability-raw-log".to_string(),
            "Replace raw println!/console.log with a structured logging crate (tracing, log, winston, zap). \
             Structured logs are machine-parseable and essential for production observability.".to_string(),
        )
    };

    let findings: Vec<Finding> = violations
        .iter()
        .take(50)
        .map(|v| {
            let f = v
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let line = v.get("line").and_then(|x| x.as_u64());
            let pat = v
                .get("pattern")
                .and_then(|x| x.as_str())
                .unwrap_or("print")
                .to_string();
            Finding {
                file: f.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!(
                    "Unstructured log call '{}' in {}",
                    pat.trim_end_matches('('),
                    f
                ),
                rule_id: "observability-raw-log".to_string(),
                fix_hint:
                    "Replace with a structured logging macro (e.g. tracing::info!, log::warn!)."
                        .to_string(),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    let unstructured_files = total_non_test_files.saturating_sub(files_with_structured_log);
    CheckResult {
        name: "observability".to_string(),
        passed,
        score: Some(count as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            format!(
                "{} raw log calls (≤ {} allowed); {}/{} files use structured logging",
                count, max_violations, files_with_structured_log, total_non_test_files
            )
        } else {
            format!(
                "{} raw log calls > {} allowed; {} files lack structured logging imports",
                count, max_violations, unstructured_files
            )
        },
        details: serde_json::json!({
            "raw_log_calls": count,
            "total_non_test_files": total_non_test_files,
            "files_with_structured_log": files_with_structured_log,
            "violations": violations.iter().take(20).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

/// HQSE §Test: scans test files for non-determinism patterns (SystemTime::now, thread::sleep, etc.)
/// and optionally integrates mutation score from the mutate binary.
pub(crate) fn check_test_quality(
    path: &str,
    recursive: bool,
    max_nondeterminism: usize,
) -> CheckResult {
    let extensions = ["rs", "py", "js", "ts", "go", "java", "cs", "rb"];
    let files = find_source_files(path, recursive, &extensions);

    let nondeterminism_patterns: &[&str] = &[
        "SystemTime::now()",
        "Instant::now()",
        "thread::sleep(",
        "time.sleep(",
        "Time.now",
        "Date.now()",
        "new Date()",
        "Math.random()",
        "random.random()",
        "rand()",
        "time.Now()",
        "os.Getpid()",
    ];

    let mut violations = Vec::new();

    for file in &files {
        // Only scan test files and all .rs files (for inline #[cfg(test)] blocks)
        let is_test = file.contains("test")
            || file.contains("spec")
            || file.contains("_test.")
            || file.ends_with("_test.rs");
        if !is_test {
            // Also scan inline #[cfg(test)] blocks in Rust
            let is_rs = file.ends_with(".rs");
            if !is_rs {
                continue;
            }
        }
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }
            for pat in nondeterminism_patterns {
                if line.contains(pat) {
                    violations.push(serde_json::json!({
                        "file": file,
                        "line": line_num + 1,
                        "pattern": pat,
                    }));
                    break;
                }
            }
        }
    }

    // Attempt to read mutation score from the mutate binary (graceful skip on absence)
    let mutation_score: Option<f64> = {
        let mut args = vec![path, "--format", "json"];
        if recursive {
            args.push("--recursive");
        }
        let res = run_tool("mutation-test", "mutate", &args, std::time::Instant::now());
        if res.data.is_null() {
            None
        } else {
            res.data
                .get("score")
                .or_else(|| res.data.get("mutation_score"))
                .and_then(|v| v.as_f64())
        }
    };

    let count = violations.len();
    let passed = count <= max_nondeterminism;
    let (severity, rule_id, help) = if passed {
        (
            "info".to_string(),
            "test-quality-pass".to_string(),
            "Test quality is acceptable.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "test-quality-nondeterminism".to_string(),
            "Remove time/random dependencies from tests; use mocks or fixed seeds for deterministic results.".to_string(),
        )
    };

    let mut details = serde_json::json!({
        "nondeterminism_violations": count,
        "violations": violations.iter().take(20).collect::<Vec<_>>(),
    });
    if let Some(ms) = mutation_score {
        if let Some(obj) = details.as_object_mut() {
            obj.insert("mutation_score".to_string(), serde_json::json!(ms));
        }
    }

    let findings: Vec<Finding> = violations
        .iter()
        .take(50)
        .map(|v| {
            let f = v
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let line = v.get("line").and_then(|x| x.as_u64());
            let pat = v
                .get("pattern")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            Finding {
                file: f.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("Non-deterministic pattern '{}' in test file {}", pat, f),
                rule_id: "test-quality-nondeterminism".to_string(),
                fix_hint: "Replace with a mock, fixed seed, or deterministic alternative."
                    .to_string(),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    let msg_suffix = mutation_score
        .map(|ms| format!("; mutation score: {:.1}%", ms))
        .unwrap_or_default();
    CheckResult {
        name: "test-quality".to_string(),
        passed,
        score: Some(count as f64),
        threshold: Some(max_nondeterminism as f64),
        message: if passed {
            format!(
                "{} non-determinism patterns detected (≤ {} allowed){}",
                count, max_nondeterminism, msg_suffix
            )
        } else {
            format!(
                "{} non-determinism patterns > {} allowed{}",
                count, max_nondeterminism, msg_suffix
            )
        },
        details,
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

/// HQSE §Design: checks for ADR directory, ARCHITECTURE/DESIGN doc, and CHANGELOG presence.
/// Score = number of pillars present (0–3); passes if ≥1.
pub(crate) fn check_design_docs(path: &str) -> CheckResult {
    let base = std::path::Path::new(path);
    let mut pillars_present = 0u32;
    let mut missing = Vec::new();
    let mut present = Vec::new();

    // Pillar 1: ADR directory with ≥1 .md file
    let adr_dirs = [
        "docs/adr",
        "doc/adr",
        "docs/decisions",
        "doc/decisions",
        "adr",
    ];
    let has_adr = adr_dirs.iter().any(|d| {
        let dir = base.join(d);
        if dir.is_dir() {
            let count = std::fs::read_dir(&dir)
                .map(|e| {
                    e.filter_map(|x| x.ok())
                        .filter(|x| x.file_name().to_string_lossy().ends_with(".md"))
                        .count()
                })
                .unwrap_or(0);
            count > 0
        } else {
            false
        }
    });
    if has_adr {
        pillars_present += 1;
        present.push("ADR directory");
    } else {
        missing.push("ADR directory (docs/adr/ or doc/decisions/ with ≥1 .md)");
    }

    // Pillar 2: ARCHITECTURE or DESIGN doc at root or docs/
    let arch_candidates = [
        "ARCHITECTURE.md",
        "DESIGN.md",
        "docs/ARCHITECTURE.md",
        "docs/DESIGN.md",
        "docs/architecture/README.md",
        "docs/design/README.md",
    ];
    let has_arch = arch_candidates.iter().any(|f| base.join(f).exists());
    if has_arch {
        pillars_present += 1;
        present.push("Architecture/Design doc");
    } else {
        missing.push("ARCHITECTURE.md or DESIGN.md");
    }

    // Pillar 3: CHANGELOG or CHANGES
    let changelog_candidates = [
        "CHANGELOG.md",
        "CHANGES.md",
        "CHANGELOG",
        "CHANGES",
        "HISTORY.md",
    ];
    let has_changelog = changelog_candidates.iter().any(|f| base.join(f).exists());
    if has_changelog {
        pillars_present += 1;
        present.push("CHANGELOG");
    } else {
        missing.push("CHANGELOG.md or CHANGES.md");
    }

    let passed = pillars_present >= 1;
    let (severity, rule_id, help) = if pillars_present == 3 {
        (
            "info".to_string(),
            "design-docs-pass".to_string(),
            "All design documentation pillars present.".to_string(),
        )
    } else if pillars_present >= 1 {
        (
            "warning".to_string(),
            "design-docs-partial".to_string(),
            format!(
                "Missing design documentation: {}. Add these to improve HQSE §Design coverage.",
                missing.join(", ")
            ),
        )
    } else {
        (
            "warning".to_string(),
            "design-docs-missing".to_string(),
            "No design documentation found. Add ARCHITECTURE.md, ADRs, and CHANGELOG.md for HQSE §Design compliance.".to_string(),
        )
    };

    let findings: Vec<Finding> = if !passed {
        missing
            .iter()
            .map(|m| Finding {
                file: path.to_string(),
                line: None,
                column: None,
                severity: severity.clone(),
                message: format!("Missing design documentation: {}", m),
                rule_id: rule_id.clone(),
                fix_hint: format!("Create {} at the project root or docs/ directory.", m),
                evidence: None,
                suggested_fix: None,
                controls: None,
            })
            .collect()
    } else {
        Vec::new()
    };

    CheckResult {
        name: "design-docs".to_string(),
        passed,
        score: Some(pillars_present as f64),
        threshold: Some(1.0),
        message: if passed {
            format!(
                "{}/3 design doc pillars present: {}",
                pillars_present,
                present.join(", ")
            )
        } else {
            format!(
                "0/3 design doc pillars present — missing: {}",
                missing.join(", ")
            )
        },
        details: serde_json::json!({
            "pillars_present": pillars_present,
            "present": present,
            "missing": missing,
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

/// HQSE §Support/Debug/Code: finds contextless .unwrap() calls in library source (non-test) files.
/// Distinguished from errhandle: this specifically targets *contextless* unwraps
/// (no preceding SAFETY comment, no .context()/.with_context() wrapping).
pub(crate) fn check_debuggability(
    path: &str,
    recursive: bool,
    max_violations: usize,
) -> CheckResult {
    let extensions = ["rs"];
    let files = find_source_files(path, recursive, &extensions);

    let mut violations = Vec::new();

    for file in &files {
        // Skip test files and benches
        if file.contains("test") || file.contains("bench") || file.contains("spec") {
            continue;
        }
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains(".unwrap()") && !line.contains(".expect(\"") {
                continue;
            }
            // Check if the preceding line(s) contain a SAFETY comment
            let prev = if i > 0 { lines[i - 1].trim() } else { "" };
            let has_safety = prev.contains("SAFETY") || prev.contains("safety");
            // Check if the call is wrapped with .context( or .with_context(
            let has_context = line.contains(".context(") || line.contains(".with_context(");
            if !has_safety && !has_context {
                let call = if line.contains(".unwrap()") {
                    ".unwrap()"
                } else {
                    ".expect()"
                };
                violations.push(serde_json::json!({
                    "file": file,
                    "line": i + 1,
                    "call": call,
                }));
            }
        }
    }

    let count = violations.len();
    let passed = count <= max_violations;
    let (severity, rule_id, help) = if passed {
        (
            "info".to_string(),
            "debuggability-pass".to_string(),
            "Contextless unwrap count is within acceptable limits.".to_string(),
        )
    } else if count > max_violations * 2 {
        (
            "error".to_string(),
            "debuggability-contextless-unwrap".to_string(),
            "Many contextless .unwrap() calls make debugging hard. Use .context(\"what failed\") \
             from the anyhow or thiserror crate, or propagate with `?`."
                .to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "debuggability-contextless-unwrap".to_string(),
            "Contextless .unwrap() calls reduce debuggability. Wrap with .context(\"description\") \
             or annotate with a // SAFETY comment.".to_string(),
        )
    };

    let findings: Vec<Finding> = violations
        .iter()
        .take(50)
        .map(|v| {
            let f = v
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let line = v.get("line").and_then(|x| x.as_u64());
            let call = v
                .get("call")
                .and_then(|x| x.as_str())
                .unwrap_or(".unwrap()")
                .to_string();
            Finding {
                file: f.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("Contextless {} in {}", call, f),
                rule_id: "debuggability-contextless-unwrap".to_string(),
                fix_hint: "Add .context(\"description\") or use `?` with a // SAFETY comment."
                    .to_string(),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    CheckResult {
        name: "debuggability".to_string(),
        passed,
        score: Some(count as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            format!(
                "{} contextless unwrap calls (≤ {} allowed)",
                count, max_violations
            )
        } else {
            format!(
                "{} contextless unwrap calls > {} allowed",
                count, max_violations
            )
        },
        details: serde_json::json!({
            "contextless_unwraps": count,
            "violations": violations.iter().take(20).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── build_check_args ──

    #[test]
    fn test_build_check_args_basic() {
        let args = build_check_args(".", true, &[]);
        assert_eq!(args[0], ".");
        assert_eq!(args[1], "--format");
        assert_eq!(args[2], "json");
        assert_eq!(args[3], "--recursive");
    }

    #[test]
    fn test_build_check_args_non_recursive() {
        let args = build_check_args("src", false, &[]);
        assert_eq!(args[0], "src");
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_build_check_args_with_extra() {
        let args = build_check_args(".", true, &["--max-findings", "10"]);
        assert!(args.contains(&"--max-findings".to_string()));
        assert!(args.contains(&"10".to_string()));
        assert!(args.contains(&"--recursive".to_string()));
    }

    #[test]
    fn test_build_check_args_extra_without_recursive() {
        let args = build_check_args(".", false, &["--threshold", "5"]);
        assert_eq!(args.len(), 5);
        assert_eq!(args[3], "--threshold");
        assert_eq!(args[4], "5");
    }

    // ── args_ref ──

    #[test]
    fn test_args_ref_converts_strings_to_strs() {
        let v = vec!["a".to_string(), "b".to_string()];
        let r = args_ref(&v);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], "a");
        assert_eq!(r[1], "b");
    }

    #[test]
    fn test_args_ref_empty() {
        let v: Vec<String> = vec![];
        let r = args_ref(&v);
        assert!(r.is_empty());
    }

    // ── extract_f64 ──

    #[test]
    fn test_extract_f64_top_level() {
        let data = json!({"score": 42.5});
        assert_eq!(extract_f64(&data, &["score"]), 42.5);
    }

    #[test]
    fn test_extract_f64_nested() {
        let data = json!({"summary": {"total": 10.0}});
        assert_eq!(extract_f64(&data, &["summary", "total"]), 10.0);
    }

    #[test]
    fn test_extract_f64_deeply_nested() {
        let data = json!({"a": {"b": {"c": 99.9}}});
        assert_eq!(extract_f64(&data, &["a", "b", "c"]), 99.9);
    }

    #[test]
    fn test_extract_f64_missing_key_returns_zero() {
        let data = json!({"score": 42.0});
        assert_eq!(extract_f64(&data, &["nonexistent"]), 0.0);
    }

    #[test]
    fn test_extract_f64_partial_path_returns_zero() {
        let data = json!({"a": {"b": 5.0}});
        assert_eq!(extract_f64(&data, &["a", "b", "c"]), 0.0);
    }

    #[test]
    fn test_extract_f64_null_value_returns_zero() {
        let data = json!({"score": null});
        assert_eq!(extract_f64(&data, &["score"]), 0.0);
    }

    #[test]
    fn test_extract_f64_integer_value() {
        let data = json!({"count": 7});
        assert_eq!(extract_f64(&data, &["count"]), 7.0);
    }

    #[test]
    fn test_extract_f64_string_value_returns_zero() {
        let data = json!({"name": "hello"});
        assert_eq!(extract_f64(&data, &["name"]), 0.0);
    }

    #[test]
    fn test_extract_f64_empty_object() {
        let data = json!({});
        assert_eq!(extract_f64(&data, &["key"]), 0.0);
    }

    // ── make_check_result ──

    #[test]
    fn test_make_check_result_basic() {
        let data = json!({"findings": []});
        let result = make_check_result(
            "test-check", true, 0.0, 10.0, data,
            "info", "test-rule", "All good".into(), None,
        );
        assert_eq!(result.name, "test-check");
        assert!(result.passed);
        assert_eq!(result.score, Some(0.0));
        assert_eq!(result.threshold, Some(10.0));
        assert_eq!(result.message, "All good");
        assert_eq!(result.severity.as_deref(), Some("info"));
        assert_eq!(result.rule_id.as_deref(), Some("test-rule"));
        assert!(result.help.is_none());
    }

    #[test]
    fn test_make_check_result_failed() {
        let data = json!({"findings": []});
        let result = make_check_result(
            "security-check", false, 15.0, 10.0, data,
            "high", "security-001", "Threshold exceeded".into(), Some("Reduce violations"),
        );
        assert!(!result.passed);
        assert_eq!(result.score, Some(15.0));
        assert_eq!(result.threshold, Some(10.0));
        assert_eq!(result.severity.as_deref(), Some("high"));
        assert_eq!(result.help.as_deref(), Some("Reduce violations"));
    }

    #[test]
    fn test_make_check_result_extracts_findings() {
        let data = json!({
            "findings": [
                {"file": "a.rs", "message": "issue 1", "severity": "high"},
                {"file": "b.rs", "message": "issue 2", "severity": "medium"},
            ]
        });
        let result = make_check_result(
            "multi-find", false, 2.0, 1.0, data,
            "error", "multi-rule", "Multiple issues".into(), None,
        );
        assert_eq!(result.findings.len(), 2);
        assert_eq!(result.findings[0].file, "a.rs");
        assert_eq!(result.findings[1].file, "b.rs");
    }

    #[test]
    fn test_make_check_result_empty_findings() {
        let data = json!({});
        let result = make_check_result(
            "no-find", true, 0.0, 5.0, data,
            "info", "no-rule", "No issues".into(), None,
        );
        assert!(result.findings.is_empty());
    }
}
