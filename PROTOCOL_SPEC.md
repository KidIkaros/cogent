# Cogent Protocol Specification

**Version:** 0.1.0-draft
**Status:** Design Phase
**Transport:** JSON-RPC 2.0 over MCP (stdio, HTTP, WebSocket)

---

## 1. Overview

The Cogent Protocol standardizes communication between **consumers** (AI agents, IDEs, CI systems, dashboards) and **providers** (code quality engines, language analyzers, policy evaluators).

It solves: *Every agent today shells out to different CLI tools with different output formats. This protocol makes quality engines addressable, discoverable, and composable.*

### Design Principles

- **Transport-agnostic** — stdio for local agents, HTTP for remote gateways, WebSocket for streaming
- **Capability-based** — consumer discovers provider capabilities at runtime
- **Streaming-first** — findings stream as they're produced, not batched at end
- **Policy-aware** — baseline, suppression, and compliance are first-class operations
- **Language-neutral** — Rust, TypeScript, Python, Go reference implementations

---

## 2. Transport Bindings

| Transport | Use Case | Endpoint |
|-----------|----------|----------|
| **stdio** | Local agent ↔ local engine | Parent process spawns child |
| **HTTP** | Remote gateway, CI workers | `POST /rpc` (JSON-RPC batch) |
| **WebSocket** | Real-time dashboards, watch mode | `ws://host/cogent/v1` |
| **gRPC** | High-throughput internal | Optional, schema-compatible |

All transports carry **JSON-RPC 2.0** payloads. Batch requests supported on HTTP.

### Capability Negotiation (All Transports)

```
Client → Server: initialize({protocol_version, client_info, capabilities})
Server → Client: initialized({server_info, capabilities, supported_methods})
```

---

## 3. Core Methods

### 3.1 `check.run`

Execute a quality check on a codebase.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "check.run",
  "params": {
    "workspace": "file:///path/to/repo",
    "targets": ["src/", "tests/"],
    "rules": ["crap", "secrets", "complexity", "mutate"],
    "rule_packs": ["soc2", "pci-dss"],
    "config": {
      "crap": { "threshold": 30 },
      "secrets": { "ignore_paths": ["tests/fixtures/", "*.snap"] },
      "mutate": { "min_kill_rate": 80, "package": "my-crate" }
    },
    "baseline_id": "baseline-abc123",
    "incremental": true,
    "changed_files": ["src/main.rs", "src/lib.rs"],
    "output_format": "streaming"
  }
}
```

**Streaming Response (multiple messages):**
```json
// Finding produced
{"jsonrpc":"2.0","method":"findings.finding","params":{"finding":{...}}}

// Progress update
{"jsonrpc":"2.0","method":"check.progress","params":{"rule":"crap","stage":"analyzing","files_processed":42,"total":104}}

// Rule completed
{"jsonrpc":"2.0","method":"check.rule_complete","params":{"rule":"crap","passed":true,"score":28.5,"threshold":30,"duration_ms":1250}}

// Final summary
{"jsonrpc":"2.0","id":1,"result":{"check_id":"chk-xyz789","passed":false,"summary":{...},"baseline_id":"baseline-def456"}}
```

### 3.2 `findings.stream`

Subscribe to real-time findings (for dashboards, watch mode).

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "findings.stream",
  "params": {
    "check_id": "chk-xyz789",
    "filters": { "severity": ["high", "critical"], "rules": ["secrets", "crypto"] }
  }
}
```

**Responses:** Stream of `findings.finding` notifications until `findings.end`.

### 3.3 `baseline.get` / `baseline.set`

Manage finding baselines (suppress known issues, track new ones).

**Get:**
```json
{"jsonrpc":"2.0","id":3,"method":"baseline.get","params":{"baseline_id":"baseline-abc123"}}
```
**Response:**
```json
{"jsonrpc":"2.0","id":3,"result":{"baseline_id":"baseline-abc123","created_at":"2025-01-15T10:30:00Z","findings":[{"finding_id":"sec:src/main.rs:42","status":"suppressed","suppressed_by":"alice","suppressed_at":"2025-01-15T10:31:00Z","reason":"Test fixture"}]}
```

**Set (create/update):**
```json
{"jsonrpc":"2.0","id":4,"method":"baseline.set","params":{"baseline_id":"baseline-new","findings":[{"finding_id":"sec:src/main.rs:42","status":"suppressed","reason":"Test fixture"}]}}
```

