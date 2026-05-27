# ISO 27001:2022 Control Mapping

Cogent tools map to ISO 27001:2022 Annex A controls as follows.

| ISO Control | Title | Cogent Tool(s) | Finding Type |
|---|---|---|---|
| A.5.1 | Policies for information security | `debt` | Policy gaps (TODO markers in security docs) |
| A.5.7 | Threat intelligence | `vulnscan` | Known CVEs in dependencies |
| A.5.8 | Information security in project management | `debt` | Unaddressed security debt |
| A.5.9 | Inventory of information and other associated assets | `sbom` | Dependency inventory |
| A.5.24 | Information security incident management planning | `errhandle` | Error handling gaps |
| A.5.37 | Documented operating procedures | `doccov` | Missing documentation |
| A.6.1 | Screening | `access-control` | Missing access controls |
| A.6.3 | Information security awareness | `debt` | Security training TODOs |
| A.7.1 | Physical security perimeters | `access-control` | Network boundary checks |
| A.7.4 | Physical security monitoring | `access-control` | Logging gaps |
| A.8.1 | User endpoint devices | `sast` | Client-side vulnerabilities |
| A.8.4 | Removal of assets | `access-control` | Asset decommissioning checks |
| A.8.9 | Configuration management | `debt` | Configuration drift markers |
| A.8.10 | Information deletion | `taint` | Data retention policy gaps |
| A.8.11 | Data masking | `taint` | Missing data masking |
| A.8.12 | Data leakage prevention | `secrets`, `taint` | Hardcoded secrets, data exfiltration |
| A.8.15 | Logging | `errhandle` | Missing error logging |
| A.8.16 | Monitoring activities | `riskmap` | Monitoring coverage gaps |
| A.8.23 | Web filtering | `sast` | SSRF, open redirect |
| A.8.24 | Use of cryptography | `crypto` | Weak encryption, deprecated algorithms |
| A.8.25 | Secure development life cycle | `sast`, `debt`, `doccov` | Code review gaps |
| A.8.26 | Application security requirements | `sast`, `fuzz` | Input validation gaps |
| A.8.27 | Secure system architecture | `coupling` | Architectural coupling risks |
| A.8.28 | Secure coding | `sast`, `errhandle`, `typecov` | Secure coding violations |
| A.8.29 | Security testing in development | `mutate`, `fuzz` | Test coverage gaps |
| A.8.30 | Outsourced development | `supply-chain`, `licenses` | Third-party risks |
| A.8.31 | Separation of development and production | `access-control` | Environment separation |
| A.8.32 | Change management | `debt` | Change tracking gaps |
| A.8.33 | Test information | `fuzz` | Test data protection |

## Generating ISO 27001 Reports

```bash
cogent check . --framework iso27001 --format json
cogent check . --framework iso27001 --format html
```

## Example Output

```json
{
  "name": "crypto",
  "passed": true,
  "controls": ["A.8.24"],
  "findings": []
}
```
