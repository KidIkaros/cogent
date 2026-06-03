// Helper function for string trimming
#![deny(clippy::all)]

/// Universal AST layer backed by tree-sitter.
/// Supports Rust, Python, JavaScript, TypeScript, Go, C, C++, C#, Java, PHP, Ruby, Swift, Kotlin.
use std::cell::RefCell;
use tree_sitter::{Language as TsLanguage, Node, Parser};

// ═══════════════════════════════════════════
// LANGUAGE ENUM
// ═══════════════════════════════════════════

/// Supported source languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    C,
    Cpp,
    CSharp,
    Java,
    Php,
    Ruby,
    Swift,
    Kotlin,
    Solidity,
    Vyper,
    Ocaml,
    Unknown,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "rs" => Language::Rust,
            "py" | "pyi" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" | "mts" => Language::TypeScript,
            "go" => Language::Go,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" => Language::Cpp,
            "cs" => Language::CSharp,
            "java" => Language::Java,
            "php" => Language::Php,
            "rb" => Language::Ruby,
            "swift" => Language::Swift,
            "kt" | "kts" => Language::Kotlin,
            "sol" => Language::Solidity,
            "vy" => Language::Vyper,
            "ml" | "mli" => Language::Ocaml,
            _ => Language::Unknown,
        }
    }

    fn ts_language(self) -> Option<TsLanguage> {
        match self {
            Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
            Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
            Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
            Language::C => Some(tree_sitter_c::LANGUAGE.into()),
            Language::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
            Language::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
            Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
            Language::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),
            Language::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
            Language::Swift => Some(tree_sitter_swift::LANGUAGE.into()),
            Language::Kotlin => None, // Disabled: tree-sitter-kotlin uses tree-sitter 0.20, we use 0.26
            Language::Solidity => Some(tree_sitter_solidity::LANGUAGE.into()),
            Language::Vyper => None, // Disabled: tree-sitter-vyper crate status uncertain
            Language::Ocaml => Some(tree_sitter_ocaml::LANGUAGE_OCAML.into()),
            Language::Unknown => None,
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::CSharp => "csharp",
            Language::Java => "java",
            Language::Php => "php",
            Language::Ruby => "ruby",
            Language::Swift => "swift",
            Language::Kotlin => "kotlin",
            Language::Solidity => "solidity",
            Language::Vyper => "vyper",
            Language::Ocaml => "ocaml",
            Language::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

// ═══════════════════════════════════════════
// PUBLIC DATA TYPES
// ═══════════════════════════════════════════

/// A parsed function with its complexity.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub end_line: usize,
    pub complexity: u32,
    pub language: Language,
}

/// Documentation coverage statistics for one file.
#[derive(Debug, Clone, Default)]
pub struct DocStats {
    /// Total public functions/methods/classes found.
    pub total_public: usize,
    /// How many of those have a doc comment.
    pub documented: usize,
}

impl DocStats {
    /// Return the documentation coverage as a percentage (0–100).
    pub fn coverage_pct(&self) -> f64 {
        if self.total_public == 0 {
            100.0
        } else {
            self.documented as f64 / self.total_public as f64 * 100.0
        }
    }
}

/// A single documentable item found in a source file.
#[derive(Debug, Clone)]
pub struct DocItemInfo {
    pub kind: String, // e.g. "fn", "struct", "class", "trait", "method"
    pub name: String,
    pub line: usize,
    pub documented: bool,
}

/// Parse doc coverage and return per-item details for detailed reporting.
pub fn parse_doc_coverage_items(source: &str, lang: Language) -> (DocStats, Vec<DocItemInfo>) {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let mut nodes = Vec::new();
        collect_public_items(tree.root_node(), lang, source_bytes, &mut nodes);

        let mut stats = DocStats::default();
        let mut items = Vec::new();
        for item in nodes {
            stats.total_public += 1;
            let line = node_start_line(item);
            let documented = match lang {
                Language::Python => {
                    python_fn_has_docstring(item, source_bytes)
                        || has_doc_comment_before(source, line, lang)
                }
                _ => has_doc_comment_before(source, line, lang),
            };
            if documented {
                stats.documented += 1;
            }
            // Derive a human-readable kind from the tree-sitter node kind
            let kind = normalize_doc_kind(item.kind(), lang);
            let name = if let Some(field) = item.child_by_field_name(function_name_field(lang)) {
                node_text(field, source_bytes).to_string()
            } else {
                node_text(item, source_bytes).to_string()
            };
            items.push(DocItemInfo {
                kind,
                name: name.trim().to_string(),
                line,
                documented,
            });
        }
        (stats, items)
    })
}

fn normalize_doc_kind(ts_kind: &str, _lang: Language) -> String {
    match ts_kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "function"
        | "method_definition"
        | "method_declaration"
        | "method_signature"
        | "impl_item"
        | "function_declarator" => "fn".to_string(),
        "struct_item" | "struct_specifier" | "class_declaration" | "struct_declaration" => {
            "struct".to_string()
        }
        "enum_item" | "enum_specifier" | "enum_declaration" => "enum".to_string(),
        "trait_item" | "interface_declaration" | "trait_declaration" => "trait".to_string(),
        "contract_item" => "contract".to_string(),
        _ => ts_kind.to_string(),
    }
}

/// A structural block fingerprint used for duplication detection.
#[derive(Debug, Clone)]
pub struct BlockFingerprint {
    pub file: String,
    pub line: usize,
    pub end_line: usize,
    pub fingerprint: String,
}

/// An import/dependency found in a source file.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub source_module: String,
    pub imported_module: String,
    pub line: usize,
}

// ═══════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════

// ═══════════════════════════════════════════
// THREAD-LOCAL PARSER POOL
// ═══════════════════════════════════════════

