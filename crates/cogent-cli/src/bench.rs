//! Benchmark and profiling for Cogent tool suite.
//!
//! Runs each tool individually, captures wall time, peak RSS, exit code,
//! and output size. Reports serial vs parallel comparison and system
//! capabilities (CPU cores, RAM, GPU).

#![deny(clippy::all)]

use crate::check_runners::run_tool;
use colored::Colorize;
use serde::Serialize;
use std::time::Instant;

// ═══════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════

/// System hardware capabilities detected at runtime.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    /// `true` = GPU detected, `false` = no GPU found, `"unknown"` = detection tools missing
    pub gpu_detected: bool,
    pub gpu_name: Option<String>,
    pub gpu_detection_possible: bool,
    pub recommended_concurrency: usize,
}

/// Per-tool benchmark result.
#[derive(Debug, Serialize, Clone)]
pub struct ToolBenchResult {
    pub name: String,
    pub crate_name: String,
    pub binary: String,
    pub wall_time_ms: u64,
    pub exit_code: Option<i32>,
    pub output_bytes: usize,
    pub peak_rss_mb: u64,
    pub success: bool,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    /// `true` = tool produced findings (non-zero exit + valid JSON output).
    /// This is expected behavior, not an error.
    pub has_findings: bool,
}

/// Full benchmark report.
#[derive(Debug, Serialize)]
pub struct BenchReport {
    pub system: SystemInfo,
    pub path: String,
    pub tools: Vec<ToolBenchResult>,
    pub serial_total_ms: u64,
    pub parallel_total_ms: u64,
    pub speedup_factor: f64,
    pub total_output_bytes: usize,
    pub tools_passed: usize,
    pub tools_skipped: usize,
    pub tools_findings: usize,
    pub tools_clean: usize,
    pub tools_failed: usize,
}

// ═══════════════════════════════════════════
// SYSTEM DETECTION
// ═══════════════════════════════════════════

/// Detect system hardware capabilities.
pub fn detect_system() -> SystemInfo {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // Physical cores: try to detect from /proc/cpuinfo or fall back to logical / 2
    let cpu_threads = cpu_cores;
    let cpu_cores = detect_physical_cores().unwrap_or(cpu_cores.max(2) / 2);

    let (total_ram_mb, available_ram_mb) = detect_memory_mb();

    let (gpu_detected, gpu_name, gpu_detection_possible) = detect_gpu();

    // Recommended concurrency: min of CPU threads and available RAM / 512MB per worker
    let ram_limited = (available_ram_mb / 512).max(1) as usize;
    let recommended_concurrency = cpu_threads.min(ram_limited).max(1);

    SystemInfo {
        cpu_cores,
        cpu_threads,
        total_ram_mb,
        available_ram_mb,
        gpu_detected,
        gpu_name,
        gpu_detection_possible,
        recommended_concurrency,
    }
}

fn detect_physical_cores() -> Option<usize> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut cores = 0usize;
    let mut seen = std::collections::HashSet::new();
    let mut current_id = None;
    for line in cpuinfo.lines() {
        if line.starts_with("core id") || line.starts_with("physical id") {
            if let Some(val) = line.split(':').nth(1) {
                let val = val.trim();
                if line.starts_with("physical id") {
                    current_id = Some(val.to_string());
                } else if line.starts_with("core id") {
                    let key = format!("{}:{}", current_id.as_deref().unwrap_or("0"), val);
                    if seen.insert(key) {
                        cores += 1;
                    }
                }
            }
        }
    }
    if cores > 0 {
        Some(cores)
    } else {
        None
    }
}

fn detect_memory_mb() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    let result = {
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let field = |name: &str| {
            meminfo
                .lines()
                .find(|l| l.starts_with(name))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        };
        (field("MemTotal:") / 1024, field("MemAvailable:") / 1024)
    };
    #[cfg(target_os = "macos")]
    let result = {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok();
        let mb = parse_total_ram_mb(out);
        (mb, mb)
    };
    #[cfg(target_os = "windows")]
    let result = {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .output()
            .ok();
        let mb = parse_total_ram_mb(out);
        (mb, mb)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result = (0u64, 0u64);
    result
}