### 3.4 `baseline.diff`

Compare current findings against baseline → returns only *new* findings.

```json
{"jsonrpc":"2.0","id":5,"method":"baseline.diff","params":{"baseline_id":"baseline-abc123","current_findings":[...]}}
```

### 3.5 `rule.pack.install` / `rule.pack.list` / `rule.pack.remove`

Manage compliance/rule packs (SOC2, PCI-DSS, HIPAA, custom).

```json
{"jsonrpc":"2.0","id":6,"method":"rule.pack.install","params":{"pack":"soc2","version":"2024.1","source":"registry.cogent.dev"}}
```

### 3.6 `rule.pack.resolve`

Expand a pack into concrete rules with config.

```json
{"jsonrpc":"2.0","id":7,"method":"rule.pack.resolve","params":{"pack":"soc2"}}
```
**Response:**
```json
{"result":{"rules":[{"id":"secrets","config":{"ignore_paths":["tests/"]}},{"id":"crypto","config":{}},{"id":"access-control","config":{"max_violations":0}}]}}
```

### 3.7 `remediation.apply`

Apply automated fix for a finding (when provider supports it).

```json
{"jsonrpc":"2.0","id":8,"method":"remediation.apply","params":{"finding_id":"crap:src/lib.rs:42:process_data","action":"extract_method"}}
```

### 3.8 `capabilities`

Discover provider capabilities.

```json
{"jsonrpc":"2.0","id":9,"method":"capabilities","params":{}}
```
**Response:**
```json
{"result":{"rules":["crap","secrets","complexity","mutate","debt","linelen","doccov","taint","riskmap","coupling","propcov","fuzz","halstead","typecov","errhandle","deadcode","observability","test-quality","design-docs","debuggability","vulnscan","sast","crypto","licenses","supply-chain","access-control","outdated","sbom"],"rule_packs":["soc2","pci-dss","hipaa","iso27001","owasp-top10"],"features":["streaming","incremental","baseline","remediation","compliance_mapping"],"max_workspace_size_mb":5000,"languages":["rust","python","javascript","typescript","go","c","cpp","java","csharp","php","ruby","swift","kotlin","solidity"]}}
```

---

## 4. Data Types

### 4.1 Finding

```json
{
  "finding_id": "sec:src/main.rs:42",           // rule:file:line (unique)
  "rule_id": "secrets",                          // rule that produced it
  "rule_pack": "soc2",                           // optional pack origin
  "severity": "critical",                        // info|low|medium|high|critical
  "category": "security",                        // security|quality|compliance|style
  "file": "src/main.rs",
  "line": 42,
  "column": 15,
  "end_line": 42,
  "end_column": 30,
  "message": "Hardcoded AWS secret key detected",
  "code_snippet": "aws_secret = \"AKIA...\"",
  "suggested_fix": {
    "description": "Move to environment variable",
    "diff": "- aws_secret = \"AKIA...\"\n+ aws_secret = std::env::var(\"AWS_SECRET\")?",
    "confidence": "high",
    "auto_applicable": true
  },
  "compliance_controls": ["CC7.1", "A.8.24"],
  "tags": ["secret", "aws", "credential"],
  "metadata": {"entropy": 4.2, "pattern": "aws_secret_key"}
}
```

### 4.2 CheckSummary

```json
{
  "check_id": "chk-xyz789",
  "workspace": "file:///path/to/repo",
  "started_at": "2025-01-15T10:30:00Z",
  "completed_at": "2025-01-15T10:30:05Z",
  "passed": false,
  "total_findings": 142,
  "by_severity": {"critical": 3, "high": 12, "medium": 45, "low": 82},
  "by_category": {"security": 18, "quality": 89, "compliance": 12, "style": 23},
  "by_rule": {"secrets": {"findings": 15, "passed": false}, "crap": {"findings": 31, "passed": true}},
  "rules_run": ["secrets", "crap", "complexity"],
  "skipped_rules": ["mutate"],
  "incremental": true,
  "baseline_id": "baseline-abc123",
  "new_findings": 23,
  "suppressed_findings": 5
}
```

### 4.3 RulePack

