# crypto — Cryptographic Weakness Detection

Detects weak cryptography, insecure randomness, and deprecated algorithms.

## What it detects

- **Weak hashes** — MD5, SHA1 usage
- **Insecure random** — `Math.random()`, `rand()` for security
- **Hardcoded IVs** — Predictable initialization vectors
- **ECB mode** — Insecure AES mode
- **Deprecated TLS** — SSLv3, TLS 1.0/1.1
- **Fast hash passwords** — MD5/SHA1 for password storage
- **Weak key sizes** — RSA < 2048, DSA < 2048

## Configuration

```toml
[crypto]
max_crypto = 0
```

## Examples

```bash
cryptocheck ./src
cryptocheck ./src --format json
```

## Remediation

- Use SHA-256/SHA-3 for hashing, bcrypt/Argon2 for passwords
- Use `crypto/rand` (Go), `secrets` (Python), `SecureRandom` (Java)
- Use AES-GCM or ChaCha20-Poly1305 for encryption
- Enforce TLS 1.2+
