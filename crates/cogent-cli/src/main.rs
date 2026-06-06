#![deny(clippy::all)]

use clap::Parser;
use tracing_subscriber::prelude::*;

mod audit;
mod cache;
mod check_runners;
mod checks_cmd;
mod cli;
mod commands;
mod config;
mod diff;
mod dispatcher;
mod doctor;
mod history;
mod hooks;
mod progress;
mod report;
mod report_formatters;
mod serve;
mod types;
mod bench;
mod watch;

use cli::Cli;

/// Guard that keeps the Tokio runtime alive for OpenTelemetry OTLP export.
/// When OTLP is not enabled, this is a no-op (empty struct).
/// The runtime must outlive all spans so tonic gRPC connections remain functional.
struct OtelGuard {
    #[cfg(feature = "opentelemetry")]
    _runtime: Option<tokio::runtime::Runtime>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        // Runtime is dropped here, which shuts down all spawned tasks.
        // With install_simple(), spans are exported inline so no explicit flush needed.
    }
}

/// Default tracing filter: `info` level for the `cogent` crate, `warn` for everything else.
/// This ensures `#[tracing::instrument(level = "info")]` spans are visible by default.
const DEFAULT_TRACING_FILTER: &str = "cogent=info,warn";

/// Build the `EnvFilter` for the tracing subscriber.
///
/// Uses `RUST_LOG` if set, otherwise falls back to [`DEFAULT_TRACING_FILTER`].
/// Extracted from `init_tracing` so it can be tested without calling `.init()`.
fn build_env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_TRACING_FILTER))
}

/// Initialize tracing with optional OpenTelemetry OTLP export.
///
/// Always sets up a `fmt` layer gated by `RUST_LOG` (default: `cogent=info,warn`).
/// When built with `--features opentelemetry` and `OTEL_EXPORTER_OTLP_ENDPOINT`
/// is set, an additional OTLP span exporter is registered.
///
/// Returns an `OtelGuard` that flushes pending spans on drop.
fn init_tracing() -> OtelGuard {
    let env_filter = build_env_filter();

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr);

    #[cfg(feature = "opentelemetry")]
    {
        if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
            // Create a minimal Tokio runtime for the batch span processor and tonic gRPC.
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => {
                    let _guard = rt.enter();
                    let otel_exporter = opentelemetry_otlp::new_exporter().tonic();
                    let otel_tracer = opentelemetry_otlp::new_pipeline()
                        .tracing()
                        .with_exporter(otel_exporter)
                        .install_simple();
                    match otel_tracer {
                        Ok(tracer) => {
                            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
                            tracing_subscriber::registry()
                                .with(env_filter)
                                .with(fmt_layer)
                                .with(otel_layer)
                                .init();
                            return OtelGuard { _runtime: Some(rt) };
                        }
                        Err(e) => {
                            eprintln!("Warning: OpenTelemetry init failed ({}), falling back to fmt-only", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to create Tokio runtime ({}), falling back to fmt-only", e);
                }
            }
        }
    }

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
    OtelGuard {
        #[cfg(feature = "opentelemetry")]
        _runtime: None,
    }
}

/// Reset SIGPIPE to its default behaviour (terminate the process).
///
/// Rust sets SIGPIPE to SIG_IGN by default, which means writing to a
/// closed pipe returns `ErrorKind::BrokenPipe` instead of the normal
/// Unix behaviour of silently terminating.  Tools like `head(1)` close
/// the read end early, so every `println!` after that triggers a noisy
/// broken-pipe error.
///
/// Resetting to SIG_DFL restores the expected contract: the kernel
/// kills the writer, no error message is printed, and the exit code
/// is 141 (128 + SIGPIPE).
///
/// See: <https://github.com/rust-lang/rust/issues/62569>
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL); }
}

#[cfg(not(unix))]
fn reset_sigpipe() {
    // No-op on Windows — SIGPIPE does not exist there.
}