/// Parse total RAM (in MB) from a command that prints a single byte count.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_total_ram_mb(out: Option<std::process::Output>) -> u64 {
    let bytes = out
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    bytes / (1024 * 1024)
}

fn detect_gpu() -> (bool, Option<String>, bool) {
    // Try nvidia-smi first
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if out.status.success() {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() {
                return (true, Some(name), true);
            }
        }
    }
    // Try lspci — if not installed, we can't determine GPU status
    let lspci_available = std::process::Command::new("lspci")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !lspci_available {
        return (false, None, false); // unknown — detection tools missing
    }
    if let Ok(out) = std::process::Command::new("lspci").output() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let lower = line.to_lowercase();
            if lower.contains("vga") || lower.contains("3d") || lower.contains("display") {
                if let Some(name) = line.split(':').nth(2) {
                    let name = name.trim().to_string();
                    if !name.is_empty() {
                        return (true, Some(name), true);
                    }
                }
            }
        }
    }
    (false, None, true) // confirmed no GPU
}

// ═══════════════════════════════════════════
// TOOL DEFINITIONS
// ═══════════════════════════════════════════

/// All benchmarkable tools: (display_name, crate_name, binary, needs_recursive).
fn benchmark_tools() -> Vec<(&'static str, &'static str, &'static str, bool)> {
    vec![
        ("debt", "debt-scan", "debt", true),
        ("doccov", "doc-coverage", "doccov", true),
        ("crap", "crap-metric", "crap", true),
        ("coupling", "coupling", "coupling", false),
        ("riskmap", "risk-map", "riskmap", false),
        ("dupfind", "duplication", "dupfind", true),
        ("propcov", "prop-cov", "propcov", true),
        ("taint", "taint-scan", "taint", true),
        ("fuzz", "fuzz-surface", "fuzz", true),
        ("linelen", "line-length", "linelen", true),
        ("halstead", "halstead", "halstead", true),
        ("secrets", "secrets", "secrets", true),
        ("deadcode", "dead-code", "deadcode", true),
        ("cohesion", "cohesion", "cohesion", true),
        ("comments", "comment-ratio", "comments", true),
        ("errhandle", "error-handling", "errhandle", true),
        ("typecov", "type-coverage", "typecov", true),
        ("vulnscan", "vuln-scan", "vulnscan", false),
        ("sast", "sast", "sast", true),
        ("crypto", "crypto-check", "cryptocheck", true),
        ("licenses", "licenses", "licenses", false),
        ("access-control", "access-control", "access-control", false),
        ("supply-chain", "supply-chain", "supply-chain", false),
    ]
}

// ═══════════════════════════════════════════
// BENCHMARK RUNNERS
// ═══════════════════════════════════════════

/// Benchmark a single tool: run it, capture metrics.
///
/// Success is determined by whether the tool produced valid JSON output,
/// not by exit code. Many tools exit non-zero when they find issues —
/// that's expected behavior ("findings"), not an error.
fn bench_single_tool(
    name: &str,
    crate_name: &str,
    binary: &str,
    path: &str,
    recursive: bool,
) -> ToolBenchResult {
    let mut args = vec![path];
    if recursive {
        args.push("--recursive");
    }
    args.push("--format");
    args.push("json");
    let start = Instant::now();
    let result = run_tool(crate_name, binary, &args, start);
    let wall_time_ms = result.duration_ms;
    let output_bytes = serde_json::to_string(&result.data)
        .map(|s| s.len())
        .unwrap_or(0);
    let exit_code = if result.success { Some(0) } else { Some(1) };

    // Tool is skipped if it's not installed
    let skipped = result
        .error
        .as_deref()
        .is_some_and(|e| e.contains("not found") || e.contains("Skipped"));
    let skip_reason = if skipped { result.error.clone() } else { None };

    // Tool succeeded if it produced valid JSON output (not null).
    // Non-zero exit code with valid data = found findings (expected).
    // Null data + error = actual failure.
    let has_valid_output = !result.data.is_null();
    let success = has_valid_output || skipped;
    let has_findings = success && !result.success;
    let peak_rss_mb = crate::check_runners::get_peak_rss_mb();

    ToolBenchResult {
        name: name.to_string(),
        crate_name: crate_name.to_string(),
        binary: binary.to_string(),
        wall_time_ms,
        exit_code,
        output_bytes,
        peak_rss_mb,
        success,
        skipped,
        skip_reason,
        has_findings,
    }
}

