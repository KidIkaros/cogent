//! Configuration loading for the Cogent Engine

use crate::types::CheckThresholds;
use cogent_protocol::types::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Engine configuration loaded from .quality.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineConfig {
    /// Check thresholds
    #[serde(flatten)]
    pub thresholds: CheckThresholds,

    /// Per-rule configuration
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,

    /// Enabled rule packs
    #[serde(default)]
    pub rule_packs: Vec<String>,

    /// Exclude patterns (glob)
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Include patterns (glob)
    #[serde(default)]
    pub include: Vec<String>,

    /// Cache directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,

    /// Max cache size in MB
    #[serde(default = "default_max_cache_mb")]
    pub max_cache_mb: usize,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,

    /// Number of worker threads
    #[serde(default = "default_workers")]
    pub workers: usize,
}

fn default_max_cache_mb() -> usize {
    100
}

fn default_cache_ttl_secs() -> u64 {
    7 * 24 * 60 * 60 // 7 days
}

fn default_workers() -> usize {
    num_cpus::get().max(1)
}

impl EngineConfig {
    /// Load config from .quality.toml file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parse config from string
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        // Parse as TOML first
        if let Ok(toml_value) = content.parse::<toml::Value>() {
            if let Some(table) = toml_value.as_table() {
                // Parse thresholds
                if let Some(thresholds_value) = table.get("thresholds") {
                    config.thresholds = toml::from_str(&thresholds_value.to_string())?;
                }

                // Parse rules config
                if let Some(rules_value) = table.get("rules") {
                    config.rules = toml::from_str(&rules_value.to_string()).unwrap_or_default();
                }

                // Parse rule packs
                if let Some(packs_value) = table.get("rule_packs") {
                    config.rule_packs = toml::from_str(&packs_value.to_string()).unwrap_or_default();
                }

                // Parse exclude/include
                if let Some(exclude_value) = table.get("exclude") {
                    config.exclude = toml::from_str(&exclude_value.to_string()).unwrap_or_default();
                }
                if let Some(include_value) = table.get("include") {
                    config.include = toml::from_str(&include_value.to_string()).unwrap_or_default();
                }

                // Parse cache settings
                if let Some(cache_value) = table.get("cache") {
                    if let Some(cache_table) = cache_value.as_table() {
                        if let Some(dir) = cache_table.get("dir").and_then(|v| v.as_str()) {
                            config.cache_dir = Some(dir.to_string());
                        }
                        if let Some(max_mb) = cache_table.get("max_mb").and_then(|v| v.as_integer()) {
                            config.max_cache_mb = max_mb as usize;
                        }
                        if let Some(ttl) = cache_table.get("ttl_secs").and_then(|v| v.as_integer()) {
                            config.cache_ttl_secs = ttl as u64;
                        }
                    }
                }

                // Parse workers
                if let Some(workers) = table.get("workers").and_then(|v| v.as_integer()) {
                    config.workers = workers as usize;
                }
            }
        }

        Ok(config)
    }

    /// Merge with another config (other takes precedence)
    pub fn merge(&mut self, other: Self) {
        self.thresholds = other.thresholds;
        self.rules.extend(other.rules);
        self.rule_packs = other.rule_packs;
        self.exclude = other.exclude;
        self.include = other.include;
        if other.cache_dir.is_some() {
            self.cache_dir = other.cache_dir;
        }
        self.max_cache_mb = other.max_cache_mb;
        self.cache_ttl_secs = other.cache_ttl_secs;
        self.workers = other.workers;
    }
}

/// Config error type
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

/// Parse legacy .quality.toml format (line-by-line key = value)
impl CheckThresholds {
    /// Load from legacy .quality.toml format
    pub fn load_legacy<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let mut t = Self::default();
        let content = std::fs::read_to_string(path)?;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            macro_rules! parse_f64 {
                ($key:expr, $field:ident) => {
                    if let Some(val) = parse_config_f64(line, $key) {
                        t.$field = val;
                    }
                };
            }
            macro_rules! parse_usize {
                ($key:expr, $field:ident) => {
                    if let Some(val) = parse_config_usize(line, $key) {
                        t.$field = val;
                    }
                };
            }

            parse_f64!("max_avg", max_crap);
            parse_f64!("min_pct", min_doc);
            parse_usize!("max_markers", max_debt);
            parse_usize!("max_violations", max_complexity_violations);
            parse_f64!("max_duplicates", max_duplication);
            parse_usize!("max_taint", max_taint);
            parse_f64!("max_risk", max_risk);
            parse_usize!("max_coupling", max_coupling);
            parse_f64!("min_propcov", min_propcov);
            parse_usize!("max_fuzz_risk", max_fuzz_risk);
            parse_usize!("max_linelen", max_linelen);
            parse_f64!("max_halstead_bugs", max_halstead_bugs);
            parse_usize!("max_secrets", max_secrets);
            parse_usize!("max_deadcode", max_deadcode);
            parse_usize!("max_cohesion", max_cohesion);
            parse_f64!("min_comment_ratio", min_comment_ratio);
            parse_usize!("max_errhandle", max_errhandle);
            parse_f64!("min_typecov", min_typecov);
            parse_usize!("max_vuln_critical", max_vuln_critical);
            parse_usize!("max_vuln_high", max_vuln_high);
            parse_usize!("max_sast", max_sast);
            parse_usize!("max_crypto", max_crypto);
            parse_usize!("max_license_violations", max_license_violations);
            parse_usize!("max_outdated", max_outdated);
            parse_usize!("max_access_control", max_access_control);
            parse_usize!("max_supply_chain", max_supply_chain);

            if let Some(val) = parse_config_u32(line, "min_complexity") {
                t.min_complexity = val;
            }
        }

        Ok(t)
    }
}

/// Parse a `key = value` line for f64 values
fn parse_config_f64(line: &str, key: &str) -> Option<f64> {
    let prefix = format!("{} =", key);
    let prefix2 = format!("{}=", key);
    let rest = line.strip_prefix(&prefix).or_else(|| line.strip_prefix(&prefix2))?;
    rest.split_whitespace().next()?.parse().ok()
}

/// Parse a `key = value` line for usize values
fn parse_config_usize(line: &str, key: &str) -> Option<usize> {
    parse_config_f64(line, key).map(|v| v as usize)
}

/// Parse a `key = value` line for u32 values
fn parse_config_u32(line: &str, key: &str) -> Option<u32> {
    parse_config_f64(line, key).map(|v| v as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_parse_legacy_config() {
        let content = r#"
max_avg = 30.0
min_pct = 50.0
max_markers = 100
max_taint = 5
max_secrets = 0
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let thresholds = CheckThresholds::load_legacy(tmp.path()).unwrap();
        assert_eq!(thresholds.max_crap, 30.0);
        assert_eq!(thresholds.min_doc, 50.0);
        assert_eq!(thresholds.max_debt, 100);
        assert_eq!(thresholds.max_taint, 5);
        assert_eq!(thresholds.max_secrets, 0);
    }
}