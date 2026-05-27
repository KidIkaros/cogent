/// Memory monitoring and resource limiting utilities.
/// Provides safe memory threshold checking and auto-terminate capabilities.
use std::time::{Duration, Instant};

/// Memory usage information for the current process.
#[derive(Debug, Clone)]
pub struct MemoryUsage {
    /// Resident Set Size (RSS) in bytes
    pub rss_bytes: u64,
    /// Virtual memory size in bytes
    pub vsize_bytes: u64,
}

/// Memory monitor that tracks usage and enforces safe thresholds.
pub struct MemoryMonitor {
    /// Maximum allowed RSS in bytes
    pub max_rss_bytes: u64,
    /// Warning threshold (percentage of max)
    warn_threshold: f64,
    /// Last check timestamp
    last_check: Instant,
    /// Check interval
    check_interval: Duration,
    /// Whether to auto-terminate on threshold exceed
    auto_terminate: bool,
}

impl MemoryMonitor {
    /// Create a new memory monitor.
    ///
    /// # Arguments
    /// * `max_rss_bytes` - Maximum allowed RSS in bytes (0 for no limit)
    /// * `warn_threshold` - Warning threshold as percentage of max (0.0-1.0)
    /// * `check_interval` - How often to check memory usage
    /// * `auto_terminate` - Whether to auto-terminate when threshold exceeded
    pub fn new(
        max_rss_bytes: u64,
        warn_threshold: f64,
        check_interval: Duration,
        auto_terminate: bool,
    ) -> Self {
        Self {
            max_rss_bytes,
            warn_threshold,
            last_check: Instant::now(),
            check_interval,
            auto_terminate,
        }
    }

    /// Create a monitor from environment variables.
    /// - `QUALITY_MAX_MEMORY_MB`: Maximum memory in MB (default: 80% of system RAM)
    /// - `QUALITY_WARN_THRESHOLD`: Warning threshold percentage (default: 0.8)
    /// - `QUALITY_AUTO_TERMINATE`: Auto-terminate on exceed (default: true)
    pub fn from_env() -> Self {
        let max_mb = std::env::var("QUALITY_MAX_MEMORY_MB")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| {
                // Default to 80% of system RAM
                if let Ok(total_kb) = Self::get_system_memory_kb() {
                    (total_kb * 1024 / 100 * 80) / 1024 // Convert KB to MB, take 80%
                } else {
                    12 * 1024 // Fallback to 12GB
                }
            });

        let warn_threshold = std::env::var("QUALITY_WARN_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.8);

        let auto_terminate = std::env::var("QUALITY_AUTO_TERMINATE")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);

        Self::new(
            max_mb * 1024 * 1024,
            warn_threshold.clamp(0.5, 0.95),
            Duration::from_secs(5),
            auto_terminate,
        )
    }

    /// Check current memory usage and enforce limits.
    ///
    /// Returns `Ok(usage)` if within limits, `Err(usage)` if threshold exceeded.
    /// If auto_terminate is enabled, will exit the process on threshold exceed.
    pub fn check(&mut self) -> Result<MemoryUsage, MemoryUsage> {
        // Rate limit checks
        if self.last_check.elapsed() < self.check_interval {
            return Ok(MemoryUsage {
                rss_bytes: 0,
                vsize_bytes: 0,
            });
        }
        self.last_check = Instant::now();

        let usage = Self::get_current_usage().unwrap_or(MemoryUsage {
            rss_bytes: 0,
            vsize_bytes: 0,
        });

        if self.max_rss_bytes == 0 {
            return Ok(usage);
        }

        let ratio = usage.rss_bytes as f64 / self.max_rss_bytes as f64;

        // Log warnings at thresholds
        if ratio >= 0.9 {
            eprintln!(
                "⚠ CRITICAL: Memory usage {:.0}% of limit ({} MB used, {} MB limit)",
                ratio * 100.0,
                usage.rss_bytes / 1024 / 1024,
                self.max_rss_bytes / 1024 / 1024
            );
        } else if ratio >= self.warn_threshold {
            eprintln!(
                "⚠ WARNING: Memory usage {:.0}% of limit ({} MB used, {} MB limit)",
                ratio * 100.0,
                usage.rss_bytes / 1024 / 1024,
                self.max_rss_bytes / 1024 / 1024
            );
        }

        if ratio > 1.0 && self.auto_terminate {
            eprintln!(
                "❌ FATAL: Memory limit exceeded ({} MB used > {} MB limit). Terminating to prevent OOM.",
                usage.rss_bytes / 1024 / 1024,
                self.max_rss_bytes / 1024 / 1024
            );
            std::process::exit(137); // SIGKILL exit code
        }

        if ratio > 1.0 {
            Err(usage)
        } else {
            Ok(usage)
        }
    }

    /// Get current process memory usage.
    fn get_current_usage() -> Result<MemoryUsage, Box<dyn std::error::Error>> {
        let status = std::fs::read_to_string("/proc/self/status")?;
        let mut rss_bytes = 0u64;
        let mut vsize_bytes = 0u64;

        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    rss_bytes = parts[1].parse::<u64>()? * 1024; // Convert kB to bytes
                }
            } else if line.starts_with("VmSize:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    vsize_bytes = parts[1].parse::<u64>()? * 1024; // Convert kB to bytes
                }
            }
        }

        Ok(MemoryUsage {
            rss_bytes,
            vsize_bytes,
        })
    }

    /// Get total system memory in KB.
    fn get_system_memory_kb() -> Result<u64, Box<dyn std::error::Error>> {
        let meminfo = std::fs::read_to_string("/proc/meminfo")?;
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].parse::<u64>()?);
                }
            }
        }
        Err("MemTotal not found in /proc/meminfo".into())
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new(
            1024 * 1024 * 1024, // 1GB
            0.8,
            Duration::from_secs(5),
            true,
        );
        assert_eq!(monitor.max_rss_bytes, 1024 * 1024 * 1024);
        assert_eq!(monitor.warn_threshold, 0.8);
    }

    #[test]
    fn test_memory_monitor_default() {
        let monitor = MemoryMonitor::default();
        // Should use env defaults or system defaults
        assert!(monitor.max_rss_bytes > 0);
    }
}