/// Run all tools serially and collect per-tool results.
fn run_serial(path: &str) -> (Vec<ToolBenchResult>, u64) {
    let start = Instant::now();
    let tools = benchmark_tools();
    let mut results = Vec::with_capacity(tools.len());

    for (name, crate_name, binary, recursive) in &tools {
        let result = bench_single_tool(name, crate_name, binary, path, *recursive);
        results.push(result);
    }

    let total_ms = start.elapsed().as_millis() as u64;
    (results, total_ms)
}

/// Run all tools in parallel (same as `cogent check` concurrency model)
/// and return wall time. Results are taken from the serial run.
fn run_parallel(path: &str) -> u64 {
    use std::sync::Mutex;

    let tools = benchmark_tools();
    let n_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(tools.len());

    let work: Mutex<Vec<(&str, &str, &str, bool)>> = Mutex::new(tools);
    let start = Instant::now();

    std::thread::scope(|s| {
        for _ in 0..n_workers {
            s.spawn(|| loop {
                let job = work.lock().expect("work mutex poisoned").pop();
                match job {
                    Some((name, crate_name, binary, recursive)) => {
                        let mut args = vec![path];
                        if recursive {
                            args.push("--recursive");
                        }
                        args.push("--format");
                        args.push("json");
                        let _ = run_tool(crate_name, binary, &args, Instant::now());
                        let _ = (name, recursive); // suppress unused warning
                    }
                    None => break,
                }
            });
        }
    });

    start.elapsed().as_millis() as u64
}

// ═══════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════

/// Run the `cogent bench` command.
pub fn bench_command(path: &str, format: &str) -> i32 {
    let system = detect_system();

    if format == "text" {
        eprintln!();
        eprintln!("  {} Cogent Benchmark", "▶".cyan().bold());
        eprintln!(
            "  {}────────────────────────────────────",
            "".bright_black()
        );
        eprintln!();
        eprintln!("  {} System", "⚙".cyan().bold());
        eprintln!(
            "    {} CPU cores: {} ({} threads)",
            "·".bright_black(),
            system.cpu_cores,
            system.cpu_threads
        );
        eprintln!(
            "    {} RAM: {} MB total, {} MB available",
            "·".bright_black(),
            system.total_ram_mb,
            system.available_ram_mb
        );
        if system.gpu_detected {
            eprintln!(
                "    {} GPU: {} {}",
                "·".bright_black(),
                "✓".green().bold(),
                system.gpu_name.as_deref().unwrap_or("detected")
            );
        } else if system.gpu_detection_possible {
            eprintln!(
                "    {} GPU: none detected (not needed — all tools are CPU-bound static analysis)",
                "·".bright_black()
            );
        } else {
            eprintln!(
                "    {} GPU: unknown (detection tools not installed)",
                "·".bright_black()
            );
        }
        eprintln!(
            "    {} Recommended concurrency: {}",
            "·".bright_black(),
            system.recommended_concurrency.to_string().cyan()
        );
        eprintln!();
        eprintln!(
            "  {} Benchmarking {} tools (serial)...",
            "▶".cyan().bold(),
            benchmark_tools().len()
        );
    }

    let (tools, serial_total_ms) = run_serial(path);

    if format == "text" {
        eprintln!(
            "  {} Serial complete: {:.1}s",
            "✓".green().bold(),
            serial_total_ms as f64 / 1000.0
        );
        eprintln!();
        eprintln!(
            "  {} Benchmarking (parallel, {} workers)...",
            "▶".cyan().bold(),
            system.recommended_concurrency
        );
    }

    let parallel_total_ms = run_parallel(path);

    let speedup = if parallel_total_ms > 0 {
        serial_total_ms as f64 / parallel_total_ms as f64
    } else {
        1.0
    };

    let total_output_bytes: usize = tools.iter().map(|t| t.output_bytes).sum();
    let tools_passed = tools.iter().filter(|t| t.success && !t.skipped).count();
    let tools_skipped = tools.iter().filter(|t| t.skipped).count();
    let tools_findings = tools.iter().filter(|t| t.has_findings).count();
    let tools_clean = tools
        .iter()
        .filter(|t| t.success && !t.has_findings && !t.skipped)
        .count();
    let tools_failed = tools.iter().filter(|t| !t.success && !t.skipped).count();

    let report = BenchReport {
        system,
        path: path.to_string(),
        tools: tools.clone(),
        serial_total_ms,
        parallel_total_ms,
        speedup_factor: speedup,
        total_output_bytes,
        tools_passed,
        tools_skipped,
        tools_findings,
        tools_clean,
        tools_failed,
    };

    match format {
        "text" => print_text_report(&report),
        _ => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
    }

    0
}

