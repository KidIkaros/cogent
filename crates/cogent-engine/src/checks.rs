#![deny(clippy::all)]

use ast_parse_ts::{parse_complexity_file, parse_doc_coverage_file, Language};
use cogent_common::{
    CheckResult, Finding,
    crap_score, function_coverage, parse_lcov, CoverageRecord, find_source_files,
};
use std::time::Instant;
use syn::visit::Visit;
use syn::{ImplItemFn, ItemEnum, ItemFn, ItemStruct, ItemTrait, Visibility};

/// Scan all source files under `path`, invoking `predicate` on each function.
/// Returns `(total_functions_count, collected_items)`.
fn scan_source_functions<T, F>(path: &str, recursive: bool, mut predicate: F) -> (usize, Vec<T>)
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

// ═══════════════════════════════════════════
// PHASE HELPERS — extract typed values from tool JSON output
// ═══════════════════════════════════════════

/// Extract a `u64` from `data.summary.field`, defaulting to `0`.
pub(crate) fn summary_u64(data: &serde_json::Value, field: &str) -> usize {
    data.get("summary")
        .and_then(|s| s.get(field))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize
}

/// Extract an `f64` from `data.summary.field`, defaulting to `0.0`.
#[allow(dead_code)]
pub(crate) fn summary_f64(data: &serde_json::Value, field: &str) -> f64 {
    data.get("summary")
        .and_then(|s| s.get(field))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Build a standard `CheckResult` for a tool-based count check.
///
/// This helper centralises the repeated pattern:
/// 1. Extract a count from JSON summary
/// 2. Compare against threshold
/// 3. Construct `CheckResult` with standard fields
#[allow(clippy::too_many_arguments, dead_code)]
fn check_result_from_count(
    name: &str,
    score: usize,
    threshold: usize,
    details: serde_json::Value,
    severity_pass: &str,
    severity_fail: &str,
    help: Option<&str>,
    rule_id: &str,
    findings_rule_id: &str,
    findings_severity: &str,
) -> CheckResult {
    let passed = score <= threshold;
    let findings = crate::extract_findings_from_details(&details, findings_rule_id, findings_severity);
    CheckResult {
        name: name.into(),
        passed,
        score: Some(score as f64),
        threshold: Some(threshold as f64),
        message: if passed {
            format!("{} findings <= {}", score, threshold)
        } else {
            format!("{} findings > allowed {}", score, threshold)
        },
        details,
        severity: Some(if passed { severity_pass.into() } else { severity_fail.into() }),
        help: help.map(|h| h.into()),
        findings,
        rule_id: Some(rule_id.into()),
    }
}

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

pub fn check_crap(
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
pub fn check_debt(path: &str, recursive: bool, max_debt: usize) -> CheckResult {
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
pub fn check_doc_coverage(path: &str, recursive: bool, min_doc: f64) -> CheckResult {
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
pub fn check_complexity(
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
pub fn check_access_control(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_access_control_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

/// [`check_access_control`] with an injectable [`ToolRunner`] for testing.
pub fn check_access_control_with_runner(
    path: &str,
    recursive: bool,
    max_violations: usize,
    runner: &dyn crate::ToolRunner,
) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let max_str = format!("{}", max_violations);
    args.push("--max-violations");
    args.push(&max_str);
    let res = crate::run_tool_with_runner(runner, "access-control", "access-control", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check(
            "access-control",
            "access-control",
            max_violations,
            res.error.clone(),
        );
    }
    let total = summary_u64(&res.data, "total_findings");
    let critical = summary_u64(&res.data, "critical");
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
        findings: crate::extract_findings_from_details(&res.data, "access-control", "high"),
        rule_id: Some("access-control".into()),
    }
}
pub fn check_supply_chain(path: &str, max_risks: usize) -> CheckResult {
    check_supply_chain_with_runner(path, max_risks, &crate::DefaultToolRunner)
}

/// [`check_supply_chain`] with an injectable [`ToolRunner`] for testing.
pub fn check_supply_chain_with_runner(
    path: &str,
    max_risks: usize,
    runner: &dyn crate::ToolRunner,
) -> CheckResult {
    let max_str = format!("{}", max_risks);
    let args = vec![path, "--format", "json", "--max-risks", &max_str];
    let res = crate::run_tool_with_runner(runner, "supply-chain", "supply-chain", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check("supply-chain", "supply-chain", max_risks, res.error.clone());
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
        findings: crate::extract_findings_from_details(&res.data, "supply-chain", "high"),
        rule_id: Some("supply-chain".into()),
    }
}
pub fn check_taint(path: &str, recursive: bool, max_taint: usize) -> CheckResult {
    check_taint_with_runner(path, recursive, max_taint, &crate::DefaultToolRunner)
}

pub fn check_taint_with_runner(path: &str, recursive: bool, max_taint: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "taint-scan", "taint", &args, Instant::now());
    let violations = summary_u64(&res.data, "violations_count");
    let passed = violations <= max_taint;
    CheckResult {
        name: "taint".into(),
        passed,
        score: Some(violations as f64),
        threshold: Some(max_taint as f64),
        message: if passed {
            format!("{} taint violations <= {}", violations, max_taint)
        } else {
            format!("{} taint violations > allowed {}", violations, max_taint)
        },
        details: res.data.clone(),
        severity: Some(if passed { "info" } else { "high" }.into()),
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "taint_limit", "high"),
        rule_id: Some("taint_limit".into()),
    }
}
pub fn check_dupfind(path: &str, recursive: bool, max_duplication: f64) -> CheckResult {
    check_dupfind_with_runner(path, recursive, max_duplication, &crate::DefaultToolRunner)
}

pub fn check_dupfind_with_runner(path: &str, recursive: bool, max_duplication: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "duplication", "dupfind", &args, Instant::now());
    let groups = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_groups"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = groups <= max_duplication;
    CheckResult {
        name: "duplication".into(),
        passed,
        score: Some(groups),
        threshold: Some(max_duplication),
        message: if passed {
            format!("{} duplicated groups <= {}", groups, max_duplication)
        } else {
            format!("{} duplicated groups > allowed {}", groups, max_duplication)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "duplication_limit", "medium"),
        rule_id: Some("duplication_limit".into()),
    }
}
pub fn check_riskmap(path: &str, _recursive: bool, max_risk: f64) -> CheckResult {
    check_riskmap_with_runner(path, _recursive, max_risk, &crate::DefaultToolRunner)
}

pub fn check_riskmap_with_runner(path: &str, _recursive: bool, max_risk: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let args = vec![path, "--format", "json"];
    let res = crate::run_tool_with_runner(runner, "risk-map", "riskmap", &args, Instant::now());
    let max_found_risk = res
        .data
        .get("files")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("risk_score").and_then(|v| v.as_f64()))
                .fold(0.0f64, f64::max)
        })
        .unwrap_or(0.0);
    let passed = max_found_risk <= max_risk;
    CheckResult {
        name: "riskmap".into(),
        passed,
        score: Some(max_found_risk),
        threshold: Some(max_risk),
        message: if passed {
            format!("Max risk score {:.1} <= {:.1}", max_found_risk, max_risk)
        } else {
            format!(
                "Max risk score {:.1} > allowed {:.1}",
                max_found_risk, max_risk
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "riskmap_limit", "high"),
        rule_id: Some("riskmap_limit".into()),
    }
}
pub fn check_coupling(path: &str, max_coupling: usize) -> CheckResult {
    check_coupling_with_runner(path, max_coupling, &crate::DefaultToolRunner)
}

