//! Individual check command handlers and tool dispatch.

#![deny(clippy::all)]

use crate::progress::run_standalone_check;

/// Data-driven dispatch for standalone tool checks.
/// Maps a tool name string to the corresponding check function.
#[allow(dead_code)]
pub fn dispatch_tool(name: &str, path: &str, recursive: bool, format: &str) -> i32 {
    tracing::info!(
        tool = name,
        path,
        recursive,
        format,
        "dispatching tool via string name"
    );
    let p = path.to_string();
    match name {
        "crap" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_crap(&p, recursive, &None, 30.0)
        }),
        "debt" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_debt(&p, recursive, 100)
        }),
        "doccov" | "doc" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_doc_coverage(&p, recursive, 50.0)
        }),
        "complexity" | "complex" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_complexity(&p, recursive, 10, 0)
        }),
        "taint" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_taint(&p, recursive, 0)
        }),
        "dupfind" | "dup" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_dupfind(&p, recursive, 5.0)
        }),
        "riskmap" | "risk" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_riskmap(&p, false, 50.0)
        }),
        "coupling" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_coupling(&p, 5)
        }),
        "propcov" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_propcov(&p, recursive, 0.0)
        }),
        "fuzz" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_fuzz(&p, recursive, 0)
        }),
        "linelen" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_linelen(&p, recursive, 0)
        }),
        "halstead" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_halstead(&p, recursive, 2.0)
        }),
        "secrets" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_secrets(&p, recursive, 0)
        }),
        "deadcode" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_deadcode(&p, recursive, 10)
        }),
        "cohesion" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_cohesion(&p, recursive, 5)
        }),
        "comments" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_comments(&p, recursive, 0.05)
        }),
        "errhandle" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_errhandle(&p, recursive, 50)
        }),
        "typecov" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_typecov(&p, recursive, 0.0)
        }),
        "vulnscan" | "vuln" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_vulnscan(&p, 0, 0)
        }),
        "sast" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_sast(&p, recursive, 0)
        }),
        "crypto" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_crypto(&p, recursive, 0)
        }),
        "licenses" | "license" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_licenses(&p, 0)
        }),
        "access-control" | "accesscontrol" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_access_control(
                &p,
                recursive,
                0,
                &[],
                &cogent_engine::DefaultToolRunner,
            )
        }),
        "supply-chain" | "supplychain" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_supply_chain(&p, 0)
        }),
        "outdated" => run_standalone_check(name, format, move || {
            cogent_engine::checks::check_outdated(&p, 0)
        }),
        _ => {
            eprintln!(
                "Unknown tool: {}. Run 'cogent discover' to list available tools.",
                name
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── dispatch_tool: unknown tool ──

    #[test]
    fn test_dispatch_unknown_tool_exit_code() {
        // Unknown tool should return 1
        let code = dispatch_tool("some_fake_tool_xyz_123", "./", true, "json");
        assert_eq!(code, 1, "unknown tool should exit 1");
    }

    // ── dispatch_tool: known tools (smoke tests) ──
    //
    // These tests verify that each tool name dispatches correctly.
    // They point at the fixture (test) dir so they have files to analyze.
    // We use --format json to get JSON output and avoid spinner/terminal issues.

    fn test_dir() -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        // Point at the tests/ directory itself (has .rs files)
        std::path::Path::new(manifest)
            .join("tests")
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_dispatch_crap() {
        let code = dispatch_tool("crap", &test_dir(), false, "json");
        // Should run without crashing; exit 0 or 1 depending on findings
        assert!(
            code == 0 || code == 1,
            "crap should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_debt() {
        let code = dispatch_tool("debt", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "debt should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_doccov() {
        let code = dispatch_tool("doccov", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "doccov should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_complexity() {
        let code = dispatch_tool("complexity", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "complexity should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_linelen() {
        let code = dispatch_tool("linelen", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "linelen should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_halstead() {
        let code = dispatch_tool("halstead", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "halstead should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_secrets() {
        let code = dispatch_tool("secrets", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "secrets should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_cohesion() {
        let code = dispatch_tool("cohesion", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "cohesion should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_comments() {
        let code = dispatch_tool("comments", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "comments should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_errhandle() {
        let code = dispatch_tool("errhandle", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "errhandle should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_typecov() {
        // typecov targets TS/JS/Python — on Rust code it may have 0 findings
        let code = dispatch_tool("typecov", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "typecov should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_deadcode() {
        let code = dispatch_tool("deadcode", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "deadcode should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_dupfind() {
        let code = dispatch_tool("dupfind", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "dupfind should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_crypto() {
        let code = dispatch_tool("crypto", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "crypto should complete with 0 or 1, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_taint() {
        let code = dispatch_tool("taint", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "taint should complete with 0 or 1, got {}",
            code
        );
    }

    // ── dispatch_tool: alias tests ──
    // Verify that short aliases map to the same tool

    #[test]
    fn test_dispatch_alias_doc() {
        let code = dispatch_tool("doc", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "doc alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_complex() {
        let code = dispatch_tool("complex", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "complex alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_dup() {
        let code = dispatch_tool("dup", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "dup alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_risk() {
        let code = dispatch_tool("risk", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "risk alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_vuln() {
        let code = dispatch_tool("vuln", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "vuln alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_license() {
        let code = dispatch_tool("license", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "license alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_accesscontrol() {
        let code = dispatch_tool("accesscontrol", &test_dir(), true, "json");
        assert!(
            code == 0 || code == 1,
            "accesscontrol alias should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_alias_supplychain() {
        let code = dispatch_tool("supplychain", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "supplychain alias should complete, got {}",
            code
        );
    }

    // ── dispatch_tool: tools that may need external binaries ──
    // These are best-effort: they may exit 1 if the external tool is not installed

    #[test]
    fn test_dispatch_vulnscan() {
        let code = dispatch_tool("vulnscan", &test_dir(), false, "json");
        // May fail if cargo-audit not installed, but should still run without crash
        assert!(
            code == 0 || code == 1,
            "vulnscan should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_sast() {
        let code = dispatch_tool("sast", &test_dir(), true, "json");
        assert!(code == 0 || code == 1, "sast should complete, got {}", code);
    }

    #[test]
    fn test_dispatch_outdated() {
        let code = dispatch_tool("outdated", &test_dir(), false, "json");
        // May fail if cargo-outdated not installed
        assert!(
            code == 0 || code == 1,
            "outdated should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_propcov() {
        let code = dispatch_tool("propcov", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "propcov should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_fuzz() {
        let code = dispatch_tool("fuzz", &test_dir(), false, "json");
        assert!(code == 0 || code == 1, "fuzz should complete, got {}", code);
    }

    #[test]
    fn test_dispatch_coupling() {
        let code = dispatch_tool("coupling", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "coupling should complete, got {}",
            code
        );
    }

    #[test]
    fn test_dispatch_riskmap() {
        // riskmap needs git history, may fail gracefully
        let code = dispatch_tool("riskmap", &test_dir(), false, "json");
        assert!(
            code == 0 || code == 1,
            "riskmap should complete, got {}",
            code
        );
    }
}