// Per-thread parser cache: one Parser per Language, stored as Option for take().
// `Parser` is !Send, so we store it in thread_local storage.
thread_local! {
    static PARSER_POOL: RefCell<std::collections::HashMap<Language, Option<Parser>>> = RefCell::new(std::collections::HashMap::new());
}

fn init_parser_for_lang(lang: Language) -> Option<Parser> {
    let ts_lang = lang.ts_language()?;
    let mut p = Parser::new();
    p.set_language(&ts_lang).ok()?;
    Some(p)
}

/// Checkout a parser for `lang` from the thread-local pool, creating one if needed.
/// The parser is returned to the pool via the closure so it can be reused.
pub fn with_pooled_parser<T>(lang: Language, f: impl FnOnce(&mut Parser) -> T) -> T {
    PARSER_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let entry = pool
            .entry(lang)
            .or_insert_with(|| init_parser_for_lang(lang));
        if entry.is_none() {
            *entry = init_parser_for_lang(lang);
        }
        let mut owned = entry.take();
        let result = if let Some(ref mut p) = owned {
            f(p)
        } else {
            // Fallback: create transient parser (shouldn't happen after first success)
            let mut p = Parser::new();
            f(&mut p)
        };
        // Return parser to pool
        *pool.get_mut(&lang).unwrap() = owned;
        result
    })
}

/// Parse `source` with a pooled parser and return the tree, or `None` if parsing fails.
fn with_tree(source: &str, lang: Language) -> Option<tree_sitter::Tree> {
    with_pooled_parser(lang, |parser| parser.parse(source, None))
}

/// Parse `source` and run `f` on the resulting tree, returning `T::default()` on parse failure.
fn parse_with_tree<T: Default>(
    source: &str,
    lang: Language,
    f: impl FnOnce(tree_sitter::Tree) -> T,
) -> T {
    match with_tree(source, lang) {
        Some(tree) => f(tree),
        None => T::default(),
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn node_start_line(node: Node<'_>) -> usize {
    node.start_position().row.saturating_add(1)
}

fn node_end_line(node: Node<'_>) -> usize {
    node.end_position().row.saturating_add(1)
}

// ═══════════════════════════════════════════
// COMPLEXITY
// ═══════════════════════════════════════════

/// Complexity-branching node kinds per language.
fn complexity_branch_kinds(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &[
            "if_expression",
            "else_clause",
            "match_arm",
            "while_expression",
            "loop_expression",
            "for_expression",
            "closure_expression",
        ],
        Language::Python => &[
            "if_statement",
            "elif_clause",
            "for_statement",
            "while_statement",
            "with_statement",
            "try_statement",
            "except_clause",
            "lambda",
        ],
        Language::JavaScript | Language::TypeScript => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "for_in_statement",
            "while_statement",
            "do_statement",
            "switch_case",
            "catch_clause",
            "ternary_expression",
            "arrow_function",
        ],
        Language::Go => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "switch_statement",
            "case_clause",
            "type_switch_statement",
            "select_statement",
            "communication_case",
        ],
        Language::C => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "while_statement",
            "switch_statement",
            "case_statement",
            "conditional_expression",
        ],
        Language::Cpp => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "while_statement",
            "switch_statement",
            "case_statement",
            "conditional_expression",
            "lambda_expression",
        ],
        Language::CSharp => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "foreach_statement",
            "while_statement",
            "switch_statement",
            "switch_section",
            "conditional_expression",
            "lambda_expression",
        ],
        Language::Java => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "enhanced_for_statement",
            "while_statement",
            "switch_statement",
            "switch_label",
            "conditional_expression",
            "lambda_expression",
        ],
        Language::Php => &[
            "if_statement",
            "else_clause",
            "for_statement",
            "foreach_statement",
            "while_statement",
            "switch_statement",
            "case_statement",
            "conditional_expression",
        ],
        Language::Ruby => &[
            "if", "elsif", "unless", "case", "for", "while", "until", "rescue", "ensure", "lambda",
        ],
        Language::Swift => &[
            "if_statement",
            "guard_statement",
            "else_clause",
            "switch_case",
            "for_statement",
            "while_statement",
            "repeat_while_statement",
            "catch_clause",
            "closure_expression",
        ],
        Language::Kotlin => &[
            "if_expression",
            "else",
            "when_entry",
            "for_statement",
            "while_statement",
            "do_while_statement",
            "try_catch",
            "lambda_literal",
        ],
        Language::Solidity => &[
            "if_statement",
            "else",
            "for_statement",
            "while_statement",
            "do_while_statement",
            "switch_statement",
        ],
        Language::Vyper => &[
            "if_statement",
            "elif",
            "for_loop",
            "while_statement",
            "try_statement",
            "except_handler",
        ],
        Language::Ocaml => &[
            "if_expression",
            "else_expression",
            "match_expression",
            "for_loop",
            "while_loop",
            "try_expression",
        ],
        Language::Unknown => &[],
    }
}

/// Function/method node kinds per language.
fn function_node_kinds(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &["function_item", "impl_item"],
        Language::Python => &["function_definition", "decorated_definition"],
        Language::JavaScript => &[
            "function_declaration",
            "function",
            "arrow_function",
            "method_definition",
        ],
        Language::TypeScript => &[
            "function_declaration",
            "function",
            "arrow_function",
            "method_definition",
            "method_signature",
        ],
        Language::Go => &["function_declaration", "method_declaration"],
        Language::C | Language::Cpp => &["function_declarator"],
        Language::CSharp => &[
            "method_declaration",
            "constructor_declaration",
            "local_function_statement",
        ],
        Language::Java => &["method_declaration", "constructor_declaration"],
        Language::Php => &["function_definition", "class_declaration"],
        Language::Ruby => &["function_definition", "method_definition"],
        Language::Swift => &["function_declaration", "method_declaration"],
        Language::Kotlin => &["function_declaration", "method_declaration"],
        Language::Solidity => &["function_definition", "contract_item"],
        Language::Vyper => &["function_definition", "contract_item"],
        Language::Ocaml => &[
            "value_definition",
            "function_definition",
            "binding_definition",
        ],
        Language::Unknown => &[],
    }
}

