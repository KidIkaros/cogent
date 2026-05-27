# SOC 2 Type II Control Mapping

Cogent tools map to AICPA Trust Services Criteria (TSC) controls as follows. Run `cogent check . --framework soc2` to generate a SOC2-aligned report.

## Common Criteria (CC)

| TSC Control | Control Description | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| CC6.1 | Logical access security — proper authorization checks | `access-control` | Missing auth checks |
| CC6.2 | Prior to access, register and authorize users | `access-control` | Unprotected endpoints |
| CC6.3 | Access removal on termination | `access-control` | Orphaned permissions |
| CC7.1 | Detect security events and anomalies | `sast`, `taint`, `secrets` | Code-level vulnerabilities |
| CC7.2 | System operations monitoring | `errhandle`, `typecov` | Unhandled errors, missing types |
| CC7.3 | Evaluate security event anomalies | `vulnscan`, `crypto` | Known CVEs, weak crypto |
| CC7.4 | Incident detection and response | `sast` | Injection patterns |
| CC8.1 | Change management process | `debt`, `deadcode` | TODO/FIXME markers, unused code |
| CC8.2 | Changes authorized, tested, approved | `debt` | Unaddressed technical debt |
| CC3.1 | Risk assessment process | `riskmap` | Churn × complexity hotspots |
| CC3.2 | Fraud risk analysis | `sast`, `secrets` | Injection, credential leakage |
| CC2.1 | Identify and communicate information | `doccov` | Missing API documentation |

## Availability (A)

| TSC Control | Control Description | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| A1.1 | System availability monitoring | `errhandle` | Unhandled errors that could crash systems |
| A1.2 | Recovery point objective achieved | `debt` | Technical debt that impacts recoverability |

## Processing Integrity (PI)

| TSC Control | Control Description | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| PI1.1 | Processing authorization | `access-control` | Missing authorization checks |
| PI1.2 | Complete and valid processing | `sast` | Logic flaws, injection risks |

## Confidentiality (C)

| TSC Control | Control Description | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| C1.1 | Confidential information identification | `taint` | Data flow tracking for sensitive data |
| C1.2 | Confidential information access | `access-control` | Missing access controls |

## Privacy (P)

| TSC Control | Control Description | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| P1.1 | Personal information collection | `taint`, `sast` | PII detection in code |
| P2.1 | Personal information use | `taint` | Improper data flows |

## Generating SOC2 Reports

```bash
# Run all checks with SOC2 control mapping
cogent check . --framework soc2 --format json

# Generate attestation-ready HTML report
cogent check . --framework soc2 --format html
```

The `--framework soc2` flag adds a `controls` array to each finding showing the mapped TSC control IDs.

## Example Output

```json
{
  "name": "secrets",
  "passed": true,
  "controls": ["CC7.1", "CC3.2", "C1.1"],
  "findings": []
}
```
