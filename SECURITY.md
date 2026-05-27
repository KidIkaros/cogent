# Security Policy

## Supported Versions
We take security seriously and strive to promptly address security vulnerabilities.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability
If you discover a security vulnerability in Cogent, please report it responsibly.

### How to Report
**Please DO NOT report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to: kidikaros@proton.me

### What to Include
Please include the following information in your report:
- Type of vulnerability (e.g., buffer overflow, SQL injection, etc.)
- Full path to the affected file(s)
- Steps to reproduce the issue
- Proof-of-concept or exploit code (if available)
- Potential impact of the vulnerability
- Any suggested fixes or mitigations

### Response Timeline
- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 5 business days
- **Status Updates**: Every 7 days until resolution
- **Fix Release**: Timeline depends on severity

### Severity Levels
- **Critical**: Immediate security impact, exploit available → 24-48 hours
- **High**: Significant security impact → 3-7 days
- **Medium**: Moderate security impact → 14-30 days
- **Low**: Minor security impact → Next regular release

## Disclosure Policy
- We follow a coordinated disclosure approach
- We will work with you to understand and resolve the issue
- We will credit you in the security advisory (unless you prefer to remain anonymous)
- Public disclosure happens after a fix is available and users have had reasonable time to update

## Security Best Practices for Contributors
When contributing to Cogent, please keep these security considerations in mind:

### Code Review Checklist
- [ ] Input validation: Are all user inputs properly validated?
- [ ] Path traversal: Are file paths properly sanitized?
- [ ] Command injection: Is external command execution safe?
- [ ] Resource exhaustion: Are there limits on memory/CPU usage?
- [ ] Error handling: Are errors properly handled without leaking sensitive info?
- [ ] Dependencies: Are dependencies free from known vulnerabilities?

### Safe Coding Patterns
```rust
// Use safe path handling
let path = Path::new(input).canonicalize()?;
if !path.starts_with(base_dir) {
    return Err("Path traversal detected".into());
}

// Limit resource usage
let monitor = MemoryMonitor::new();
monitor.check_memory()?;

// Proper error handling
let content = std::fs::read_to_string(&path)
    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
```

### Dependencies
We regularly update dependencies to patch known vulnerabilities. You can help by:
- Running `cargo audit` to check for vulnerable dependencies
- Updating dependencies in your PRs (within reason)
- Not adding dependencies with known vulnerabilities

## Known Security Considerations

### Cogent Security Model
- Cogent is a CLI tool that analyzes source code
- It runs locally on the user's machine
- It does not expose network services (except cogent-server, which is optional)
- It reads source files and produces reports

### Potential Risks
1. **Malicious Source Files**: Tree-sitter parsers are generally safe, but malformed input could cause issues
2. **Path Traversal**: When processing user-specified paths
3. **Resource Exhaustion**: Large codebases could cause high memory/CPU usage
4. **cogent-server**: The JSON-RPC server should not be exposed to untrusted networks

### Mitigations
- We use memory monitoring to prevent OOM conditions
- We limit rayon parallelism to prevent CPU exhaustion
- We validate and canonicalize file paths
- cogent-server is designed for local/trusted network use only

## Contact
For security-related questions or concerns, contact: kidikaros@proton.me

For general questions, please use [GitHub Issues](https://github.com/KidIkaros/cogent/issues).