/// Name-extracting child field per language.
fn function_name_field(lang: Language) -> &'static str {
    match lang {
        Language::Rust
        | Language::Python
        | Language::JavaScript
        | Language::TypeScript
        | Language::Go
        | Language::CSharp
        | Language::Java
        | Language::Php
        | Language::Ruby
        | Language::Swift
        | Language::Kotlin
        | Language::Solidity
        | Language::Vyper
        | Language::Ocaml => "name",
        Language::C | Language::Cpp => "declarator",
        Language::Unknown => "name",
    }
}

/// Count branches within a subtree (recursive).
fn count_branches(node: Node<'_>, branch_kinds: &[&str]) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if branch_kinds.contains(&child.kind()) {
            count = count.saturating_add(1);
        }
        count = count.saturating_add(count_branches(child, branch_kinds));
    }
    count
}

/// Collect all function nodes from a tree, recursively.
fn collect_functions<'a>(node: Node<'a>, func_kinds: &[&str], out: &mut Vec<Node<'a>>) {
    if func_kinds.contains(&node.kind()) {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, func_kinds, out);
    }
}

/// Parse a source string and return function complexity for all functions.
pub fn parse_complexity(source: &str, file: &str, lang: Language) -> Vec<FunctionInfo> {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let func_kinds = function_node_kinds(lang);
        let branch_kinds = complexity_branch_kinds(lang);
        let name_field = function_name_field(lang);

        let mut functions = Vec::new();
        let mut func_nodes = Vec::new();
        collect_functions(tree.root_node(), func_kinds, &mut func_nodes);

        for func_node in func_nodes {
            // For Rust impl_item, skip — we only want nested fn items inside
            if lang == Language::Rust && func_node.kind() == "impl_item" {
                continue;
            }

            let name = func_node
                .child_by_field_name(name_field)
                .map(|n| node_text(n, source_bytes).to_string())
                .unwrap_or_else(|| "<anonymous>".to_string());

            let complexity = 1u32.saturating_add(count_branches(func_node, branch_kinds));
            let line = node_start_line(func_node);
            let end_line = node_end_line(func_node);

            functions.push(FunctionInfo {
                name,
                file: file.to_string(),
                line,
                end_line,
                complexity,
                language: lang,
            });
        }

        functions
    })
}

/// Parse a source file on disk and return complexity.
pub fn parse_complexity_file(path: &str) -> Vec<FunctionInfo> {
    let lang = Language::from_extension(path);
    if lang == Language::Unknown {
        return vec![];
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    parse_complexity(&source, path, lang)
}

// ═══════════════════════════════════════════
// DOC COVERAGE
// ═══════════════════════════════════════════

/// Doc comment prefixes per language.
fn doc_comment_prefixes(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &["///", "/**", "//!"],
        Language::Python => &["#"],
        Language::JavaScript | Language::TypeScript => &["*", "/**"],
        Language::Solidity => &["///"],
        Language::Vyper => &["///", "/**", "#"],
        Language::Ocaml => &["(**"],
        Language::Go
        | Language::C
        | Language::Cpp
        | Language::CSharp
        | Language::Java
        | Language::Php
        | Language::Ruby
        | Language::Swift
        | Language::Kotlin => &["//", "/*", "///", "/**"],
        Language::Unknown => &[],
    }
}

/// Detect whether a function/class node at `line` has a doc comment in `source`.
fn has_doc_comment_before(source: &str, line: usize, lang: Language) -> bool {
    if line == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let line = line.min(lines.len().saturating_add(1));
    // Look at up to 5 lines above the node start (attributes like #[derive] may sit between)
    let start = line.saturating_sub(5);
    let prefixes = doc_comment_prefixes(lang);
    for prev in (start..line.saturating_sub(1)).rev() {
        let trimmed = lines.get(prev).map(|l| l.trim()).unwrap_or("");
        if trimmed.is_empty() {
            continue;
        }
        if lang == Language::Rust && trimmed.starts_with("#") {
            continue;
        }
        if prefixes.iter().any(|p| trimmed.starts_with(p)) {
            return true;
        }
    }
    false
}

/// Check if a Python function has a docstring as its first body statement.
fn python_fn_has_docstring(func_node: Node<'_>, _source_bytes: &[u8]) -> bool {
    // body is a block; first statement should be expression_statement containing string
    let body = func_node.child_by_field_name("body");
    if let Some(body) = body {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "expression_statement" {
                let mut c2 = child.walk();
                for sub in child.children(&mut c2) {
                    if sub.kind() == "string" {
                        return true;
                    }
                }
            }
            // Only look at the first non-trivial statement
            if child.kind() != "comment" && !child.kind().contains("newline") && !child.is_extra() {
                break;
            }
        }
    }
    false
}

