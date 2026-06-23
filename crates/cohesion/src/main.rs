#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "cohesion",
    about = "Cohesion analyzer — LCOM4 metric to detect structs/classes doing too many unrelated things"
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

    /// Max allowed LCOM4 score (default: 2; 1 = perfectly cohesive)
    #[arg(long, default_value = "2")]
    max_lcom: usize,

    /// Show only violations
    #[arg(long)]
    violations_only: bool,
}

#[derive(Debug, Clone, Serialize)]
struct StructCohesion {
    file: String,
    name: String,
    line: usize,
    /// LCOM4: number of disconnected method-field components (1 = cohesive, >1 = should split)
    lcom4: usize,
    /// Fields accessed by at least one method
    fields_accessed: usize,
    /// Total methods analyzed
    methods: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct CohesionReport {
    structs: Vec<StructCohesion>,
    summary: CohesionSummary,
}

#[derive(Serialize)]
struct CohesionSummary {
    files_scanned: usize,
    structs_analyzed: usize,
    violations: usize,
    max_lcom_threshold: usize,
    avg_lcom: f64,
}

/// Parse a Rust source file for struct definitions and their method bodies.
/// Returns (struct_name, start_line, Vec<(method_name, fields_accessed)>)
type MethodInfo = Vec<(String, Vec<String>)>;
fn parse_rust_structs(source: &str) -> Vec<(String, usize, MethodInfo)> {
    let mut result = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Detect `struct Foo {` or `pub struct Foo {`
        let struct_name = if let Some(rest) = line.strip_prefix("pub struct ") {
            rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .map(|s| s.to_string())
        } else if let Some(rest) = line.strip_prefix("struct ") {
            rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .map(|s| s.to_string())
        } else {
            None
        };

        if let Some(name) = struct_name {
            if name.is_empty() {
                i += 1;
                continue;
            }
            let struct_line = i + 1;

            // Collect struct fields by scanning until `}`
            let mut fields: Vec<String> = Vec::new();
            let mut depth = if line.contains('{') { 1usize } else { 0 };
            let mut j = i + 1;
            while j < lines.len() && depth > 0 {
                let l = lines[j].trim();
                depth += l.chars().filter(|&c| c == '{').count();
                depth = depth.saturating_sub(l.chars().filter(|&c| c == '}').count());
                // field: `name: Type,`
                if depth == 1
                    && l.contains(':')
                    && !l.starts_with("//")
                    && !l.starts_with("pub fn")
                    && !l.starts_with("fn ")
                {
                    let field_name = l
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches("pub ")
                        .trim_start_matches("pub(crate) ")
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !field_name.is_empty()
                        && field_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        fields.push(field_name);
                    }
                }
                j += 1;
            }

            // Now look for `impl Name {` block
            let mut methods: Vec<(String, Vec<String>)> = Vec::new();
            let mut k = i + 1;
            while k < lines.len() {
                let impl_line = lines[k].trim();
                let is_impl = (impl_line.starts_with(&format!("impl {name}"))
                    || impl_line.starts_with("impl<"))
                    && impl_line.contains(&name);
                if is_impl {
                    // Scan impl block
                    let mut depth2 = if impl_line.contains('{') { 1usize } else { 0 };
                    let mut m = k + 1;
                    let mut current_method: Option<(String, Vec<String>)> = None;
                    let mut method_depth = 0usize;

                    while m < lines.len() && depth2 > 0 {
                        let ml = lines[m].trim();
                        depth2 += ml.chars().filter(|&c| c == '{').count();
                        depth2 = depth2.saturating_sub(ml.chars().filter(|&c| c == '}').count());

                        // Detect method start
                        if (ml.starts_with("fn ")
                            || ml.starts_with("pub fn ")
                            || ml.starts_with("pub(crate) fn ")
                            || ml.starts_with("async fn "))
                            && ml.contains('(')
                        {
                            if let Some((mn, mf)) = current_method.take() {
                                methods.push((mn, mf));
                            }
                            let method_name = ml
                                .split('(')
                                .next()
                                .unwrap_or("")
                                .split_whitespace()
                                .last()
                                .unwrap_or("")
                                .to_string();
                            current_method = Some((method_name, Vec::new()));
                            method_depth = depth2;
                        }

                        // Detect field accesses: `self.field`
                        if let Some((_, ref mut accessed)) = current_method {
                            for field in &fields {
                                let pattern = format!("self.{}", field);
                                if ml.contains(&pattern) && !accessed.contains(field) {
                                    accessed.push(field.clone());
                                }
                            }
                        }

                        // Detect method end
                        if depth2 < method_depth {
                            if let Some((mn, mf)) = current_method.take() {
                                methods.push((mn, mf));
                            }
                        }

                        m += 1;
                    }
                    if let Some((mn, mf)) = current_method {
                        methods.push((mn, mf));
                    }
                }
                k += 1;
            }

            if methods.len() >= 2 {
                result.push((name, struct_line, methods));
            }
        }
        i += 1;
    }
    result
}

/// Compute LCOM4: number of connected components in the method-field graph.
/// Two methods are connected if they share a field access or one calls the other.
fn compute_lcom4(methods: &[(String, Vec<String>)]) -> usize {
    if methods.is_empty() {
        return 1;
    }

    // Build adjacency: methods[i] and methods[j] connected if they share a field
    let n = methods.len();
    let mut adj: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for i in 0..n {
        for j in (i + 1)..n {
            let fields_i: HashSet<&String> = methods[i].1.iter().collect();
            let fields_j: HashSet<&String> = methods[j].1.iter().collect();
            if !fields_i.is_disjoint(&fields_j) || !fields_i.is_empty() && !fields_j.is_empty() {
                // Only connect if they share at least one field
                if fields_i.intersection(&fields_j).next().is_some() {
                    adj[i].insert(j);
                    adj[j].insert(i);
                }
            }
        }
    }

    count_components(&adj, n)
}

