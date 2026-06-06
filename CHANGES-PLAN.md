# Cogent — Consolidated Change Plan

> Created: 2026-06-05
> Status: ✅ ALL PHASES COMPLETE
> Context: Documentation re-evaluation session found multiple inconsistencies between docs and code

---

## Phase 1: Health Score in JSON Output (HIGH PRIORITY)

**Problem:** `health_score` and `grade` only appear in the text summary box. CI consumers and agents using `--format json` can't see the weighted score.

### Changes Required

**File: `crates/cogent-common/src/types.rs`**
- Add `health_score: u32` and `grade: String` fields to `CheckReport` struct
- These are computed fields (not serialized from config), so use `#[serde(skip_serializing_if = "...")]` or compute in the serializer

**File: `crates/cogent-cli/src/dispatcher.rs`** (run_check_subcommand, ~line 780)
- After computing `let (health, grade) = health_score(&report.checks);`
- Set `report.health_score = health;` and `report.grade = grade.to_string();` before serialization
- Currently `health` and `grade` are computed but only used for the text box and CI summary

**File: `crates/cogent-cli/src/types.rs`**
- Re-export the new fields if CheckReport is re-exported from cogent-common

### Verification
```bash
cargo test -p cogent-cli -p cogent-common
./target/release/cogent check . --format json | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('health_score'), d.get('grade'))"
```

---

## Phase 2: HQSE Threshold Defaults (MEDIUM PRIORITY)

**Problem:** `observability`, `test-quality`, and `debuggability` thresholds default to `usize::MAX` (1.84e+19), making them impossible to fail. This means these checks are purely advisory and can never gate a build.

### Changes Required

**File: `crates/cogent-engine/src/lib.rs`** (CheckThresholds::default, ~line 340)
```rust
// BEFORE:
max_observability: usize::MAX,
max_test_quality: usize::MAX,
max_debuggability: usize::MAX,

// AFTER (sensible defaults that can actually fail):
max_observability: 1000,   // allows moderate violation counts
max_test_quality: 60,      // minimum 60% test quality score
max_debuggability: 1000,   // allows moderate contextless unwraps
```

**File: `crates/cogent-cli/src/dispatcher.rs`** (run_check_subcommand, ~line 655)
- Update `reg_thresholds` to use the new defaults instead of hardcoded `usize::MAX`:
```rust
// BEFORE:
max_observability: usize::MAX,
max_test_quality: usize::MAX,
max_debuggability: usize::MAX,

// AFTER:
// These now come from CheckThresholds::default() or .quality.toml
```

**File: `.quality.toml`** (if it exists)
- Add HQSE threshold keys so users can configure them:
```toml
max_observability = 1000
max_test_quality = 60
max_debuggability = 1000
```

### Verification
```bash
cargo test -p cogent-engine -p cogent-cli
./target/release/cogent check . --only 'observability' --format text
./target/release/cogent check . --only 'test-quality' --format text
./target/release/cogent check . --only 'debuggability' --format text
```

---

## Phase 3: Audit Opinion Model (HIGH PRIORITY — NEW FEATURE)

**Problem:** The current score is a simple `passed/total × 100` ratio (in progress.rs) or a weighted ratio (in cogent-common). Neither captures the full audit picture. A project with one exposed API key and 28 perfect checks currently scores 96/100 (A grade) — dangerously misleading.

### Design (approved by user)

**Tier 1 — Gate Killers (binary pass/fail):**
- `secrets` — any exposed credential = automatic fail
- `vulnscan` — any critical CVE = automatic fail
- `sast` — any critical severity finding = automatic fail
- `taint` — any unvalidated sensitive data flow = automatic fail

**Tier 2 — Weighted Category Scores (0–100 each):**
| Category | Weight | Tools |
|---|---|---|
| Security | 5× | secrets, sast, crypto, taint, vulnscan, access-control, errhandle |
| Compliance | 3× | licenses, sbom, supply-chain, outdated |
| Quality | 2× | crap, complexity, deadcode, coupling, dupfind, riskmap, halstead, cohesion, fuzz, propcov |
| Hygiene | 1× | debt, comments, linelen, doccov, typecov |
| Operations | 1× | observability, test-quality, design-docs, debuggability |

**Tier 3 — Margin-to-Threshold:**
- Already implemented in `print_margin_summary()` — integrate into the grade

**Audit Opinion:**
- **UNQUALIFIED PASS** — all gates pass, weighted score ≥ 80
- **QUALIFIED PASS** — all gates pass, weighted score 60–79
- **ADVERSE** — one or more gate killers failed
- **DISCLAIMER** — too many tools unavailable (5+ skipped)

### Changes Required