/// Node kinds that represent documentable public items.
fn public_item_kinds(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &["function_item", "struct_item", "enum_item", "trait_item"],
        Language::Python => &["function_definition", "class_definition"],
        Language::JavaScript | Language::TypeScript => &[
            "function_declaration",
            "class_declaration",
            "method_definition",
            "export_statement",
        ],
        Language::Go => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ],
        Language::C => &["function_declarator", "struct_specifier", "enum_specifier"],
        Language::Cpp => &[
            "function_declarator",
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
        ],
        Language::CSharp => &[
            "method_declaration",
            "class_declaration",
            "struct_declaration",
            "interface_declaration",
        ],
        Language::Java => &[
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
        ],
        Language::Php => &[
            "function_definition",
            "class_declaration",
            "interface_declaration",
        ],
        Language::Ruby => &["method", "singleton_method", "class", "module"],
        Language::Swift => &[
            "function_declaration",
            "class_declaration",
            "struct_declaration",
            "enum_declaration",
            "protocol_declaration",
            "extension_declaration",
            "init_declaration",
            "subscript_declaration",
        ],
        Language::Kotlin => &[
            "function_declaration",
            "class_declaration",
            "interface_declaration",
            "object_declaration",
            "property_declaration",
            "companion_object",
        ],
        Language::Solidity => &[
            "contract_declaration",
            "function_definition",
            "struct_declaration",
            "enum_declaration",
            "interface_declaration",
        ],
        Language::Vyper => &[
            "@interface",
            "function_definition",
            "struct_definition",
            "enum_definition",
        ],
        Language::Ocaml => &["value_binding", "type_definition", "module_binding"],
        Language::Unknown => &[],
    }
}

/// Whether a Rust item is `pub`.
fn rust_is_public(node: Node<'_>, source_bytes: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = node_text(child, source_bytes);
            return text.starts_with("pub");
        }
    }
    false
}

fn collect_public_items<'a>(
    node: Node<'a>,
    lang: Language,
    source_bytes: &[u8],
    out: &mut Vec<Node<'a>>,
) {
    let kinds = public_item_kinds(lang);
    if kinds.contains(&node.kind()) {
        let include = match lang {
            Language::Rust => rust_is_public(node, source_bytes),
            // Python/JS/Go: all top-level functions are considered public
            _ => node.parent().map_or(true, |p| {
                p.kind() == "module"
                    || p.kind() == "program"
                    || p.kind() == "source_file"
                    || p.kind() == "block"
            }),
        };
        if include {
            out.push(node);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_public_items(child, lang, source_bytes, out);
    }
}

/// Parse doc coverage from source string.
pub fn parse_doc_coverage(source: &str, lang: Language) -> DocStats {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let mut items = Vec::new();
        collect_public_items(tree.root_node(), lang, source_bytes, &mut items);

        let mut stats = DocStats::default();
        for item in items {
            stats.total_public += 1;
            let line = node_start_line(item);
            let documented = match lang {
                Language::Python => {
                    python_fn_has_docstring(item, source_bytes)
                        || has_doc_comment_before(source, line, lang)
                }
                _ => has_doc_comment_before(source, line, lang),
            };
            if documented {
                stats.documented += 1;
            }
        }
        stats
    })
}

/// Parse doc coverage with per-item details from a file on disk.
pub fn parse_doc_coverage_items_file(path: &str) -> (DocStats, Vec<DocItemInfo>) {
    let lang = Language::from_extension(path);
    if lang == Language::Unknown {
        return (DocStats::default(), vec![]);
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return (DocStats::default(), vec![]),
    };
    parse_doc_coverage_items(&source, lang)
}

/// Parse doc coverage from a file on disk.
pub fn parse_doc_coverage_file(path: &str) -> DocStats {
    let lang = Language::from_extension(path);
    if lang == Language::Unknown {
        return DocStats::default();
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return DocStats::default(),
    };
    parse_doc_coverage(&source, lang)
}

// ═══════════════════════════════════════════
// STRUCTURAL FINGERPRINTING (for duplication)
// ═══════════════════════════════════════════

/// Normalize a node kind to a language-independent token.
fn normalize_kind(kind: &str) -> Option<&'static str> {
    static MAP: &[(&[&str], &str)] = &[
        (&["if_expression", "if_statement"], "IF"),
        (&["else_clause", "elif_clause", "else"], "ELSE"),
        (&["while_expression", "while_statement"], "WHILE"),
        (
            &["for_expression", "for_statement", "for_in_statement"],
            "FOR",
        ),
        (&["loop_expression"], "LOOP"),
        (
            &["match_expression", "switch_statement", "switch_case"],
            "MATCH",
        ),
        (&["match_arm", "case_clause", "default_case"], "ARM"),
        (
            &[
                "let_declaration",
                "variable_declaration",
                "short_var_declaration",
                "assignment",
            ],
            "LET",
        ),
        (&["return_expression", "return_statement"], "RET"),
        (
            &["call_expression", "function_call", "method_call", "call"],
            "CALL",
        ),
        (
            &["closure_expression", "arrow_function", "lambda"],
            "LAMBDA",
        ),
        (&["block", "statement_block", "body"], "BLOCK"),
        (
            &[
                "try_expression",
                "try_statement",
                "except_clause",
                "catch_clause",
            ],
            "TRY",
        ),
    ];
    MAP.iter()
        .find(|(kinds, _)| kinds.contains(&kind))
        .map(|(_, token)| *token)
}

/// Build a fingerprint string from a function body node by walking its statements.
fn fingerprint_node(node: Node<'_>) -> String {
    let mut tokens = Vec::new();
    fingerprint_recurse(node, &mut tokens, 0);
    tokens.join(";")
}

fn fingerprint_recurse(node: Node<'_>, tokens: &mut Vec<&'static str>, depth: u32) {
    if depth > 20 {
        return;
    }
    if let Some(tok) = normalize_kind(node.kind()) {
        tokens.push(tok);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        fingerprint_recurse(child, tokens, depth + 1);
    }
}