/// Count connected components in an undirected graph via BFS.
fn count_components(adj: &[HashSet<usize>], n: usize) -> usize {
    let mut visited = vec![false; n];
    let mut components = 0;
    for start in 0..n {
        if !visited[start] {
            components += 1;
            let mut queue = vec![start];
            visited[start] = true;
            while let Some(node) = queue.pop() {
                for &neighbor in &adj[node] {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push(neighbor);
                    }
                }
            }
        }
    }
    components
}

fn scan_file(path: &str, max_lcom: usize, violations_only: bool) -> Vec<StructCohesion> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let structs = match ext {
        "rs" => parse_rust_structs(&source),
        _ => return vec![], // Extend for Python/JS class analysis in future
    };

    let mut results = Vec::new();
    for (name, line, methods) in structs {
        let lcom4 = compute_lcom4(&methods);
        let fields_accessed: HashSet<String> = methods
            .iter()
            .flat_map(|(_, f)| f.iter().cloned())
            .collect();

        if violations_only && lcom4 <= max_lcom {
            continue;
        }

        results.push(StructCohesion {
            file: path.to_string(),
            name: name.clone(),
            line,
            lcom4,
            fields_accessed: fields_accessed.len(),
            methods: methods.len(),
            suggested_fix: if lcom4 > max_lcom {
                Some(format!(
                    "`{}` has {} disconnected method groups (LCOM4={}). Consider splitting into {} separate types.",
                    name, lcom4, lcom4, lcom4
                ))
            } else {
                None
            },
        });
    }
    results
}

fn run(cli: Cli) {
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &["rs"])
    };

    let mut all: Vec<StructCohesion> = Vec::new();
    for file in &files {
        all.extend(scan_file(file, cli.max_lcom, cli.violations_only));
    }

    all.sort_by_key(|b| std::cmp::Reverse(b.lcom4));

    let violations = all.iter().filter(|s| s.lcom4 > cli.max_lcom).count();
    let avg_lcom = if all.is_empty() {
        1.0
    } else {
        all.iter().map(|s| s.lcom4 as f64).sum::<f64>() / all.len() as f64
    };

    let summary = CohesionSummary {
        files_scanned: files.len(),
        structs_analyzed: all.len(),
        violations,
        max_lcom_threshold: cli.max_lcom,
        avg_lcom,
    };

    match cli.format.as_str() {
        "json" => {
            let report = CohesionReport {
                structs: all,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for s in &all {
                println!("{}", serde_json::to_string(s).unwrap());
            }
        }
        _ => {
            if all.is_empty() {
                println!("No structs with cohesion issues found.");
            } else {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 35,
                        align_right: false,
                    },
                    Column {
                        header: "Struct",
                        width: 25,
                        align_right: false,
                    },
                    Column {
                        header: "LCOM4",
                        width: 7,
                        align_right: true,
                    },
                    Column {
                        header: "Methods",
                        width: 8,
                        align_right: true,
                    },
                    Column {
                        header: "Fields",
                        width: 7,
                        align_right: true,
                    },
                ];
                print_table_header(&cols);
                for s in &all {
                    let flag = if s.lcom4 > cli.max_lcom { "!" } else { " " };
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&s.file, 35),
                            &truncate(&s.name, 25),
                            &format!("{}{}", flag, s.lcom4),
                            &s.methods.to_string(),
                            &s.fields_accessed.to_string(),
                        ],
                    );
                }
            }
            println!(
                "\nSummary: {} structs analyzed  |  {} exceed LCOM4 threshold ({})  |  avg LCOM4: {:.2}",
                summary.structs_analyzed, summary.violations, summary.max_lcom_threshold, summary.avg_lcom
            );
        }
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcom4_single_method() {
        let methods = vec![("foo".to_string(), vec!["x".to_string()])];
        assert_eq!(compute_lcom4(&methods), 1);
    }

    #[test]
    fn test_lcom4_two_connected_methods() {
        let methods = vec![
            ("foo".to_string(), vec!["x".to_string()]),
            ("bar".to_string(), vec!["x".to_string()]),
        ];
        assert_eq!(compute_lcom4(&methods), 1);
    }

    #[test]
    fn test_lcom4_two_disconnected_methods() {
        let methods = vec![
            ("foo".to_string(), vec!["x".to_string()]),
            ("bar".to_string(), vec!["y".to_string()]),
        ];
        assert_eq!(compute_lcom4(&methods), 2);
    }

    #[test]
    fn test_lcom4_empty() {
        assert_eq!(compute_lcom4(&[]), 1);
    }

    #[test]
    fn test_parse_rust_struct() {
        let src = r#"
struct Foo {
    x: i32,
    y: i32,
}
impl Foo {
    fn get_x(&self) -> i32 { self.x }
    fn get_y(&self) -> i32 { self.y }
}
"#;
        let structs = parse_rust_structs(src);
        assert!(!structs.is_empty());
        let (name, _, methods) = &structs[0];
        assert_eq!(name, "Foo");
        assert_eq!(methods.len(), 2);
    }
}