pub fn check_coupling_with_runner(path: &str, max_coupling: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let args = vec![path, "--format", "json"];
    let res = crate::run_tool_with_runner(runner, "coupling", "coupling", &args, Instant::now());
    let avg_fan_out = res
        .data
        .get("summary")
        .and_then(|s| s.get("avg_fan_out"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = avg_fan_out <= max_coupling as f64;
    CheckResult {
        name: "coupling".into(),
        passed,
        score: Some(avg_fan_out),
        threshold: Some(max_coupling as f64),
        message: if passed {
            format!("Avg fan-out {:.1} <= {}", avg_fan_out, max_coupling)
        } else {
            format!("Avg fan-out {:.1} > allowed {}", avg_fan_out, max_coupling)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "coupling_limit", "medium"),
        rule_id: Some("coupling_limit".into()),
    }
}
pub fn check_propcov(path: &str, recursive: bool, min_propcov: f64) -> CheckResult {
    check_propcov_with_runner(path, recursive, min_propcov, &crate::DefaultToolRunner)
}

pub fn check_propcov_with_runner(path: &str, recursive: bool, min_propcov: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "prop-cov", "propcov", &args, Instant::now());
    let coverage = res
        .data
        .get("summary")
        .and_then(|s| s.get("coverage_percentage"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = coverage >= min_propcov;
    CheckResult {
        name: "propcov".into(),
        passed,
        score: Some(coverage),
        threshold: Some(min_propcov),
        message: if passed {
            format!("PropCov {:.1}% >= {:.1}%", coverage, min_propcov)
        } else {
            format!("PropCov {:.1}% < required {:.1}%", coverage, min_propcov)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "propcov_limit", "high"),
        rule_id: Some("propcov_limit".into()),
    }
}
pub fn check_fuzz(path: &str, recursive: bool, max_fuzz_risk: usize) -> CheckResult {
    check_fuzz_with_runner(path, recursive, max_fuzz_risk, &crate::DefaultToolRunner)
}

pub fn check_fuzz_with_runner(path: &str, recursive: bool, max_fuzz_risk: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "fuzz-surface", "fuzz", &args, Instant::now());
    let fuzzable = res
        .data
        .get("summary")
        .and_then(|s| s.get("fuzzable_functions"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = fuzzable <= max_fuzz_risk;
    CheckResult {
        name: "fuzz".into(),
        passed,
        score: Some(fuzzable as f64),
        threshold: Some(max_fuzz_risk as f64),
        message: if passed {
            format!("{} fuzzable endpoints <= {}", fuzzable, max_fuzz_risk)
        } else {
            format!(
                "{} fuzzable endpoints > allowed {}",
                fuzzable, max_fuzz_risk
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: crate::extract_findings_from_details(&res.data, "fuzz_limit", "high"),
        rule_id: Some("fuzz_limit".into()),
    }
}
pub fn check_linelen(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_linelen_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

pub fn check_linelen_with_runner(path: &str, recursive: bool, max_violations: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "line-length", "linelen", &args, Instant::now());
    let fn_viols = res
        .data
        .get("summary")
        .and_then(|s| s.get("fn_violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let file_viols = res
        .data
        .get("summary")
        .and_then(|s| s.get("file_violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = fn_viols + file_viols;
    let passed = total <= max_violations;
    CheckResult {
        name: "linelen".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if total == 0 {
                "All functions and files within size limits".to_string()
            } else {
                format!("{} violations <= allowed {}", total, max_violations)
            }
        } else {
            format!(
                "{} line-length violations > allowed {}",
                total, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some("Functions should be <= 40 lines; files should be <= 500 lines.".into()),
        findings: crate::extract_findings_from_details(&res.data, "linelen_limit", "warning"),
        rule_id: Some("linelen_limit".into()),
    }
}
pub fn check_halstead(path: &str, recursive: bool, max_bugs: f64) -> CheckResult {
    check_halstead_with_runner(path, recursive, max_bugs, &crate::DefaultToolRunner)
}

pub fn check_halstead_with_runner(path: &str, recursive: bool, max_bugs: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let max_bugs_str = format!("{}", max_bugs);
    let mut args = vec![path, "--format", "json", "--max-bugs", &max_bugs_str];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "halstead", "halstead", &args, Instant::now());
    let exceeding = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_exceeding_bugs_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total_bugs = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_bugs_estimated"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = exceeding == 0;
    CheckResult {
        name: "halstead".into(),
        passed,
        score: Some(total_bugs),
        threshold: Some(max_bugs),
        message: if passed {
            format!(
                "Halstead bugs estimated {:.2} (no file exceeds {:.1})",
                total_bugs.max(0.0),
                max_bugs
            )
        } else {
            format!(
                "{} files exceed Halstead bugs threshold of {:.1}",
                exceeding, max_bugs
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some(
            "Halstead bugs = Volume/3000. High values indicate complex, error-prone code.".into(),
        ),
        findings: crate::extract_findings_from_details(&res.data, "halstead_bugs", "warning"),
        rule_id: Some("halstead_bugs".into()),
    }
}
pub fn check_secrets(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_secrets_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

/// [`check_secrets`] with an injectable [`ToolRunner`] for testing.
pub fn check_secrets_with_runner(
    path: &str,
    recursive: bool,
    max_violations: usize,
    runner: &dyn crate::ToolRunner,
) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "secrets", "secrets", &args, Instant::now());
    let findings = summary_u64(&res.data, "findings_count");
    let passed = findings <= max_violations;
    CheckResult {
        name: "secrets".into(),
        passed,
        score: Some(findings as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if findings == 0 {
                "No hardcoded secrets detected".into()
            } else {
                format!("{} secret findings <= allowed {}", findings, max_violations)
            }
        } else {
            format!(
                "{} hardcoded secret findings > allowed {}",
                findings, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: Some("Move secrets to environment variables or a secrets manager.".into()),
        findings: crate::extract_findings_from_details(&res.data, "secrets_limit", "high"),
        rule_id: Some("secrets_limit".into()),
    }
}
pub fn check_deadcode(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_deadcode_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

pub fn check_deadcode_with_runner(path: &str, recursive: bool, max_violations: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "dead-code", "deadcode", &args, Instant::now());
    let findings = summary_u64(&res.data, "total_findings");
    let passed = findings <= max_violations;
    CheckResult {
        name: "deadcode".into(),
        passed,
        score: Some(findings as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if findings == 0 {
                "No dead code patterns detected".into()
            } else {
                format!(
                    "{} dead code findings <= allowed {}",
                    findings, max_violations
                )
            }
        } else {
            format!(
                "{} dead code findings > allowed {}",
                findings, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some(
            "Remove unused imports, #[allow(dead_code)] suppressions, and dead assignments.".into(),
        ),
        findings: crate::extract_findings_from_details(&res.data, "deadcode_limit", "warning"),
        rule_id: Some("deadcode_limit".into()),
    }
}
pub fn check_sast(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    check_sast_with_runner(path, recursive, max_findings, &crate::DefaultToolRunner)
}

pub fn check_sast_with_runner(path: &str, recursive: bool, max_findings: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "sast", "sast", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check("sast", "sast_limit", max_findings, res.error.clone());
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
    let high = res
        .data
        .get("summary")
        .and_then(|s| s.get("high"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_findings;
    CheckResult {
        name: "sast".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_findings as f64),
        message: if passed {
            if total == 0 { "No SAST findings (SQL injection, XSS, path traversal, cmd injection)".into() }
            else { format!("{} SAST findings <= allowed {}", total, max_findings) }
        } else {
            format!("{} SAST findings ({} critical, {} high) — exceeds threshold of {}", total, critical, high, max_findings)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Review SAST findings. Parameterize SQL, sanitize input, use allowlists for file paths and commands.".into()),
        findings: crate::extract_findings_from_details(&res.data, "sast_limit", "high"),
        rule_id: Some("sast_limit".into()),
    }
}
pub fn check_crypto(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    check_crypto_with_runner(path, recursive, max_findings, &crate::DefaultToolRunner)
}

pub fn check_crypto_with_runner(path: &str, recursive: bool, max_findings: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "crypto-check", "cryptocheck", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check("crypto", "crypto_limit", max_findings, res.error.clone());
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
    let passed = total <= max_findings;
    CheckResult {
        name: "crypto".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_findings as f64),
        message: if passed {
            if total == 0 { "No cryptographic issues (weak hash, insecure random, ECB, disabled TLS)".into() }
            else { format!("{} crypto findings <= allowed {}", total, max_findings) }
        } else {
            format!("{} crypto findings ({} critical) — exceeds threshold of {}", total, critical, max_findings)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Replace MD5/SHA1 with SHA-256. Use OsRng for security randomness. Use AES-GCM, not ECB.".into()),
        findings: crate::extract_findings_from_details(&res.data, "crypto_limit", "high"),
        rule_id: Some("crypto_limit".into()),
    }
}
pub fn check_licenses(path: &str, max_violations: usize) -> CheckResult {
    check_licenses_with_runner(path, max_violations, &crate::DefaultToolRunner)
}

pub fn check_licenses_with_runner(path: &str, max_violations: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let max_str = format!("{}", max_violations);
    let args = vec![path, "--format", "json", "--max-violations", &max_str];
    let res = crate::run_tool_with_runner(runner, "licenses", "licenses", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check(
            "licenses",
            "license_compliance",
            max_violations,
            res.error.clone(),
        );
    }
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("packages_scanned"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = violations <= max_violations;
    CheckResult {
        name: "licenses".into(),
        passed,
        score: Some(violations as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if violations == 0 { format!("No license violations in {} packages scanned", total) }
            else { format!("{} license violations <= allowed {} ({} packages)", violations, max_violations, total) }
        } else {
            format!("{} license violations — GPL/AGPL packages in deny list", violations)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Review copyleft (GPL/AGPL) licenses. They may require open-sourcing your code. Consult legal counsel.".into()),
        findings: crate::extract_findings_from_details(&res.data, "license_compliance", "high"),
        rule_id: Some("license_compliance".into()),
    }
}
pub fn check_outdated(path: &str, max_major_behind: usize) -> CheckResult {
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
pub fn check_typecov(path: &str, recursive: bool, min_pct: f64) -> CheckResult {
    check_typecov_with_runner(path, recursive, min_pct, &crate::DefaultToolRunner)
}

pub fn check_typecov_with_runner(path: &str, recursive: bool, min_pct: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let min_pct_str = format!("{}", min_pct);
    let mut args = vec![path, "--format", "json", "--min-pct", &min_pct_str];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "type-coverage", "typecov", &args, Instant::now());
    let overall = res
        .data
        .get("summary")
        .and_then(|s| s.get("overall_coverage_pct"))
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let below = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_below_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = below == 0;
    CheckResult {
        name: "typecov".into(),
        passed,
        score: Some(overall),
        threshold: Some(min_pct),
        message: if passed {
            format!("Type coverage {:.1}% >= {:.0}%", overall, min_pct)
        } else {
            format!(
                "{} files below type coverage threshold of {:.0}%",
                below, min_pct
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: Some(
            "Add type annotations to Python/JS/TS functions for better maintainability.".into(),
        ),
        findings: crate::extract_findings_from_details(&res.data, "typecov_limit", "medium"),
        rule_id: Some("typecov_limit".into()),
    }
}
pub fn check_vulnscan(path: &str, max_critical: usize, max_high: usize) -> CheckResult {
    check_vulnscan_with_runner(path, max_critical, max_high, &crate::DefaultToolRunner)
}

pub fn check_vulnscan_with_runner(path: &str, max_critical: usize, max_high: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let max_critical_str = format!("{}", max_critical);
    let max_high_str = format!("{}", max_high);
    let args = vec![
        path,
        "--format",
        "json",
        "--max-critical",
        &max_critical_str,
        "--max-high",
        &max_high_str,
    ];
    let res = crate::run_tool_with_runner(runner, "vuln-scan", "vulnscan", &args, Instant::now());
    if res.data.is_null() {
        return crate::skipped_tool_check("vulnscan", "vuln_limit", max_critical, res.error.clone());
    }
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let high = res
        .data
        .get("summary")
        .and_then(|s| s.get("high"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = critical <= max_critical && high <= max_high;
    CheckResult {
        name: "vulnscan".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_critical as f64),
        message: if passed {
            if total == 0 {
                "No known vulnerabilities".into()
            } else {
                format!(
                    "{} vulnerabilities ({} critical, {} high) within allowed thresholds",
                    total, critical, high
                )
            }
        } else {
            format!(
                "{} critical + {} high CVEs exceed allowed thresholds ({}/{})",
                critical, high, max_critical, max_high
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: Some(
            "Update vulnerable dependencies. Run cargo audit / npm audit for details.".into(),
        ),
        findings: crate::extract_findings_from_details(&res.data, "vuln_limit", "high"),
        rule_id: Some("vuln_limit".into()),
    }
}
pub fn check_cohesion(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_cohesion_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

pub fn check_cohesion_with_runner(path: &str, recursive: bool, max_violations: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "cohesion", "cohesion", &args, Instant::now());
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let avg_lcom = res
        .data
        .get("summary")
        .and_then(|s| s.get("avg_lcom"))
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let passed = violations <= max_violations;
    CheckResult {
        name: "cohesion".into(),
        passed,
        score: Some(avg_lcom),
        threshold: Some(max_violations as f64),
        message: if passed {
            if violations == 0 {
                format!("All structs cohesive (avg LCOM4 {:.2})", avg_lcom)
            } else {
                format!(
                    "{} cohesion violations <= allowed {} (avg LCOM4 {:.2})",
                    violations, max_violations, avg_lcom
                )
            }
        } else {
            format!(
                "{} structs exceed LCOM4 threshold of {}",
                violations, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some("High LCOM4 means a struct does too many unrelated things. Split it.".into()),
        findings: crate::extract_findings_from_details(&res.data, "cohesion_lcom4", "warning"),
        rule_id: Some("cohesion_lcom4".into()),
    }
}
pub fn check_comments(path: &str, recursive: bool, min_ratio: f64) -> CheckResult {
    check_comments_with_runner(path, recursive, min_ratio, &crate::DefaultToolRunner)
}

pub fn check_comments_with_runner(path: &str, recursive: bool, min_ratio: f64, runner: &dyn crate::ToolRunner) -> CheckResult {
    let min_ratio_str = format!("{}", min_ratio);
    let mut args = vec![path, "--format", "json", "--min-ratio", &min_ratio_str];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "comment-ratio", "comments", &args, Instant::now());
    let below = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_below_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let overall = res
        .data
        .get("summary")
        .and_then(|s| s.get("overall_comment_ratio"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = below == 0;
    CheckResult {
        name: "comments".into(),
        passed,
        score: Some(overall * 100.0),
        threshold: Some(min_ratio * 100.0),
        message: if passed {
            format!("Overall comment ratio {:.1}% >= {:.0}%", overall * 100.0, min_ratio * 100.0)
        } else {
            format!("{} files below comment ratio threshold of {:.0}%", below, min_ratio * 100.0)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("low".into()) },
        help: Some("Add inline comments explaining non-obvious logic. Doc comments are tracked separately by doccov.".into()),
        findings: crate::extract_findings_from_details(&res.data, "comment_ratio", "low"),
        rule_id: Some("comment_ratio".into()),
    }
}
pub fn check_errhandle(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    check_errhandle_with_runner(path, recursive, max_violations, &crate::DefaultToolRunner)
}

pub fn check_errhandle_with_runner(path: &str, recursive: bool, max_violations: usize, runner: &dyn crate::ToolRunner) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = crate::run_tool_with_runner(runner, "error-handling", "errhandle", &args, Instant::now());
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_violations;
    CheckResult {
        name: "errhandle".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if total == 0 {
                "No error handling issues detected".into()
            } else {
                format!(
                    "{} error handling findings <= allowed {}",
                    total, max_violations
                )
            }
        } else {
            format!(
                "{} error handling violations > allowed {}",
                total, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: Some(
            "Replace .unwrap()/.expect() with proper error propagation using `?` or match.".into(),
        ),
        findings: crate::extract_findings_from_details(&res.data, "errhandle_limit", "medium"),
        rule_id: Some("errhandle_limit".into()),
    }
}