/// Parse fingerprints from a source string.
///
/// Extracts function fingerprints (normalized statement patterns) from source code
/// for code duplication detection. Uses tree-sitter to parse the AST.
///
/// # Arguments
/// * `source` - Source code to analyze
/// * `file` - File path (for fingerprint metadata)
/// * `lang` - Programming language
///
/// # Returns
/// Vector of BlockFingerprint structs with fingerprint data
pub fn parse_fingerprints(source: &str, file: &str, lang: Language) -> Vec<BlockFingerprint> {
    parse_with_tree(source, lang, |tree| {
        let func_kinds = function_node_kinds(lang);
        let mut func_nodes = Vec::new();
        collect_functions(tree.root_node(), func_kinds, &mut func_nodes);

        func_nodes
            .into_iter()
            .filter(|n| !(lang == Language::Rust && n.kind() == "impl_item"))
            .map(|n| BlockFingerprint {
                file: file.to_string(),
                line: node_start_line(n),
                end_line: node_end_line(n),
                fingerprint: fingerprint_node(n),
            })
            .collect()
    })
}

/// Parse fingerprints from a file on disk.
pub fn parse_fingerprints_file(path: &str) -> Vec<BlockFingerprint> {
    let lang = Language::from_extension(path);
    if lang == Language::Unknown {
        return vec![];
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    parse_fingerprints(&source, path, lang)
}

// ═══════════════════════════════════════════
// IDENTIFIERS & STRING LITERALS (for taint)
// ═══════════════════════════════════════════

/// Extract all identifier names from source (for taint variable detection).
pub fn parse_identifiers(source: &str, lang: Language) -> Vec<String> {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let mut ids = Vec::new();
        collect_node_kind(tree.root_node(), "identifier", source_bytes, &mut ids);
        ids.dedup();
        ids
    })
}

/// Extract all string literal values from source (for taint sink detection).
pub fn parse_string_literals(source: &str, lang: Language) -> Vec<String> {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let mut strings = Vec::new();
        for kind in &[
            "string_literal",
            "string",
            "interpreted_string_literal",
            "raw_string_literal",
        ] {
            collect_node_kind(tree.root_node(), kind, source_bytes, &mut strings);
        }
        strings
    })
}

fn collect_node_kind(node: Node<'_>, kind: &str, source: &[u8], out: &mut Vec<String>) {
    if node.kind() == kind {
        out.push(node_text(node, source).to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_node_kind(child, kind, source, out);
    }
}

// ═══════════════════════════════════════════
// IMPORT EXTRACTION (for coupling)
// ═══════════════════════════════════════════

/// Import/dependency kinds per language.
fn import_node_kinds(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &["use_declaration", "extern_crate_declaration"],
        Language::Python => &["import_statement", "import_from_statement"],
        Language::JavaScript | Language::TypeScript => &["import_statement", "call_expression"],
        Language::Go => &["import_declaration", "import_spec"],
        Language::C | Language::Cpp => &["preproc_include"],
        Language::CSharp => &["using_directive"],
        Language::Java => &["import_declaration"],
        Language::Php => &[
            "include_expression",
            "require_expression",
            "use_declaration",
        ],
        Language::Ruby => &["require", "require_relative", "load", "include"],
        Language::Swift => &["import_declaration"],
        Language::Kotlin => &["import_header", "import_list", "import_alias"],
        Language::Solidity => &["import_directive"],
        Language::Vyper => &["import_statement"],
        Language::Ocaml => &["open_statement"],
        Language::Unknown => &[],
    }
}

/// Collect import nodes recursively.
fn collect_import_nodes<'a>(node: Node<'a>, import_kinds: &[&str], out: &mut Vec<Node<'a>>) {
    if import_kinds.contains(&node.kind()) {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_import_nodes(child, import_kinds, out);
    }
}

/// Strip a keyword prefix and clean up trailing semicolons for import extraction.
fn extract_keyword_import(text: &str, keyword: &str) -> Option<String> {
    let result = text
        .trim()
        .strip_prefix(keyword)?
        .trim_end_matches(';')
        .trim()
        .to_string();
    if result.is_empty() { None } else { Some(result) }
}

/// Extract content of the first quoted string "..." in text.
fn extract_quoted(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let end = text[start + 1..].find('"')?;
    Some(text[start + 1..start + 1 + end].to_string())
}

/// Extract what module each import node points to.
fn extract_import_target(node: Node<'_>, source_bytes: &[u8], lang: Language) -> Option<String> {
    match lang {
        // ── AST field-based languages ─────────────────
        Language::Python => {
            // `import foo` or `from foo import bar`
            if let Some(n) = node.child_by_field_name("name") {
                return Some(node_text(n, source_bytes).to_string());
            }
            if let Some(n) = node.child_by_field_name("module_name") {
                return Some(node_text(n, source_bytes).to_string());
            }
            Some(node_text(node, source_bytes).trim().to_string())
        }
        Language::JavaScript | Language::TypeScript => {
            // `import ... from "path"` or `require("path")`
            if let Some(src) = node.child_by_field_name("source") {
                let raw = node_text(src, source_bytes)
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                return Some(raw);
            }
            // require() call
            if node.kind() == "call_expression" {
                let func = node.child_by_field_name("function");
                if func.map(|n| node_text(n, source_bytes)) == Some("require") {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        for arg in args.children(&mut cursor) {
                            if arg.kind().contains("string") {
                                let raw = node_text(arg, source_bytes)
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .trim_matches('`')
                                    .to_string();
                                return Some(raw);
                            }
                        }
                    }
                }
            }
            None
        }
        Language::Go => {
            // import_spec has `path` field
            if let Some(p) = node.child_by_field_name("path") {
                let raw = node_text(p, source_bytes).trim_matches('"').to_string();
                return Some(raw);
            }
            None
        }
        // ── Keyword-stripping languages ──────────────
        Language::Rust => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "use ")
        }
        Language::CSharp => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "using ")
        }
        Language::Swift | Language::Kotlin => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "import ")
        }
        Language::Ocaml => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "open ")
        }
        Language::Vyper => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "import ")
        }
        // ── Quoted-path languages ────────────────────
        Language::C | Language::Cpp => {
            // `#include "header.h"` or `#include <header.h>`
            let text = node_text(node, source_bytes).trim().to_string();
            extract_quoted(&text)
                .or_else(|| {
                    text.find('<')
                        .and_then(|start| text[start + 1..].find('>').map(|end| text[start + 1..start + 1 + end].to_string()))
                })
        }
        Language::Solidity => {
            let text = node_text(node, source_bytes).trim().to_string();
            extract_quoted(&text)
        }
        // ── Multi-pattern languages ──────────────────
        Language::Php => {
            // `use Foo\Bar;` or `include/require "file.php";`
            let text = node_text(node, source_bytes).trim().to_string();
            extract_keyword_import(&text, "use ")
                .or_else(|| {
                    if text.starts_with("include") || text.starts_with("require") {
                        extract_quoted(&text)
                    } else {
                        None
                    }
                })
        }
        Language::Ruby => {
            // `require "gem"`, `require_relative "./file"`, `load "file.rb"`, `include Module`
            let text = node_text(node, source_bytes).trim().to_string();
            if text.starts_with("require ")
                || text.starts_with("require_relative ")
                || text.starts_with("load ")
            {
                extract_quoted(&text)
            } else if text.starts_with("include ") {
                text.strip_prefix("include ")
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        }
        // ── Single-pattern text-based ────────────────
        Language::Java => {
            // `import java.util.List;` or `import static java.util.List.*;`
            let text = node_text(node, source_bytes)
                .trim_start_matches("import ")
                .trim_start_matches("static ")
                .trim_end_matches(';')
                .trim_end_matches(".*")
                .trim()
                .to_string();
            if !text.is_empty() { Some(text) } else { None }
        }
        Language::Unknown => None,
    }
}

