# Cogent Reporting Formats & CI Integration

Cogent supports multiple output formats for `cogent check`, `cogent diff`, and `cogent history`. This guide covers all formats and how to integrate them into CI/CD pipelines.

---

## `cogent check` Output Formats

Use `--format <format>` to select an output format. The default is `json`.

| Format | Description | Use Case |
|--------|-------------|----------|
| `json` | Full `CheckReport` JSON object | Default CI artifact |
| `text` | Human-readable terminal output with progress | Local development |
| `sarif` | SARIF 2.1.0 static analysis results | GitHub Security tab upload |
| `junit` | JUnit XML with `<testsuite>` per tool | Jenkins, GitLab CI dashboards |
| `findings` | NDJSON stream of structured `Finding` objects | Elasticsearch, data warehouses |
| `ndjson` | Same as `findings` (alias) | Backward compatibility |
| `markdown` | Full Markdown report with file heatmap | Wiki, email, Slack |

### Examples

```bash
# Default JSON
cogent check . --format json > report.json

# SARIF for GitHub Security tab
cogent check . --format sarif > results.sarif

# JUnit for Jenkins
cogent check . --format junit > test-results.xml

# NDJSON findings for Elasticsearch
cogent check . --format findings | jq .

# Markdown report
cogent check . --format markdown > report.md
```

---

## `--pr-comment` — GitHub PR Comments

Generate a markdown snippet optimized for posting as a GitHub PR comment. Includes collapsible per-tool sections, file heatmap, and summary badges.

```bash
cogent check . --pr-comment > pr-comment.md
# Paste the output into a GitHub PR comment
```

Features:
- ✅/❌ overall status badge
- Health score and grade
- Summary table (total/passed/failed checks)
- Collapsible `<details>` per failed check with findings table
- Top 10 files by issue count

---

## `--ci` — CI Artifact Mode

When `--ci` is passed, Cogent:
1. Forces JSON output to stdout
2. Suppresses TTY progress bars
3. Writes `cogent-summary.json` with high-level metadata
4. Writes `cogent-report.html` with the full interactive report

```bash
cogent check . --ci
# stdout: full JSON report
# cogent-summary.json: CI-parseable metadata
# cogent-report.html: interactive HTML report
```

### `cogent-summary.json` Schema

```json
{
  "passed": false,
  "score": 72,
  "grade": "C",
  "failed_checks": ["debt", "doccov"],
  "critical_findings": 0,
  "report_url": "./cogent-report.html"
}
```

---

## `cogent diff` — Compare Runs

Compare two check JSON snapshots.

```bash
# Text diff (default)
cogent diff report-before.json report-after.json

# HTML visual diff
cogent diff report-before.json report-after.json --format html > diff.html
```

The HTML diff report includes:
- Side-by-side health score cards
- Summary of regressions, fixes, and new checks
- Per-check comparison table with pass/fail status and change indicators

---

## `cogent history` — Trend Dashboard

Show historical trends from `.cogent-history/`.

```bash
# Text table (default)
cogent history show --last 20

# Interactive HTML trend dashboard
cogent history show --format html > history.html
```

The HTML history page includes:
- Health score sparkline over time
- Per-run details table with trend arrows (↑/↓/→)

---

## `cogent serve` — Local Report Server

Start a tiny HTTP server to browse reports.

```bash
cogent serve --port 8080
```

Endpoints:
- `/` — Report index with auto-refresh (30s)
- `/latest` — Latest `cogent-report.html` or `check-report.html`
- `/api/latest` — Latest JSON summary/report
- `/report/<file>` — Individual historical JSON report

---

## CI Integration Examples

### GitHub Actions

```yaml
- name: Run Cogent Check
  run: cogent check . --ci

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: results.sarif

- name: Post PR Comment
  if: github.event_name == 'pull_request'
  run: |
    cogent check . --pr-comment > comment.md
    gh pr comment ${{ github.event.pull_request.number }} --body-file comment.md
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### GitLab CI

```yaml
quality:
  script:
    - cogent check . --format junit > junit-report.xml
  artifacts:
    reports:
      junit: junit-report.xml
```

---

## Report Schemas

- `schemas/check-report.schema.json` — `CheckReport` with `findings` and `file_summary`
- `schemas/tool-response.schema.json` — Individual tool responses

---

## File Heatmap

The file heatmap aggregates findings across all tools to show the "hottest" files:
- **issue_count**: Total findings in the file
- **severity_score**: Weighted score (critical=4, high=3, medium=2, low=1)

In HTML reports, click a file in the heatmap to filter all findings to that file.

---

## Interactive HTML Features

- **Severity distribution chart** — Inline SVG bar chart of findings by severity
- **Health score gauge** — Semi-circle gauge showing 0-100 health score
- **Trend sparkline** — Mini line chart of health scores from `.cogent-history/`
- **Collapsible findings** — Click a tool header to expand/collapse its findings table
- **Findings search** — Real-time filter across all findings
- **File heatmap click-to-filter** — Click any file to isolate its findings
