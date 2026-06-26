# Language-Specific Guides

Cogent supports 9 programming languages with specialized tools and workflows.

---

## Available Guides

- [Rust](rust.md) — Complete guide for Rust projects
- [Python](python.md) — Complete guide for Python projects
- [JavaScript/TypeScript](javascript.md) — Complete guide for JS/TS projects
- [Go](go.md) — Complete guide for Go projects

---

## Getting Started

1. **Install Cogent** — See [installation.md](../installation.md)
2. **Initialize your project** — `cogent init`
3. **Run full audit** — `cogent check .`
4. **Read your language's guide** — See below

---

## Language Coverage

| Language | Tools | Quick Start |
|----------|-------|-------------|
| **Rust** | 10 tools (complexity, doccov, errhandle, mutation, etc.) | See [Rust Guide](rust.md) |
| **Python** | 8 tools (typecov, debt, deadcode, etc.) | See [Python Guide](python.md) (coming soon) |
| **JavaScript/TypeScript** | 6 tools (complexity, typecov, ast-parse, etc.) | See [JS Guide](javascript.md) (coming soon) |
| **Go** | 8 tools (complexity, debt, deadcode, etc.) | See [Go Guide](go.md) (coming soon) |

---

## Quick Reference

### Rust

```bash
cargo build
cogent check .
```

### Python

```bash
pip install -r requirements.txt
cogent check .
```

### JavaScript/TypeScript

```bash
npm install
cogent check .
```

### Go

```bash
go mod tidy
cogent check .
```

---

## Contributing

Want to add a language guide? Contributions welcome!

1. Create `docs/languages/<language>.md`
2. Follow the structure in [rust.md](rust.md)
3. Submit a PR to https://github.com/KidIkaros/cogent

---

## See Also

- [Quickstart](../quickstart.md) — 5-minute tutorial
- [Installation](../installation.md) — Platform-specific installation
- [Troubleshooting](../troubleshooting.md) — Common issues
- [Tools](../tools/) — All 31 tools documented