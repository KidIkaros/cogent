//! Check runner functions for cogent-cli.

#![deny(clippy::all)]

use colored::Colorize;
use std::time::Instant;
use ast_parse_ts::{parse_complexity_file, parse_doc_coverage_file, Language};
use cogent_common::{crap_score, parse_lcov, CoverageRecord, find_source_files, ToolResult};
use crate::types::{
    extract_findings_from_details, CheckResult, Finding,
};
use crate::progress::{box_row, format_ms, Bar};
use cogent_common::memory::MemoryMonitor;

// CHECKS
// ═══════════════════════════════════════════

/// Scan all source files under `path`, invoking `predicate` on each function.
/// Returns `(total_functions_count, collected_items)`.
pub(crate) fn scan_source_functions<T, F>(path: &str, recursive: bool, mut predicate: F) -> (usize, Vec<T>)
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

pub(crate) fn function_coverage(coverage_records: &[CoverageRecord], func_name: &str) -> f64 {
    coverage_records
        .iter()
        .find(|r| r.function == func_name)
        .map_or(0.0, |r| if r.hits > 0 { 1.0 } else { 0.0 })
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

// NEW 6 CHECK WRAPPERS & SETUP
// ═══════════════════════════════════════════

pub(crate) fn check_taint(path: &str, recursive: bool, max_taint: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("taint-scan", "taint", &args, Instant::now());
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
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
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "taint_limit", "high"),
        rule_id: Some("taint_limit".into()),
    }
}

pub(crate) fn check_dupfind(path: &str, recursive: bool, max_duplication: f64) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("duplication", "dupfind", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "duplication_limit", "medium"),
        rule_id: Some("duplication_limit".into()),
    }
}

pub(crate) fn check_riskmap(path: &str, _recursive: bool, max_risk: f64) -> CheckResult {
    let args = vec![path, "--format", "json"];
    // riskmap doesn't use recursive flag, it always scans dir
    let res = run_tool("risk-map", "riskmap", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "riskmap_limit", "high"),
        rule_id: Some("riskmap_limit".into()),
    }
}

pub(crate) fn check_coupling(path: &str, max_coupling: usize) -> CheckResult {
    let args = vec![path, "--format", "json"];
    let res = run_tool("coupling", "coupling", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "coupling_limit", "medium"),
        rule_id: Some("coupling_limit".into()),
    }
}

pub(crate) fn check_propcov(path: &str, recursive: bool, min_propcov: f64) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("prop-cov", "propcov", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "propcov_limit", "high"),
        rule_id: Some("propcov_limit".into()),
    }
}

pub(crate) fn check_fuzz(path: &str, recursive: bool, max_fuzz_risk: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("fuzz-surface", "fuzz", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "fuzz_limit", "high"),
        rule_id: Some("fuzz_limit".into()),
    }
}

pub(crate) fn check_linelen(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("line-length", "linelen", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "linelen_limit", "warning"),
        rule_id: Some("linelen_limit".into()),
    }
}

pub(crate) fn check_halstead(path: &str, recursive: bool, max_bugs: f64) -> CheckResult {
    let max_bugs_str = format!("{}", max_bugs);
    let mut args = vec![path, "--format", "json", "--max-bugs", &max_bugs_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("halstead", "halstead", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "halstead_bugs", "warning"),
        rule_id: Some("halstead_bugs".into()),
    }
}

pub(crate) fn check_secrets(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("secrets", "secrets", &args, Instant::now());
    let findings = res
        .data
        .get("summary")
        .and_then(|s| s.get("findings_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
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
        findings: extract_findings_from_details(&res.data, "secrets_limit", "high"),
        rule_id: Some("secrets_limit".into()),
    }
}

pub(crate) fn check_deadcode(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("dead-code", "deadcode", &args, Instant::now());
    let findings = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
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
        findings: extract_findings_from_details(&res.data, "deadcode_limit", "warning"),
        rule_id: Some("deadcode_limit".into()),
    }
}

pub(crate) fn check_sast(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("sast", "sast", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("sast", "sast_limit", max_findings, res.error.clone());
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
        findings: extract_findings_from_details(&res.data, "sast_limit", "high"),
        rule_id: Some("sast_limit".into()),
    }
}

pub(crate) fn check_crypto(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("crypto-check", "cryptocheck", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("crypto", "crypto_limit", max_findings, res.error.clone());
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
        findings: extract_findings_from_details(&res.data, "crypto_limit", "high"),
        rule_id: Some("crypto_limit".into()),
    }
}

