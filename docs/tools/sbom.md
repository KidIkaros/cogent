# sbom — Software Bill of Materials

Generates a CycloneDX SBOM listing all dependencies and their metadata.

## What it generates

- Complete dependency inventory
- License information for each dependency
- Version numbers and hashes
- Supplier/vendor information

## Output format

CycloneDX XML (default) or JSON.

## Examples

```bash
sbom ./
sbom ./ --format json
```

## Use cases

- Supply chain security audits
- License compliance reporting
- Vulnerability tracking
- Regulatory compliance (FedRAMP, SOC2)