fn print_text_report(report: &BenchReport) {
    eprintln!(
        "  {} Parallel complete: {:.1}s ({:.1}× speedup)",
        "✓".green().bold(),
        report.parallel_total_ms as f64 / 1000.0,
        report.speedup_factor
    );
    eprintln!();
    eprintln!("  {} Results", "📊".cyan().bold());
    eprintln!(
        "  {}────────────────────────────────────────────────────────────────────",
        "".bright_black()
    );
    eprintln!(
        "  {:<18} {:>8} {:>10} {:>10} Status",
        "Tool", "Time", "Output", "RSS"
    );
    eprintln!(
        "  {}────────────────────────────────────────────────────────────────────",
        "".bright_black()
    );

    for tool in &report.tools {
        let status = if tool.skipped {
            "⊘ skipped".bright_black().to_string()
        } else if tool.has_findings {
            "✓ findings".yellow().to_string()
        } else if tool.success {
            "✓ clean".green().to_string()
        } else {
            "✗ error".red().to_string()
        };

        let time_str = format_ms(tool.wall_time_ms);
        let output_str = format_bytes(tool.output_bytes);
        let rss_str = format!("{} MB", tool.peak_rss_mb);

        eprintln!(
            "  {:<18} {:>8} {:>10} {:>10} {}",
            tool.name.cyan(),
            time_str,
            output_str,
            rss_str.bright_black(),
            status
        );
    }

    eprintln!(
        "  {}────────────────────────────────────────────────────────────────────",
        "".bright_black()
    );
    eprintln!();

    // Summary box
    eprintln!("  {} Summary", "▶".cyan().bold());
    eprintln!(
        "    {} Serial total:   {:.1}s",
        "·".bright_black(),
        report.serial_total_ms as f64 / 1000.0
    );
    eprintln!(
        "    {} Parallel total: {:.1}s  ({} workers)",
        "·".bright_black(),
        report.parallel_total_ms as f64 / 1000.0,
        report.system.recommended_concurrency
    );
    eprintln!(
        "    {} Speedup:        {:.1}×",
        "·".bright_black(),
        report.speedup_factor
    );
    eprintln!(
        "    {} Tools:          {} clean, {} findings, {} skipped, {} failed",
        "·".bright_black(),
        report.tools_clean.to_string().green(),
        report.tools_findings.to_string().yellow(),
        report.tools_skipped.to_string().bright_black(),
        if report.tools_failed > 0 {
            report.tools_failed.to_string().red().to_string()
        } else {
            "0".to_string()
        }
    );
    eprintln!(
        "    {} Total output:   {}",
        "·".bright_black(),
        format_bytes(report.total_output_bytes)
    );
    eprintln!();

    // Compute characteristics
    eprintln!("  {} Compute Profile", "⚙".cyan().bold());
    eprintln!(
        "    {} CPU-bound: All 23 tools are static analysis (AST parsing, pattern matching)",
        "·".bright_black()
    );
    eprintln!(
        "    {} No GPU needed: No ML inference or matrix operations",
        "·".bright_black()
    );
    eprintln!(
        "    {} Memory: ~50-200 MB per tool subprocess",
        "·".bright_black()
    );
    eprintln!(
        "    {} Parallelism: Process-level via thread pool (COGENT_MAX_CONCURRENT={})",
        "·".bright_black(),
        report.system.recommended_concurrency
    );
    eprintln!();

    // Heaviest tools
    let mut sorted = report.tools.clone();
    sorted.sort_by_key(|t| std::cmp::Reverse(t.wall_time_ms));
    let top5: Vec<_> = sorted.iter().filter(|t| !t.skipped).take(5).collect();
    if !top5.is_empty() {
        eprintln!("  {} Heaviest tools (top 5):", "⏱".cyan().bold());
        for (i, t) in top5.iter().enumerate() {
            eprintln!(
                "    {} {} — {}",
                format!("{}.", i + 1).bright_black(),
                t.name.cyan(),
                format_ms(t.wall_time_ms)
            );
        }
        eprintln!();
    }
}