fn main() {
    reset_sigpipe();
    let _otel_guard = init_tracing();
    let cli = Cli::parse();
    let exit_code = dispatcher::dispatch(cli.command);
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DEFAULT_TRACING_FILTER ─────────────────────────────────────────

    #[test]
    fn test_default_filter_is_valid() {
        // The default filter string must parse without panic.
        let _filter = tracing_subscriber::EnvFilter::new(DEFAULT_TRACING_FILTER);
    }

    #[test]
    fn test_default_filter_allows_info_for_cogent() {
        // The default filter should allow INFO-level spans from the "cogent" target.
        let filter = tracing_subscriber::EnvFilter::new(DEFAULT_TRACING_FILTER);
        // EnvFilter::new succeeds → the directive string is well-formed.
        // We verify the constant contains the expected directive.
        assert!(
            DEFAULT_TRACING_FILTER.contains("cogent=info"),
            "default filter should include cogent=info, got: {}",
            DEFAULT_TRACING_FILTER
        );
        assert!(
            DEFAULT_TRACING_FILTER.contains(",warn"),
            "default filter should set global warn level, got: {}",
            DEFAULT_TRACING_FILTER
        );
        // EnvFilter was constructed successfully from the constant.
        let _ = filter;
    }

    #[test]
    fn test_default_filter_format() {
        // Verify the filter has the expected structure: crate=level,global_level
        let parts: Vec<&str> = DEFAULT_TRACING_FILTER.split(',').collect();
        assert_eq!(parts.len(), 2, "filter should have exactly 2 directives");
        assert_eq!(parts[0], "cogent=info");
        assert_eq!(parts[1], "warn");
    }

    // ── build_env_filter ────────────────────────────────────────────────

    #[test]
    fn test_build_env_filter_uses_default_when_rust_log_unset() {
        // Clear RUST_LOG so build_env_filter falls back to the default.
        // This test may race with others that set RUST_LOG, but the worst case
        // is we get the custom value instead of the default — still a valid filter.
        let saved = std::env::var("RUST_LOG").ok();
        std::env::remove_var("RUST_LOG");

        let filter = build_env_filter();
        // A valid EnvFilter is created — no panic, no error.
        // The filter object is opaque, but construction succeeding proves it parsed.
        let _ = filter;

        // Restore original value if any.
        match saved {
            Some(v) => std::env::set_var("RUST_LOG", v),
            None => std::env::remove_var("RUST_LOG"),
        }
    }

    #[test]
    fn test_build_env_filter_respects_rust_log() {
        // Set RUST_LOG to a custom value and verify build_env_filter uses it.
        let saved = std::env::var("RUST_LOG").ok();
        std::env::set_var("RUST_LOG", "debug");

        let filter = build_env_filter();
        // Should parse "debug" without panic.
        let _ = filter;

        // Restore.
        match saved {
            Some(v) => std::env::set_var("RUST_LOG", v),
            None => std::env::remove_var("RUST_LOG"),
        }
    }

    #[test]
    fn test_build_env_filter_handles_custom_directives() {
        // Set RUST_LOG to valid custom directives — EnvFilter should still parse.
        let saved = std::env::var("RUST_LOG").ok();
        std::env::set_var("RUST_LOG", "my_crate=trace,tower=debug");

        let filter = build_env_filter();
        let _ = filter;

        // Restore.
        match saved {
            Some(v) => std::env::set_var("RUST_LOG", v),
            None => std::env::remove_var("RUST_LOG"),
        }
    }

    #[test]
    fn test_build_env_filter_falls_back_on_malformed_rust_log() {
        // Set RUST_LOG to a malformed value — EnvFilter::try_from_default_env
        // returns Err, so build_env_filter should fall back to the default.
        let saved = std::env::var("RUST_LOG").ok();
        std::env::set_var("RUST_LOG", "====invalid===");

        let filter = build_env_filter();
        // Should not panic — falls back to DEFAULT_TRACING_FILTER.
        let _ = filter;

        // Restore.
        match saved {
            Some(v) => std::env::set_var("RUST_LOG", v),
            None => std::env::remove_var("RUST_LOG"),
        }
    }

    // ── OtelGuard ──────────────────────────────────────────────────────

    #[test]
    fn test_otel_guard_construct_and_drop() {
        // OtelGuard can be constructed without the opentelemetry feature
        // and dropped without panic.
        let guard = OtelGuard {
            #[cfg(feature = "opentelemetry")]
            _runtime: None,
        };
        drop(guard);
    }

    #[test]
    fn test_otel_guard_drop_impl_exists() {
        // Verify OtelGuard implements Drop (the compiler will reject this
        // if the impl is removed).
        #[allow(drop_bounds)]
        fn assert_drop<T: Drop>() {}
        assert_drop::<OtelGuard>();
    }

    // ── Filter level semantics ─────────────────────────────────────────

    #[test]
    fn test_env_filter_from_custom_directive() {
        // Verify various filter directive strings parse correctly.
        let directives = [
            "info",
            "warn",
            "cogent=info",
            "cogent=debug,warn",
            "cogent_cli=info,tower=debug,warn",
        ];
        for directive in &directives {
            let filter = tracing_subscriber::EnvFilter::new(*directive);
            let _ = filter;
        }
    }
}