pub(crate) fn check_licenses(path: &str, max_violations: usize) -> CheckResult {
    let max_str = format!("{}", max_violations);
    let args = vec![path, "--format", "json", "--max-violations", &max_str];
    let res = run_tool("licenses", "licenses", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check(
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
        findings: extract_findings_from_details(&res.data, "license_compliance", "high"),
        rule_id: Some("license_compliance".into()),
    }
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
    let min_pct_str = format!("{}", min_pct);
    let mut args = vec![path, "--format", "json", "--min-pct", &min_pct_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("type-coverage", "typecov", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "typecov_limit", "medium"),
        rule_id: Some("typecov_limit".into()),
    }
}

pub(crate) fn check_vulnscan(path: &str, max_critical: usize, max_high: usize) -> CheckResult {
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
    let res = run_tool("vuln-scan", "vulnscan", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("vulnscan", "vuln_limit", max_critical, res.error.clone());
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
        findings: extract_findings_from_details(&res.data, "vuln_limit", "high"),
        rule_id: Some("vuln_limit".into()),
    }
}

pub(crate) fn check_cohesion(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("cohesion", "cohesion", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "cohesion_lcom4", "warning"),
        rule_id: Some("cohesion_lcom4".into()),
    }
}

pub(crate) fn check_comments(path: &str, recursive: bool, min_ratio: f64) -> CheckResult {
    let min_ratio_str = format!("{}", min_ratio);
    let mut args = vec![path, "--format", "json", "--min-ratio", &min_ratio_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("comment-ratio", "comments", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "comment_ratio", "low"),
        rule_id: Some("comment_ratio".into()),
    }
}

pub(crate) fn check_errhandle(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("error-handling", "errhandle", &args, Instant::now());
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
        findings: extract_findings_from_details(&res.data, "errhandle_limit", "medium"),
        rule_id: Some("errhandle_limit".into()),
    }
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
    }
}

pub(crate) fn check_access_control(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
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

// NEW 6 CHECK WRAPPERS & SETUP
// ═══════════════════════════════════════════


pub(crate) fn run_tool(crate_name: &str, bin_name: &str, args: &[&str], tool_start: Instant) -> ToolResult {
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
    use cogent_common::*;

    use std::time::Instant;

    let start = Instant::now();

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

    let tools: Vec<(&str, &str, Vec<&str>)> = vec![
        (
            "debt-scan",
            "debt",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "doc-coverage",
            "doccov",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "crap-metric",
            "crap",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("coupling", "coupling", vec![path, "--format", "json"]),
        ("risk-map", "riskmap", vec![path, "--format", "json"]),
        (
            "duplication",
            "dupfind",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "prop-cov",
            "propcov",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "taint-scan",
            "taint",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "fuzz-surface",
            "fuzz",
            vec!["--recursive", path, "--format", "json"],
        ),
        // mutation-test: run with capped mutants and enforced timeout.
        // Uses scratch workspace + watchdog kill — safe to include in batch.
        // Note: requires -p flag for package selection
        (
            "mutation-test",
            "mutate",
            vec![
                path,
                "-p",
                "ast-parse-ts",
                "--max-mutants",
                "5",
                "--timeout",
                "30",
                "--format",
                "json",
            ],
        ),
        (
            "line-length",
            "linelen",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "halstead",
            "halstead",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "secrets",
            "secrets",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "dead-code",
            "deadcode",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "cohesion",
            "cohesion",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "comment-ratio",
            "comments",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "error-handling",
            "errhandle",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "type-coverage",
            "typecov",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("vuln-scan", "vulnscan", vec![path, "--format", "json"]),
        (
            "sast",
            "sast",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "crypto-check",
            "cryptocheck",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("licenses", "licenses", vec![path, "--format", "json"]),
    ];

    // Run tools sequentially to prevent memory exhaustion
    // Previous concurrent execution (MAX_CONCURRENT=4) caused OOM crashes on 16GB/32GB systems
    let mut results: Vec<ToolResult> = Vec::new();
    let mut bar = Bar::new(tools.len());
    for (crate_name, bin_name, args) in &tools {
        bar.set_current(bin_name);

        // Check memory before starting tool
        if let Err(usage) = memory_monitor.check() {
            bar.finish();
            eprintln!(
                "  {} Memory limit exceeded before running {} ({} MB used). Stopping batch.",
                "✗".red().bold(),
                bin_name,
                usage.rss_bytes / 1024 / 1024
            );
            break;
        }

        let tool_start = Instant::now();
        let result = run_tool(crate_name, bin_name, args, tool_start);
        let duration_ms = result.duration_ms;
        let success = result.success;
        results.push(result);
        bar.advance(bin_name, success, duration_ms);

        // Check memory after tool completion
        if let Err(usage) = memory_monitor.check() {
            bar.finish();
            eprintln!(
                "  {} Memory limit exceeded after {} ({} MB used). Stopping batch.",
                "✗".red().bold(),
                bin_name,
                usage.rss_bytes / 1024 / 1024
            );
            break;
        }
    }
    bar.finish();

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
                }
            }

            let run = sarif_run(
                "cogent-batch",
                env!("CARGO_PKG_VERSION"),
                sarif_results,
                if failed > 0 { 1 } else { 0 },
            );
            log.add_run(run);
            println!("{}", serde_json::to_string_pretty(&log).unwrap());
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
            let report = UnifiedReport {
                run_id: report.run_id,
                started_at: report.started_at,
                duration_ms,
                tools: results,
                summary: ReportSummary {
                    total_tools: tools.len(),
                    passed,
                    failed,
                    languages_detected: langs_detected,
                },
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
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
// ═══════════════════════════════════════════