```json
{
  "id": "soc2",
  "name": "SOC 2 Type II",
  "version": "2024.1",
  "description": "Security, Availability, Confidentiality controls",
  "rules": [
    {"rule_id": "secrets", "config": {"ignore_paths": ["tests/"]}},
    {"rule_id": "crypto", "config": {}},
    {"rule_id": "access-control", "config": {"max_violations": 0}}
  ],
  "control_mapping": {
    "CC7.1": ["secrets", "crypto"],
    "CC7.2": ["access-control"],
    "A.8.24": ["secrets"]
  }
}
```

### 4.4 Capabilities

```json
{
  "protocol_version": "0.1.0",
  "rules": ["crap", "secrets", ...],
  "rule_packs": ["soc2", "pci-dss", ...],
  "features": ["streaming", "incremental", "baseline", "remediation", "compliance_mapping"],
  "max_workspace_size_mb": 5000,
  "languages": ["rust", "python", "javascript", ...],
  "transports": ["stdio", "http", "websocket"],
  "auth_methods": ["none", "bearer", "mtls"]
}
```

---

## 5. Error Codes

| Code | Meaning |
|------|---------|
| -32600 | Invalid Request |
| -32601 | Method Not Found |
| -32602 | Invalid Params |
| -32603 | Internal Error |
| -32000 | Workspace Not Found |
| -32001 | Rule Not Supported |
| -32002 | Rule Pack Not Found |
| -32003 | Baseline Not Found |
| -32004 | Incremental State Corrupt |
| -32005 | Remediation Not Supported |
| -32006 | Authentication Required |
| -32007 | Quota Exceeded |

---

## 6. Authentication & Authorization

### Transport-Level

- **stdio** — inherits caller's identity (OS user, container context)
- **HTTP** — `Authorization: Bearer <token>` (OIDC/JWT), mTLS optional
- **WebSocket** — token in upgrade handshake or first message

### Capability-Level (Future)

```json
{
  "method": "check.run",
  "required_scopes": ["cogent:check:run", "cogent:workspace:read:/path/to/repo"],
  "resource": "file:///path/to/repo"
}
```

---

## 7. Versioning

- **Protocol version** in `initialize` — major breaking, minor additive
- **Method versioning** — `check.run.v2` for breaking changes
- **Data types** — additive only; unknown fields ignored
- **Deprecation** — 6-month notice via `deprecated` field in capabilities

---

## 8. Reference Implementation: `cogent-mcp`

### Binary Interface

```bash
# stdio (default)
cogent-mcp --mode stdio

# HTTP server
cogent-mcp --mode http --port 8080 --auth bearer

# WebSocket
cogent-mcp --mode ws --port 8081
```

### Architecture

```
cogent-mcp (protocol server)
    │
    ├── transports/
    │   ├── stdio.rs
    │   ├── http.rs
    │   └── ws.rs
    │
    ├── methods/
    │   ├── check_run.rs
    │   ├── findings_stream.rs
    │   ├── baseline.rs
    │   ├── rule_pack.rs
    │   └── remediation.rs
    │
    ├── engine/
    │   └── cogent-engine (embedded library)
    │
    ├── registry/
    │   └── rule_pack_registry.rs
    │
    └── auth/
        └── auth_middleware.rs
```

### Configuration (`cogent-mcp.toml`)

```toml
[server]
mode = "stdio"  # stdio | http | ws
port = 8080
max_concurrent_checks = 4

[auth]
type = "none"  # none | bearer | mtls
jwt_issuer = "https://auth.example.com"
allowed_audiences = ["cogent-gateway"]

[engine]
cache_dir = "/var/cache/cogent"
max_workspace_mb = 5000
worker_threads = 4

[rule_packs]
registry = "https://registry.cogent.dev"
cache_ttl_hours = 24
auto_update = true

[logging]
level = "info"
format = "json"
otel_endpoint = "http://localhost:4317"
```

---

## 9. Example Flows

### 9.1 Agent PR Review (Hermes/Claude/Cursor)

```
Agent                   Cogent-MCP (stdio)
  │                          │
  ├─ initialize() ─────────►│
  │◄─ initialized() ────────┤
  │                          │
  ├─ check.run({            │
  │   workspace: "repo",    │
  │   rules: ["secrets"],   │
  │   baseline_id: "pr-123" │
  │ }) ────────────────────►│
  │                          │
  │◄─ findings.finding ─────┤  (streaming)
  │◄─ check.progress ───────┤
  │◄─ check.result ─────────┤
  │                          │
  ├─ baseline.diff({        │
  │   baseline_id: "pr-123",│
  │   current: [...]        │
  │ }) ────────────────────►│
  │◄─ {new_findings: [...]}─┤
  │                          │
  ├─ remediation.apply({    │
  │   finding_id: "sec:42", │
  │   action: "env_var"     │
  │ }) ────────────────────►│
```

