#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "cryptocheck",
    about = "Cryptography checker — weak hash, insecure random, hardcoded IVs, ECB mode, deprecated TLS"
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

    /// Max allowed crypto findings (default: 0)
    #[arg(long, default_value = "0")]
    max_findings: usize,
}

#[derive(Debug, Clone, Serialize)]
struct CryptoFinding {
    file: String,
    line: usize,
    category: String,
    rule_id: String,
    severity: String,
    context: String,
    description: String,
    remediation: String,
}

#[derive(Serialize)]
struct CryptoReport {
    findings: Vec<CryptoFinding>,
    summary: CryptoSummary,
}

#[derive(Serialize)]
struct CryptoSummary {
    files_scanned: usize,
    total_findings: usize,
    critical: usize,
    high: usize,
    medium: usize,
    max_findings_threshold: usize,
}

struct CryptoRule {
    category: &'static str,
    rule_id: &'static str,
    severity: &'static str,
    pattern: &'static str,
    also: Option<&'static str>,
    description: &'static str,
    remediation: &'static str,
}

const RULES: &[CryptoRule] = &[
    // ── Weak Hash Algorithms ──────────────────────────────────────
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-001", severity: "high",
        pattern: "Md5::", also: None,
        description: "MD5 is cryptographically broken. Do not use for security-sensitive hashing.",
        remediation: "Replace MD5 with SHA-256 or SHA-3. For passwords use Argon2, bcrypt, or scrypt." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-002", severity: "high",
        pattern: "md5(", also: None,
        description: "md5() function call detected — MD5 is broken.",
        remediation: "Replace with SHA-256 or a password hashing function." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-003", severity: "high",
        pattern: "hashlib.md5(", also: None,
        description: "Python hashlib.md5() — MD5 is cryptographically broken.",
        remediation: "Use hashlib.sha256() or hashlib.sha3_256() for general hashing; use passlib for passwords." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-004", severity: "high",
        pattern: "hashlib.sha1(", also: None,
        description: "SHA-1 is deprecated for security use (collision attack demonstrated).",
        remediation: "Upgrade to SHA-256 or SHA-3." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-005", severity: "high",
        pattern: "Sha1::", also: None,
        description: "SHA-1 use detected in Rust — broken for security contexts.",
        remediation: "Use sha2::Sha256 or sha3::Sha256 instead." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-006", severity: "high",
        pattern: "MessageDigest.getInstance(\"MD5\")", also: None,
        description: "Java MD5 MessageDigest detected.",
        remediation: "Use MessageDigest.getInstance(\"SHA-256\") instead." },
    CryptoRule { category: "weak_hash", rule_id: "CRYPTO-HASH-007", severity: "high",
        pattern: "MessageDigest.getInstance(\"SHA-1\")", also: None,
        description: "Java SHA-1 MessageDigest detected — deprecated for security.",
        remediation: "Use MessageDigest.getInstance(\"SHA-256\") instead." },
    // ── Insecure Random ───────────────────────────────────────────
    CryptoRule { category: "insecure_random", rule_id: "CRYPTO-RAND-001", severity: "high",
        pattern: "rand::random()", also: None,
        description: "rand::random() uses the default non-cryptographic RNG.",
        remediation: "For security use, replace with rand::rngs::OsRng or ring::rand::SystemRandom." },
    CryptoRule { category: "insecure_random", rule_id: "CRYPTO-RAND-002", severity: "high",
        pattern: "random.random()", also: None,
        description: "Python random.random() is not cryptographically secure.",
        remediation: "Use secrets.token_bytes() or secrets.token_hex() for security-sensitive randomness." },
    CryptoRule { category: "insecure_random", rule_id: "CRYPTO-RAND-003", severity: "high",
        pattern: "Math.random()", also: None,
        description: "JavaScript Math.random() is not cryptographically secure.",
        remediation: "Use crypto.getRandomValues() or Node's crypto.randomBytes() for security." },
    CryptoRule { category: "insecure_random", rule_id: "CRYPTO-RAND-004", severity: "high",
        pattern: "new Random()", also: None,
        description: "Java new Random() is not cryptographically secure.",
        remediation: "Use java.security.SecureRandom for security-sensitive operations." },
    CryptoRule { category: "insecure_random", rule_id: "CRYPTO-RAND-005", severity: "high",
        pattern: "rand.Intn(", also: None,
        description: "Go math/rand is not cryptographically secure.",
        remediation: "Use crypto/rand for security-sensitive randomness." },
    // ── Hardcoded IVs / Keys ──────────────────────────────────────
    CryptoRule { category: "hardcoded_iv", rule_id: "CRYPTO-IV-001", severity: "critical",
        pattern: "iv = b\"", also: None,
        description: "Hardcoded IV (initialization vector) detected — IVs must be random.",
        remediation: "Generate a fresh cryptographic random IV for every encryption operation." },
    CryptoRule { category: "hardcoded_iv", rule_id: "CRYPTO-IV-002", severity: "critical",
        pattern: "let iv = [0", also: None,
        description: "Zero-filled IV detected — trivially predictable, breaks semantic security.",
        remediation: "Generate IV with OsRng::fill_bytes() or equivalent." },
    CryptoRule { category: "hardcoded_iv", rule_id: "CRYPTO-IV-003", severity: "critical",
        pattern: "nonce = b\"", also: None,
        description: "Hardcoded nonce detected — nonces must be unique per encryption.",
        remediation: "Generate a cryptographic random nonce for every encryption operation." },
    // ── Weak Cipher Modes ─────────────────────────────────────────
    CryptoRule { category: "weak_cipher", rule_id: "CRYPTO-CIPHER-001", severity: "critical",
        pattern: "AES_ECB", also: None,
        description: "AES-ECB mode detected — ECB does not provide semantic security.",
        remediation: "Replace with AES-GCM or AES-CBC with random IV and MAC. Never use ECB for block ciphers." },
    CryptoRule { category: "weak_cipher", rule_id: "CRYPTO-CIPHER-002", severity: "critical",
        pattern: "\"ECB\"", also: None,
        description: "ECB cipher mode string literal detected.",
        remediation: "Use GCM or CBC+HMAC. ECB leaks patterns in plaintext." },
    CryptoRule { category: "weak_cipher", rule_id: "CRYPTO-CIPHER-003", severity: "critical",
        pattern: "Cipher.getInstance(\"AES\")", also: None,
        description: "Java AES without mode defaults to ECB — insecure.",
        remediation: "Use Cipher.getInstance(\"AES/GCM/NoPadding\") instead." },
    CryptoRule { category: "weak_cipher", rule_id: "CRYPTO-CIPHER-004", severity: "high",
        pattern: "DES", also: Some("Cipher"),
        description: "DES cipher is 56-bit — easily brutable, deprecated since 1999.",
        remediation: "Replace DES with AES-256-GCM." },
    CryptoRule { category: "weak_cipher", rule_id: "CRYPTO-CIPHER-005", severity: "high",
        pattern: "RC4", also: None,
        description: "RC4 is cryptographically broken — biased output, multiple attacks.",
        remediation: "Replace RC4 with AES-GCM or ChaCha20-Poly1305." },
    // ── Deprecated TLS / SSL ──────────────────────────────────────
    CryptoRule { category: "tls_config", rule_id: "CRYPTO-TLS-001", severity: "high",
        pattern: "SSLv3", also: None,
        description: "SSLv3 is deprecated and vulnerable (POODLE attack).",
        remediation: "Enforce TLS 1.2+ only. Disable SSLv3, TLSv1.0, TLSv1.1." },
    CryptoRule { category: "tls_config", rule_id: "CRYPTO-TLS-002", severity: "high",
        pattern: "TLSv1_0", also: None,
        description: "TLS 1.0 is deprecated — vulnerable to BEAST and other attacks.",
        remediation: "Require TLS 1.2 or 1.3 minimum." },
    CryptoRule { category: "tls_config", rule_id: "CRYPTO-TLS-003", severity: "critical",
        pattern: "verify=False", also: None,
        description: "TLS certificate verification disabled — man-in-the-middle attacks possible.",
        remediation: "Never disable certificate verification in production. Use proper CA trust store." },
    CryptoRule { category: "tls_config", rule_id: "CRYPTO-TLS-004", severity: "critical",
        pattern: "danger_accept_invalid_certs(true)", also: None,
        description: "Rust reqwest TLS cert verification disabled.",
        remediation: "Remove danger_accept_invalid_certs(). Configure a proper CA certificate if using self-signed certs." },
    // ── Password Hashing ─────────────────────────────────────────
    CryptoRule { category: "password_hash", rule_id: "CRYPTO-PWD-001", severity: "critical",
        pattern: "sha256(password", also: None,
        description: "Plain SHA-256 used for password hashing — no salt/iterations.",
        remediation: "Use Argon2id, bcrypt, or scrypt for password hashing. Never use fast hashes for passwords." },
    CryptoRule { category: "password_hash", rule_id: "CRYPTO-PWD-002", severity: "critical",
        pattern: "md5(password", also: None,
        description: "MD5 used for password hashing — completely insecure.",
        remediation: "Use Argon2id with appropriate memory/iteration parameters." },
];

