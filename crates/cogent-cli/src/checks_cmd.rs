//! Individual check command handlers and tool dispatch.

#![deny(clippy::all)]

use crate::progress::run_standalone_check;

/// Data-driven dispatch for standalone tool checks.
/// Maps a tool name string to the corresponding check function.
#[allow(dead_code)]
pub fn dispatch_tool(name: &str, path: &str, recursive: bool, format: &str) -> i32 {
    tracing::info!(tool = name, path, recursive, format, "dispatching tool via string name");
    let p = path.to_string();
    match name {
        "crap" => run_standalone_check(name, format, move || cogent_engine::checks::check_crap(&p, recursive, &None, 30.0)),
        "debt" => run_standalone_check(name, format, move || cogent_engine::checks::check_debt(&p, recursive, 100)),
        "doccov" | "doc" => run_standalone_check(name, format, move || cogent_engine::checks::check_doc_coverage(&p, recursive, 50.0)),
        "complexity" | "complex" => run_standalone_check(name, format, move || cogent_engine::checks::check_complexity(&p, recursive, 10, 0)),
        "taint" => run_standalone_check(name, format, move || cogent_engine::checks::check_taint(&p, recursive, 0)),
        "dupfind" | "dup" => run_standalone_check(name, format, move || cogent_engine::checks::check_dupfind(&p, recursive, 5.0)),
        "riskmap" | "risk" => run_standalone_check(name, format, move || cogent_engine::checks::check_riskmap(&p, false, 10.0)),
        "coupling" => run_standalone_check(name, format, move || cogent_engine::checks::check_coupling(&p, 5)),
        "propcov" => run_standalone_check(name, format, move || cogent_engine::checks::check_propcov(&p, recursive, 0.0)),
        "fuzz" => run_standalone_check(name, format, move || cogent_engine::checks::check_fuzz(&p, recursive, 0)),
        "linelen" => run_standalone_check(name, format, move || cogent_engine::checks::check_linelen(&p, recursive, 0)),
        "halstead" => run_standalone_check(name, format, move || cogent_engine::checks::check_halstead(&p, recursive, 2.0)),
        "secrets" => run_standalone_check(name, format, move || cogent_engine::checks::check_secrets(&p, recursive, 0)),
        "deadcode" => run_standalone_check(name, format, move || cogent_engine::checks::check_deadcode(&p, recursive, 10)),
        "cohesion" => run_standalone_check(name, format, move || cogent_engine::checks::check_cohesion(&p, recursive, 5)),
        "comments" => run_standalone_check(name, format, move || cogent_engine::checks::check_comments(&p, recursive, 0.05)),
        "errhandle" => run_standalone_check(name, format, move || cogent_engine::checks::check_errhandle(&p, recursive, 50)),
        "typecov" => run_standalone_check(name, format, move || cogent_engine::checks::check_typecov(&p, recursive, 0.0)),
        "vulnscan" | "vuln" => run_standalone_check(name, format, move || cogent_engine::checks::check_vulnscan(&p, 0, 0)),
        "sast" => run_standalone_check(name, format, move || cogent_engine::checks::check_sast(&p, recursive, 0)),
        "crypto" => run_standalone_check(name, format, move || cogent_engine::checks::check_crypto(&p, recursive, 0)),
        "licenses" | "license" => run_standalone_check(name, format, move || cogent_engine::checks::check_licenses(&p, 0)),
        "access-control" | "accesscontrol" => run_standalone_check(name, format, move || cogent_engine::checks::check_access_control(&p, recursive, 0)),
        "supply-chain" | "supplychain" => run_standalone_check(name, format, move || cogent_engine::checks::check_supply_chain(&p, 0)),
        "outdated" => run_standalone_check(name, format, move || cogent_engine::checks::check_outdated(&p, 0)),
        _ => {
            eprintln!("Unknown tool: {}. Run 'cogent discover' to list available tools.", name);
            1
        }
    }
}