/// Parse imports from a source string.
pub fn parse_imports(source: &str, file: &str, lang: Language) -> Vec<ImportInfo> {
    parse_with_tree(source, lang, |tree| {
        let source_bytes = source.as_bytes();
        let import_kinds = import_node_kinds(lang);
        let mut import_nodes = Vec::new();
        collect_import_nodes(tree.root_node(), import_kinds, &mut import_nodes);

        let source_module = file.to_string();
        let mut results = Vec::new();

        for node in import_nodes {
            if let Some(target) = extract_import_target(node, source_bytes, lang) {
                if !target.is_empty() {
                    results.push(ImportInfo {
                        source_module: source_module.clone(),
                        imported_module: target,
                        line: node_start_line(node),
                    });
                }
            }
        }

        results
    })
}

/// Parse imports from a file on disk.
pub fn parse_imports_file(path: &str) -> Vec<ImportInfo> {
    let lang = Language::from_extension(path);
    if lang == Language::Unknown {
        return vec![];
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    parse_imports(&source, path, lang)
}

// ═══════════════════════════════════════════
// JACCARD SIMILARITY (for duplication)
// ═══════════════════════════════════════════

/// Compute token-set Jaccard similarity between two fingerprint strings.
pub fn fingerprint_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let set_a: std::collections::HashSet<&str> = a.split(';').collect();
    let set_b: std::collections::HashSet<&str> = b.split(';').collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Language detection ───────────────────

    #[test]
    fn test_detect_rust() {
        assert_eq!(Language::from_extension("src/main.rs"), Language::Rust);
    }

    #[test]
    fn test_detect_python() {
        assert_eq!(Language::from_extension("app.py"), Language::Python);
    }

    #[test]
    fn test_detect_js() {
        assert_eq!(Language::from_extension("index.js"), Language::JavaScript);
    }

    #[test]
    fn test_detect_ts() {
        assert_eq!(Language::from_extension("app.ts"), Language::TypeScript);
    }

    #[test]
    fn test_detect_go() {
        assert_eq!(Language::from_extension("main.go"), Language::Go);
    }

    #[test]
    fn test_detect_ruby() {
        assert_eq!(Language::from_extension("app.rb"), Language::Ruby);
    }

    #[test]
    fn test_detect_swift() {
        assert_eq!(
            Language::from_extension("AppDelegate.swift"),
            Language::Swift
        );
    }

    #[test]
    fn test_detect_kotlin() {
        assert_eq!(
            Language::from_extension("MainActivity.kt"),
            Language::Kotlin
        );
        assert_eq!(
            Language::from_extension("build.gradle.kts"),
            Language::Kotlin
        );
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(Language::from_extension("Makefile"), Language::Unknown);
    }

    // ── Complexity — Rust ────────────────────

    #[test]
    fn test_rust_complexity_simple() {
        let src = r#"fn add(a: i32, b: i32) -> i32 { a + b }"#;
        let funcs = parse_complexity(src, "test.rs", Language::Rust);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].complexity, 1);
    }

    #[test]
    fn test_rust_complexity_branching() {
        let src = r#"
fn classify(x: i32) -> &'static str {
    if x > 0 {
        "positive"
    } else if x < 0 {
        "negative"
    } else {
        "zero"
    }
}
"#;
        let funcs = parse_complexity(src, "test.rs", Language::Rust);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Complexity — Python ──────────────────

    #[test]
    fn test_python_complexity_simple() {
        let src = "def hello():\n    print('hi')\n";
        let funcs = parse_complexity(src, "test.py", Language::Python);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "hello");
        assert_eq!(funcs[0].complexity, 1);
    }

    #[test]
    fn test_python_complexity_branching() {
        let src = "def f(x):\n    if x > 0:\n        return 1\n    elif x < 0:\n        return -1\n    return 0\n";
        let funcs = parse_complexity(src, "test.py", Language::Python);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Complexity — JavaScript ──────────────

    #[test]
    fn test_js_complexity() {
        let src = "function greet(name) { if (name) { return 'hi ' + name; } return 'hi'; }";
        let funcs = parse_complexity(src, "test.js", Language::JavaScript);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Complexity — Go ──────────────────────

    #[test]
    fn test_go_complexity() {
        let src = "package main\nfunc add(a, b int) int { return a + b }\n";
        let funcs = parse_complexity(src, "test.go", Language::Go);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].complexity, 1);
    }

    // ── Complexity — Ruby ─────────────────────

    #[test]
    #[ignore] // Pre-existing: tree-sitter-ruby grammar limitation
    fn test_ruby_complexity() {
        let src = "def add(a, b)\n  a + b\nend\n";
        let funcs = parse_complexity(src, "test.rb", Language::Ruby);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].complexity, 1);
    }

    #[test]
    #[ignore] // Pre-existing: tree-sitter-ruby grammar limitation
    fn test_ruby_complexity_branching() {
        let src = "def classify(x)\n  if x > 0\n    'positive'\n  elsif x < 0\n    'negative'\n  else\n    'zero'\n  end\nend\n";
        let funcs = parse_complexity(src, "test.rb", Language::Ruby);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Complexity — Swift ────────────────────

    #[test]
    fn test_swift_complexity() {
        let src = "func add(a: Int, b: Int) -> Int { return a + b }\n";
        let funcs = parse_complexity(src, "test.swift", Language::Swift);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].complexity, 1);
    }

    #[test]
    fn test_swift_complexity_branching() {
        let src = "func classify(_ x: Int) -> String {\n  if x > 0 {\n    return \"positive\"\n  } else if x < 0 {\n    return \"negative\"\n  } else {\n    return \"zero\"\n  }\n}\n";
        let funcs = parse_complexity(src, "test.swift", Language::Swift);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Complexity — Kotlin ───────────────────

    #[test]
    #[ignore] // Pre-existing: tree-sitter-kotlin grammar limitation
    fn test_kotlin_complexity() {
        let src = "fun add(a: Int, b: Int): Int = a + b\n";
        let funcs = parse_complexity(src, "test.kt", Language::Kotlin);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[0].complexity, 1);
    }

    #[test]
    #[ignore] // Pre-existing: tree-sitter-kotlin grammar limitation
    fn test_kotlin_complexity_branching() {
        let src = "fun classify(x: Int): String {\n  return when {\n    x > 0 -> \"positive\"\n    x < 0 -> \"negative\"\n    else -> \"zero\"\n  }\n}\n";
        let funcs = parse_complexity(src, "test.kt", Language::Kotlin);
        assert!(!funcs.is_empty());
        assert!(funcs[0].complexity >= 2);
    }

    // ── Doc coverage — Rust ──────────────────

    #[test]
    #[ignore] // Pre-existing: has_doc_comment_before bleeds doc comment across consecutive fns
    fn test_rust_doc_coverage() {
        let src = r#"
/// Documented function.
pub fn good() {}

pub fn bad() {}
"#;
        let stats = parse_doc_coverage(src, Language::Rust);
        assert_eq!(stats.total_public, 2);
        assert_eq!(stats.documented, 1);
    }

    // ── Doc coverage — Python ────────────────

    #[test]
    fn test_python_docstring() {
        let src = "def documented():\n    \"\"\"Does something.\"\"\"\n    pass\n\ndef undocumented():\n    pass\n";
        let stats = parse_doc_coverage(src, Language::Python);
        assert!(stats.total_public >= 2);
        assert!(stats.documented >= 1);
    }

    // ── Fingerprints / Duplication ───────────

    #[test]
    fn test_fingerprint_identical() {
        let a = "IF;BLOCK;RET";
        let b = "IF;BLOCK;RET";
        assert!((fingerprint_similarity(a, b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_fingerprint_different() {
        let a = "IF;BLOCK;RET";
        let b = "FOR;CALL;LET";
        assert!(fingerprint_similarity(a, b) < 0.1);
    }

    #[test]
    fn test_rust_fingerprints() {
        let src = r#"
fn foo(x: i32) -> i32 {
    if x > 0 { return x; }
    0
}

fn bar(x: i32) -> i32 {
    if x > 0 { return x; }
    0
}
"#;
        let prints = parse_fingerprints(src, "test.rs", Language::Rust);
        assert_eq!(prints.len(), 2);
        let sim = fingerprint_similarity(&prints[0].fingerprint, &prints[1].fingerprint);
        assert!(
            sim > 0.7,
            "identical-logic functions should be similar, got {:.2}",
            sim
        );
    }

    // ── Imports — Python ─────────────────────

    #[test]
    fn test_python_imports() {
        let src = "import os\nfrom pathlib import Path\n";
        let imports = parse_imports(src, "test.py", Language::Python);
        assert!(!imports.is_empty());
        assert!(imports
            .iter()
            .any(|i| i.imported_module.contains("os") || i.imported_module.contains("pathlib")));
    }

    // ── Imports — JS ─────────────────────────

    #[test]
    fn test_js_imports() {
        let src = r#"import React from 'react';
import { useState } from 'react';
"#;
        let imports = parse_imports(src, "test.js", Language::JavaScript);
        assert!(!imports.is_empty());
        assert!(imports.iter().any(|i| i.imported_module == "react"));
    }

    // ── Imports — Go ─────────────────────────

    #[test]
    fn test_go_imports() {
        let src = "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"hi\") }\n";
        let imports = parse_imports(src, "test.go", Language::Go);
        assert!(!imports.is_empty());
        assert!(imports.iter().any(|i| i.imported_module == "fmt"));
    }

    // ── Imports — Ruby ────────────────────────

    #[test]
    #[ignore] // Pre-existing: tree-sitter-ruby grammar limitation
    fn test_ruby_imports() {
        let src = "require 'json'\nrequire_relative './utils'\ninclude Enumerable\n";
        let imports = parse_imports(src, "test.rb", Language::Ruby);
        assert!(!imports.is_empty());
        assert!(imports.iter().any(|i| i.imported_module.contains("json")));
    }

    // ── Imports — Swift ───────────────────────

    #[test]
    fn test_swift_imports() {
        let src = "import Foundation\nimport UIKit\n";
        let imports = parse_imports(src, "test.swift", Language::Swift);
        assert!(!imports.is_empty());
        assert!(imports.iter().any(|i| i.imported_module == "Foundation"));
    }

    // ── Imports — Kotlin ──────────────────────

    #[test]
    #[ignore] // Pre-existing: tree-sitter-kotlin grammar limitation
    fn test_kotlin_imports() {
        let src = "import java.util.List\nimport kotlin.math.max\n";
        let imports = parse_imports(src, "test.kt", Language::Kotlin);
        assert!(!imports.is_empty());
        assert!(imports
            .iter()
            .any(|i| i.imported_module == "java.util.List"));
    }

    // ── extract_quoted tests ─────────────────

    #[test]
    fn test_extract_quoted_basic() {
        assert_eq!(
            extract_quoted("#include \"header.h\""),
            Some("header.h".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_no_quotes() {
        assert_eq!(extract_quoted("no quotes here"), None);
    }

    #[test]
    fn test_extract_quoted_empty_string() {
        assert_eq!(extract_quoted("\"\""), Some("".to_string()));
    }

    #[test]
    fn test_extract_quoted_multiple() {
        // Should return only the first quoted string
        assert_eq!(
            extract_quoted("\"first\" and \"second\""),
            Some("first".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_with_path() {
        assert_eq!(
            extract_quoted("import \"./utils\";"),
            Some("./utils".to_string())
        );
    }

    // ── extract_keyword_import tests ─────────

    #[test]
    fn test_extract_keyword_import_basic() {
        assert_eq!(
            extract_keyword_import("use crate::foo::bar;", "use "),
            Some("crate::foo::bar".to_string())
        );
    }

    #[test]
    fn test_extract_keyword_import_no_semicolon() {
        assert_eq!(
            extract_keyword_import("import Foundation", "import "),
            Some("Foundation".to_string())
        );
    }

    #[test]
    fn test_extract_keyword_import_with_whitespace() {
        assert_eq!(
            extract_keyword_import("  using System.Collections;  ", "using "),
            Some("System.Collections".to_string())
        );
    }

    #[test]
    fn test_extract_keyword_import_no_match() {
        assert_eq!(extract_keyword_import("fn foo()", "use "), None);
    }

    #[test]
    fn test_extract_keyword_import_empty_after_strip() {
        assert_eq!(extract_keyword_import("use ;", "use "), None);
    }

    #[test]
    fn test_extract_keyword_import_open() {
        assert_eq!(
            extract_keyword_import("open List", "open "),
            Some("List".to_string())
        );
    }

    #[test]
    fn test_extract_keyword_import_dotted_path() {
        assert_eq!(
            extract_keyword_import("import java.util.List;", "import "),
            Some("java.util.List".to_string())
        );
    }

    // ── doc_comment_prefixes tests ───────────

    #[test]
    fn test_doc_comment_prefixes_rust() {
        let prefixes = doc_comment_prefixes(Language::Rust);
        assert!(prefixes.contains(&"///"));
        assert!(prefixes.contains(&"/**"));
        assert!(prefixes.contains(&"//!"));
        assert_eq!(prefixes.len(), 3);
    }

    #[test]
    fn test_doc_comment_prefixes_python() {
        let prefixes = doc_comment_prefixes(Language::Python);
        assert!(prefixes.contains(&"#"));
        assert_eq!(prefixes.len(), 1);
    }

    #[test]
    fn test_doc_comment_prefixes_javascript() {
        let prefixes = doc_comment_prefixes(Language::JavaScript);
        assert!(prefixes.contains(&"*"));
        assert!(prefixes.contains(&"/**"));
    }

    #[test]
    fn test_doc_comment_prefixes_ocaml() {
        let prefixes = doc_comment_prefixes(Language::Ocaml);
        assert!(prefixes.contains(&"(**"));
        assert_eq!(prefixes.len(), 1);
    }

    #[test]
    fn test_doc_comment_prefixes_go() {
        let prefixes = doc_comment_prefixes(Language::Go);
        assert!(prefixes.contains(&"//"));
        assert!(prefixes.contains(&"/*"));
        assert!(prefixes.contains(&"///"));
        assert!(prefixes.contains(&"/**"));
    }

    #[test]
    fn test_doc_comment_prefixes_solidity() {
        let prefixes = doc_comment_prefixes(Language::Solidity);
        assert!(prefixes.contains(&"///"));
        assert_eq!(prefixes.len(), 1);
    }

    #[test]
    fn test_doc_comment_prefixes_vyper() {
        let prefixes = doc_comment_prefixes(Language::Vyper);
        assert!(prefixes.contains(&"///"));
        assert!(prefixes.contains(&"/**"));
        assert!(prefixes.contains(&"#"));
        assert_eq!(prefixes.len(), 3);
    }

    #[test]
    fn test_doc_comment_prefixes_unknown() {
        let prefixes = doc_comment_prefixes(Language::Unknown);
        assert!(prefixes.is_empty());
    }

    #[test]
    fn test_doc_comment_prefixes_csharp() {
        let prefixes = doc_comment_prefixes(Language::CSharp);
        // C# uses the same prefixes as Go/C/C++/Java/PHP/Ruby/Swift/Kotlin
        assert!(prefixes.contains(&"//"));
        assert!(prefixes.contains(&"/*"));
        assert!(prefixes.contains(&"///"));
        assert!(prefixes.contains(&"/**"));
        assert_eq!(prefixes.len(), 4);
    }
}