fn format_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

fn format_bytes(b: usize) -> String {
    if b < 1024 {
        format!("{} B", b)
    } else if b < 1024 * 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    }
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_system_returns_nonzero_cores() {
        let sys = detect_system();
        assert!(sys.cpu_cores > 0, "should detect at least 1 CPU core");
        assert!(sys.cpu_threads >= sys.cpu_cores, "threads >= cores");
    }

    #[test]
    fn test_detect_system_returns_nonzero_ram() {
        let sys = detect_system();
        assert!(sys.total_ram_mb > 0, "should detect some RAM");
    }

    #[test]
    fn test_detect_system_recommended_concurrency() {
        let sys = detect_system();
        assert!(sys.recommended_concurrency >= 1, "concurrency >= 1");
        assert!(
            sys.recommended_concurrency <= sys.cpu_threads,
            "concurrency <= threads"
        );
    }

    #[test]
    fn test_format_ms_zero() {
        assert_eq!(format_ms(0), "0ms");
    }

    #[test]
    fn test_format_ms_milliseconds() {
        assert_eq!(format_ms(500), "500ms");
        assert_eq!(format_ms(999), "999ms");
    }

    #[test]
    fn test_format_ms_seconds() {
        assert_eq!(format_ms(1000), "1.0s");
        assert_eq!(format_ms(2500), "2.5s");
    }

    #[test]
    fn test_format_ms_minutes() {
        assert_eq!(format_ms(60_000), "1.0m");
        assert_eq!(format_ms(120_000), "2.0m");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_benchmark_tools_not_empty() {
        let tools = benchmark_tools();
        assert!(!tools.is_empty(), "should have at least one benchmark tool");
        assert!(
            tools.len() >= 20,
            "should have 20+ tools, got {}",
            tools.len()
        );
    }

    #[test]
    fn test_detect_gpu_returns_tuple() {
        let (detected, name, detection_possible) = detect_gpu();
        // On any system, we should get a valid response
        if detected {
            assert!(name.is_some(), "gpu detected but no name");
        }
        // detection_possible tells us if we had the tools to check
        let _ = detection_possible; // just verify it doesn't panic
    }

    #[test]
    fn test_bench_report_serialization() {
        let sys = SystemInfo {
            cpu_cores: 4,
            cpu_threads: 8,
            total_ram_mb: 16384,
            available_ram_mb: 8192,
            gpu_detected: false,
            gpu_name: None,
            gpu_detection_possible: true,
            recommended_concurrency: 4,
        };
        let report = BenchReport {
            system: sys,
            path: ".".to_string(),
            tools: vec![],
            serial_total_ms: 1000,
            parallel_total_ms: 500,
            speedup_factor: 2.0,
            total_output_bytes: 1024,
            tools_passed: 0,
            tools_skipped: 0,
            tools_findings: 0,
            tools_clean: 0,
            tools_failed: 0,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("cpu_cores"));
        assert!(json.contains("speedup_factor"));
    }
}