fn scan_file(path: &str) -> Vec<CryptoFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if !matches!(
        ext,
        "rs" | "py" | "js" | "ts" | "tsx" | "go" | "java" | "cs" | "rb" | "php"
    ) {
        return vec![];
    }

    let mut findings = Vec::new();
    for (lineno, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }

        for rule in RULES {
            if !line.contains(rule.pattern) {
                continue;
            }
            if let Some(also) = rule.also {
                if !line.contains(also) {
                    continue;
                }
            }
            findings.push(CryptoFinding {
                file: path.to_string(),
                line: lineno + 1,
                category: rule.category.to_string(),
                rule_id: rule.rule_id.to_string(),
                severity: rule.severity.to_string(),
                context: truncate(trimmed, 80).to_string(),
                description: rule.description.to_string(),
                remediation: rule.remediation.to_string(),
            });
            break;
        }
    }
    findings
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "js", "ts", "tsx", "go", "java", "cs", "rb", "php",
    ];
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut all_findings: Vec<CryptoFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file));
    }
    all_findings.sort_by(|a, b| {
        let sev_ord = |s: &str| match s {
            "critical" => 0u8,
            "high" => 1,
            "medium" => 2,
            _ => 3,
        };
        sev_ord(&a.severity)
            .cmp(&sev_ord(&b.severity))
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });

    let critical = all_findings
        .iter()
        .filter(|f| f.severity == "critical")
        .count();
    let high = all_findings.iter().filter(|f| f.severity == "high").count();
    let medium = all_findings
        .iter()
        .filter(|f| f.severity == "medium")
        .count();
    let total = all_findings.len();

    let summary = CryptoSummary {
        files_scanned: files.len(),
        total_findings: total,
        critical,
        high,
        medium,
        max_findings_threshold: cli.max_findings,
    };

    match cli.format.as_str() {
        "json" => {
            let report = CryptoReport {
                findings: all_findings,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for f in &all_findings {
                println!("{}", serde_json::to_string(f).unwrap());
            }
        }
        _ => {
            if all_findings.is_empty() {
                println!("No cryptographic issues detected.");
            } else {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 30,
                        align_right: false,
                    },
                    Column {
                        header: "Line",
                        width: 6,
                        align_right: true,
                    },
                    Column {
                        header: "Sev",
                        width: 9,
                        align_right: false,
                    },
                    Column {
                        header: "Rule",
                        width: 18,
                        align_right: false,
                    },
                    Column {
                        header: "Category",
                        width: 16,
                        align_right: false,
                    },
                    Column {
                        header: "Context",
                        width: 40,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for f in &all_findings {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&f.file, 30),
                            &f.line.to_string(),
                            &f.severity,
                            &truncate(&f.rule_id, 18),
                            &truncate(&f.category, 16),
                            &truncate(&f.context, 40),
                        ],
                    );
                }
            }
            let status = if total <= cli.max_findings {
                "PASS"
            } else {
                "FAIL"
            };
            println!(
                "\nSummary: {} findings ({} critical, {} high, {} medium) — {}",
                total, critical, high, medium, status
            );
        }
    }

    if total > cli.max_findings {
        std::process::exit(1);
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_md5() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".py").unwrap();
        writeln!(f, "import hashlib; h = hashlib.md5(data).hexdigest()").unwrap();
        let findings = scan_file(f.path().to_str().unwrap());
        assert!(findings.iter().any(|f| f.category == "weak_hash"));
    }

    #[test]
    fn test_detect_insecure_random() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".js").unwrap();
        writeln!(f, "const token = Math.random().toString(36);").unwrap();
        let findings = scan_file(f.path().to_str().unwrap());
        assert!(findings.iter().any(|f| f.category == "insecure_random"));
    }

    #[test]
    fn test_detect_ecb_mode() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".java").unwrap();
        writeln!(f, "Cipher c = Cipher.getInstance(\"AES\");").unwrap();
        let findings = scan_file(f.path().to_str().unwrap());
        assert!(findings.iter().any(|f| f.category == "weak_cipher"));
    }

    #[test]
    fn test_detect_tls_disabled() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(
            f,
            "let client = Client::builder().danger_accept_invalid_certs(true).build();"
        )
        .unwrap();
        let findings = scan_file(f.path().to_str().unwrap());
        assert!(findings.iter().any(|f| f.category == "tls_config"));
    }

    #[test]
    fn test_no_findings_clean() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn add(a: i32, b: i32) -> i32 {{ a + b }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap());
        assert!(findings.is_empty());
    }
}
