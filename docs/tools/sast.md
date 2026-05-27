# sast — Static Application Security Testing

Detects common security vulnerabilities in source code through static analysis.

## What it detects

- **SQL Injection** — Unsanitized user input in SQL queries
- **Cross-Site Scripting (XSS)** — Unescaped output in web contexts
- **Command Injection** — User input passed to system commands
- **Path Traversal** — File path manipulation vulnerabilities
- **Insecure Deserialization** — Unsafe object deserialization
- **Server-Side Request Forgery (SSRF)** — Unauthorized internal network requests
- **Hardcoded Credentials** — Passwords and tokens in code
- **Unsafe Eval** — Dynamic code execution with untrusted input
- **Open Redirects** — URL redirection to untrusted sites
- **Insecure Headers** — Missing security HTTP headers

## Configuration

```toml
[sast]
max_sast = 0  # Zero tolerance for security issues
```

## Output format

```json
{
  "tool": "sast",
  "passed": false,
  "message": "2 high-severity vulnerabilities found",
  "findings": [
    {
      "file": "src/handlers.rs",
      "line": 89,
      "message": "SQL Injection: user input used directly in query",
      "severity": "critical",
      "rule_id": "sast-sqli",
      "help": "Use parameterized queries or prepared statements"
    }
  ]
}
```

## Examples

```bash
# Basic scan
sast ./src

# JSON output
sast ./src --format json

# Recursive scan with specific rules
sast ./ --recursive
```

## Exit codes

- `0` — No vulnerabilities found
- `1` — Vulnerabilities found

## Remediation

1. **SQL Injection** — Use parameterized queries/prepared statements
2. **XSS** — Escape all user-generated content before rendering
3. **Command Injection** — Avoid shell execution; use library functions
4. **Path Traversal** — Validate and sanitize file paths; use allowlists
5. **SSRF** — Validate URLs; block internal IP ranges
6. **Deserialization** — Use safe formats (JSON); validate schemas

## See also

- `secrets` — Credential detection
- `crypto` — Cryptographic weakness detection
- `taint` — Data flow tracking