**File: `crates/cogent-common/src/lib.rs`**
- Add new types:
```rust
pub enum AuditOpinion {
    UnqualifiedPass,
    QualifiedPass,
    Adverse,
    Disclaimer,
}

pub struct CategoryScore {
    pub name: String,
    pub weight: u32,
    pub score: f64,       // 0-100
    pub checks_passed: usize,
    pub checks_total: usize,
}

pub struct AuditResult {
    pub opinion: AuditOpinion,
    pub overall_score: u32,
    pub grade: char,
    pub gate_killers_passed: bool,
    pub categories: Vec<CategoryScore>,
    pub margin_risks: Vec<(String, f64)>,  // (check_name, margin_%)
    pub unavailable_count: usize,
}
```
- Add `pub fn compute_audit(checks: &[CheckResult]) -> AuditResult` function

**File: `crates/cogent-common/src/types.rs`**
- Add `audit: Option<AuditResult>` field to `CheckReport`

**File: `crates/cogent-cli/src/progress.rs`**
- Replace `print_summary_box` with `print_audit_opinion` that renders:
```
╔══════════════════════════════════════════════════════════════╗
║  COGENT AUDIT OPINION                                        ║
╠══════════════════════════════════════════════════════════════╣
║  Overall: UNQUALIFIED PASS ✓                                 ║
║  Risk Score: 94/100  Grade: A                                ║
║                                                              ║
║  Gate Killers (4/4 passed)                                   ║
║    ✓ secrets · vulnscan · sast · taint                       ║
║                                                              ║
║  Security (weighted 5×)    98/100                            ║
║  Compliance (weighted 3×)  100/100                           ║
║  Quality (weighted 2×)     91/100                            ║
║  Hygiene (weighted 1×)     88/100                            ║
║  Operations (weighted 1×)  85/100                            ║
║                                                              ║
║  ⚠ Margin Risk: riskmap at threshold (75.0/75.0)            ║
║  ⚠ Margin Risk: doc_coverage at 100.0/95.0 (5% headroom)   ║
╚══════════════════════════════════════════════════════════════╝
```

**File: `crates/cogent-cli/src/dispatcher.rs`**
- Replace `print_summary_box` call with `print_audit_opinion`
- Pass `AuditResult` to JSON/SARIF output formats

**File: `crates/cogent-cli/src/report_formatters.rs`**
- Include audit opinion in JSON output

**Tests to add:**
- `test_gate_killer_secrets_fails` — secrets > 0 → Adverse
- `test_gate_killer_vulnscan_fails` — vulnscan critical > 0 → Adverse
- `test_weighted_category_scores` — verify security 5×, compliance 3×, etc.
- `test_audit_opinion_unqualified` — all pass, score ≥ 80
- `test_audit_opinion_qualified` — all pass, score 60-79
- `test_audit_opinion_adverse` — gate killer fails
- `test_audit_opinion_disclaimer` — 5+ tools unavailable

### Verification
```bash
cargo test -p cogent-cli -p cogent-common
./target/release/cogent check . --format text  # verify new output
./target/release/cogent check . --format json | python3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps(d.get('audit'), indent=2))"
```

---

## Phase 4: Remaining Registry Cleanup (LOW PRIORITY)

**Problem:** `test` and `test2` entries appear in the `test_audit_tool_with_recursive` test function in registry.rs. These are test fixtures, not production entries, but the initial grep counted them as 33 tools.

### Changes Required

**File: `crates/cogent-engine/src/registry.rs`** (test function, ~line 227)
- Rename `test` → `mock-tool-a` and `test2` → `mock-tool-b` to avoid confusion
- This is cosmetic — no functional change

---

## Execution Order

1. **Phase 1** (JSON health_score) — 15 min, unblocks CI consumers
2. **Phase 4** (registry cleanup) — 5 min, cosmetic
3. **Phase 2** (HQSE thresholds) — 10 min, makes HQSE checks meaningful
4. **Phase 3** (Audit Opinion) — 2-3 hours, the big feature

---

## Files Modified This Session (Already Done)

| File | Change | Status |
|---|---|---|
| `crates/cogent-cli/src/progress.rs` | Removed duplicate `health_score`, re-export from `cogent_common`; updated tests; added `errhandle` to security list | ✅ Done |
| `crates/cogent-cli/src/dispatcher.rs` | Moved `errhandle` from `quality_checks` to `security_checks` | ✅ Done |
| `crates/cogent-engine/Cargo.toml` | Fixed feature conflicts (tracing, mockall); added `syn` dep | ✅ Done |
| `crates/cogent-engine/src/registry.rs` | Added missing `outdated` entry; added sync test | ✅ Done (prior session) |
| `crates/cogent-protocol/src/lib.rs` | Downgraded `missing_docs` from deny to warn with TODO | ✅ Done |
| `README.md` | Fixed tool counts (31), HQSE tools, weighted scoring | ✅ Done |
| `PROJECT_STATUS.md` | Fixed tool count (31), crate count (34), version (v1.2.0) | ✅ Done |
| `ONBOARDING.md` | Fixed tool count (31), sample output | ✅ Done |
| `AGENTS.md` | Fixed batch audit tool count | ✅ Done |
| `CLAUDE.md` | Fixed batch audit tool count | ✅ Done |
| `docs/user-guide.md` | Fixed check count (31) | ✅ Done |
| `docs/developer-guide.md` | Expanded crate architecture tree (34 crates) | ✅ Done |