### 9.2 CI Pipeline (GitHub Actions)

```
GitHub Runner          Hosted Gateway (HTTP)
  │                          │
  ├─ POST /rpc (batch) ─────►│
  │  [check.run,            │
  │   rule.pack.resolve]    │
  │                          │
  │◄─ 200 OK ───────────────┤
  │  [check.result,         │
  │   rule.pack.result]     │
  │                          │
  ├─ Upload SARIF ─────────►│  (separate API or SARIF in result)
```

### 9.3 Real-Time Dashboard (WebSocket)

```
Browser                  Cogent-MCP (WS)
  │                          │
  ├─ WS Connect ───────────►│
  │                          │
  ├─ initialize() ─────────►│
  │◄─ initialized() ────────┤
  │                          │
  ├─ findings.stream({      │
  │   check_id: "chk-123"   │
  │ }) ────────────────────►│
  │                          │
  │◄─ findings.finding ─────┤  (live updates)
  │◄─ findings.finding ─────┤
  │                          │
  ├─ baseline.set({...}) ───►│  (user dismisses finding)
```

---

## 10. Rule Pack Format (Distribution)

### Package Structure

```
cogent-rules-soc2/
├── pack.toml              # Metadata, rules, control mapping
├── rules/
│   ├── secrets.rs         # Compiled rule (WASM or native)
│   └── access_control.rs
└── templates/
    └── baseline.default.json
```

### `pack.toml`

```toml
[pack]
id = "soc2"
name = "SOC 2 Type II"
version = "2024.1"
description = "Security, Availability, Confidentiality"
license = "Apache-2.0"
repository = "github.com/cogent/rules-soc2"

[rules.secrets]
type = "native"
config_schema = { ignore_paths = "string[]", allow_patterns = "string[]" }

[rules.crypto]
type = "wasm"
module = "rules/crypto.wasm"

[control_mapping]
CC7.1 = ["secrets", "crypto"]
CC7.2 = ["access-control"]
A.8.24 = ["secrets"]
```

### Distribution

- **Registry:** `https://registry.cogent.dev` (npm-style)
- **Install:** `rule.pack.install({pack: "soc2", version: "2024.1"})`
- **Verification:** Cosign signatures, SBOM attestation

---

## 11. Implementation Roadmap

| Phase | Deliverable | Weeks |
|-------|-------------|-------|
| **0** | Protocol spec freeze (this doc) | 1 |
| **1** | `cogent-engine` as standalone library (no CLI deps) | 2 |
| **2** | `cogent-mcp` stdio transport + `check.run` | 2 |
| **3** | Streaming (`findings.stream`), baseline, rule packs | 2 |
| **4** | HTTP/WebSocket transports, auth, rate limiting | 2 |
| **5** | Hosted gateway (`cogent-cloud`), multi-tenant | 3 |
| **6** | Premium rule packs (SOC2, PCI-DSS, HIPAA) | 3 |
| **7** | Ecosystem: VS Code ext, GitHub Action, Hermes integration | 3 |
| **8** | Dogfood, load test, documentation, v1.0 release | 2 |

**Total: ~20 weeks to v1.0**

---

## 12. Open Questions

1. **WASM vs native rules** — sandboxing vs performance tradeoff
2. **Large workspace streaming** — chunked transfer encoding for HTTP?
3. **Finding deduplication** — client-side or server-side?
4. **Offline mode** — cache rule packs, baseline locally
5. **Multi-root workspaces** — monorepo support in single `check.run`
6. **Policy-as-code** — OPA/Rego integration for custom rules?

---

## 13. References

- [JSON-RPC 2.0 Spec](https://www.jsonrpc.org/specification)
- [MCP Spec](https://modelcontextprotocol.io/spec)
- [LSP Spec](https://microsoft.github.io/language-server-protocol/specification)
- [SARIF 2.1](https://docs.oasis-open.org/sarif/sarif/v2.1/sarif-v2.1.html)
- [OpenAPI 3.1](https://spec.openapis.org/oas/v3.1.0) — for HTTP binding

---

*This specification is a living document. Implementations SHOULD tolerate unknown fields and methods gracefully.*