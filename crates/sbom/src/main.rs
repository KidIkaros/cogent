#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(
    name = "sbom",
    about = "SBOM generator — produce CycloneDX 1.4 or SPDX 2.3 Software Bill of Materials"
)]
struct Cli {
    /// Project root path
    path: String,

    /// SBOM format: cyclonedx (default) or spdx
    #[arg(short, long, default_value = "cyclonedx")]
    format: String,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<String>,

    /// Project name override
    #[arg(long)]
    name: Option<String>,

    /// Project version override
    #[arg(long)]
    version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Component {
    name: String,
    version: String,
    license: String,
    ecosystem: String,
    purl: String,
}

fn iso_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let tod = secs % 86400;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    let year = 1970 + days / 365;
    let doy = days % 365;
    let month = 1 + doy / 30;
    let day = 1 + doy % 30;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hh, mm, ss
    )
}

fn parse_cargo_lock(root: &Path) -> Vec<Component> {
    let lock = root.join("Cargo.lock");
    let Ok(content) = std::fs::read_to_string(&lock) else {
        return vec![];
    };

    // Read workspace Cargo.toml files for license info
    let mut license_map: HashMap<String, String> = HashMap::new();
    collect_toml_licenses(root, &mut license_map);

    let mut components = Vec::new();
    let mut name = String::new();
    let mut version = String::new();

    for line in content.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            name.clear();
            version.clear();
        } else if let Some(v) = t.strip_prefix("name = \"") {
            name = v.trim_end_matches('"').to_string();
        } else if let Some(v) = t.strip_prefix("version = \"") {
            version = v.trim_end_matches('"').to_string();
            if !name.is_empty() {
                let lic = license_map
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| "NOASSERTION".to_string());
                let purl = format!("pkg:cargo/{}@{}", name, version);
                components.push(Component {
                    name: name.clone(),
                    version: version.clone(),
                    license: lic,
                    ecosystem: "cargo".to_string(),
                    purl,
                });
            }
        }
    }
    components
}

fn collect_toml_licenses(root: &Path, map: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_toml_licenses(&p, map);
        } else if p.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&p) {
                let mut pkg_name = String::new();
                let mut in_pkg = false;
                for line in content.lines() {
                    let t = line.trim();
                    if t == "[package]" {
                        in_pkg = true;
                    } else if t.starts_with('[') && t != "[package]" {
                        in_pkg = false;
                    }
                    if !in_pkg {
                        continue;
                    }
                    if let Some(v) = t.strip_prefix("name = \"") {
                        pkg_name = v.trim_end_matches('"').to_string();
                    } else if let Some(v) = t.strip_prefix("license = \"") {
                        let lic = v.trim_end_matches('"').to_string();
                        if !pkg_name.is_empty() {
                            map.insert(pkg_name.clone(), lic);
                        }
                    }
                }
            }
        }
    }
}

fn parse_npm_lock(root: &Path) -> Vec<Component> {
    let lock = root.join("package-lock.json");
    let Ok(content) = std::fs::read_to_string(&lock) else {
        // Fall back to package.json
        return parse_npm_package_json(root);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };

    let mut components = Vec::new();
    let packages = v.get("packages").or_else(|| v.get("dependencies"));
    if let Some(pkgs) = packages.and_then(|p| p.as_object()) {
        for (key, data) in pkgs {
            let name = key.trim_start_matches("node_modules/");
            if name.is_empty() {
                continue;
            }
            let ver = data
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let lic = data
                .get("license")
                .and_then(|l| l.as_str())
                .unwrap_or("NOASSERTION")
                .to_string();
            let purl = format!("pkg:npm/{}@{}", name, ver);
            components.push(Component {
                name: name.to_string(),
                version: ver,
                license: lic,
                ecosystem: "npm".to_string(),
                purl,
            });
        }
    }
    components
}

fn parse_npm_package_json(root: &Path) -> Vec<Component> {
    let pkg = root.join("package.json");
    let Ok(content) = std::fs::read_to_string(&pkg) else {
        return vec![];
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    let mut components = Vec::new();
    for section in &["dependencies", "devDependencies"] {
        if let Some(deps) = v.get(section).and_then(|d| d.as_object()) {
            for (name, ver_val) in deps {
                let ver = ver_val
                    .as_str()
                    .unwrap_or("?")
                    .trim_start_matches('^')
                    .trim_start_matches('~')
                    .to_string();
                let purl = format!("pkg:npm/{}@{}", name, ver);
                components.push(Component {
                    name: name.clone(),
                    version: ver,
                    license: "NOASSERTION".to_string(),
                    ecosystem: "npm".to_string(),
                    purl,
                });
            }
        }
    }
    components
}

fn parse_requirements(root: &Path) -> Vec<Component> {
    let req = root.join("requirements.txt");
    let Ok(content) = std::fs::read_to_string(&req) else {
        return vec![];
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (name, ver) = if let Some(pos) = l.find("==") {
                (&l[..pos], &l[pos + 2..])
            } else {
                let name = l.split(&['>', '<', '~', '^', '!'][..]).next().unwrap_or(l);
                (name, "?")
            };
            let purl = format!("pkg:pypi/{}@{}", name.trim().to_lowercase(), ver.trim());
            Component {
                name: name.trim().to_string(),
                version: ver.trim().to_string(),
                license: "NOASSERTION".to_string(),
                ecosystem: "pypi".to_string(),
                purl,
            }
        })
        .collect()
}

fn detect_project_name_version(root: &Path) -> (String, String) {
    // Try Cargo.toml
    if let Ok(c) = std::fs::read_to_string(root.join("Cargo.toml")) {
        let mut in_pkg = false;
        let mut name = String::new();
        let mut ver = String::new();
        for line in c.lines() {
            let t = line.trim();
            if t == "[package]" {
                in_pkg = true;
            } else if t.starts_with('[') && t != "[package]" {
                in_pkg = false;
            }
            if !in_pkg {
                continue;
            }
            if let Some(v) = t.strip_prefix("name = \"") {
                name = v.trim_end_matches('"').to_string();
            }
            if let Some(v) = t.strip_prefix("version = \"") {
                ver = v.trim_end_matches('"').to_string();
            }
        }
        if !name.is_empty() {
            return (name, ver);
        }
    }
    // Try package.json
    if let Ok(c) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c) {
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let ver = v
                .get("version")
                .and_then(|n| n.as_str())
                .unwrap_or("0.0.0")
                .to_string();
            return (name, ver);
        }
    }
    ("unknown".to_string(), "0.0.0".to_string())
}

fn render_cyclonedx(components: &[Component], name: &str, version: &str) -> String {
    let timestamp = iso_timestamp();
    let mut comps = String::new();
    for (i, c) in components.iter().enumerate() {
        let lic = if c.license == "NOASSERTION" || c.license.is_empty() {
            "<licenses><expression>NOASSERTION</expression></licenses>".to_string()
        } else {
            format!(
                "<licenses><license><id>{}</id></license></licenses>",
                xml_escape(&c.license)
            )
        };
        comps.push_str(&format!(
            r#"    <component type="library" bom-ref="comp-{i}">
      <name>{name}</name>
      <version>{version}</version>
      {lic}
      <purl>{purl}</purl>
    </component>
"#,
            i = i + 1,
            name = xml_escape(&c.name),
            version = xml_escape(&c.version),
            lic = lic,
            purl = xml_escape(&c.purl),
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<bom xmlns="http://cyclonedx.org/schema/bom/1.4" version="1">
  <metadata>
    <timestamp>{timestamp}</timestamp>
    <tools><tool><vendor>Cogent</vendor><name>sbom</name><version>1.0.0</version></tool></tools>
    <component type="application">
      <name>{name}</name>
      <version>{version}</version>
    </component>
  </metadata>
  <components>
{comps}  </components>
</bom>
"#,
        timestamp = timestamp,
        name = xml_escape(name),
        version = xml_escape(version),
        comps = comps,
    )
}

fn render_spdx(components: &[Component], name: &str, version: &str) -> String {
    let timestamp = iso_timestamp();
    let doc_name = format!("{}-{}", name, version);
    let mut packages = String::new();
    for (i, c) in components.iter().enumerate() {
        let spdx_id = format!("SPDXRef-Package-{}", i + 1);
        let lic = if c.license.is_empty() || c.license == "NOASSERTION" {
            "NOASSERTION".to_string()
        } else {
            c.license.clone()
        };
        packages.push_str(&format!(
            "\nPackageName: {name}\nSPDXID: {id}\nPackageVersion: {ver}\nPackageDownloadLocation: {purl}\nFilesAnalyzed: false\nPackageLicenseConcluded: {lic}\nPackageLicenseDeclared: {lic}\n",
            name = c.name, id = spdx_id, ver = c.version, purl = c.purl, lic = lic,
        ));
    }

    format!("SPDXVersion: SPDX-2.3\nDataLicense: CC0-1.0\nSPDXID: SPDXRef-DOCUMENT\nDocumentName: {doc_name}\nDocumentNamespace: https://cogent/sbom/{doc_name}-{ts}\nCreator: Tool: Cogent-sbom-1.0.0\nCreated: {ts}\n{packages}",
        doc_name = doc_name, ts = timestamp, packages = packages,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn run(cli: Cli) {
    let root = Path::new(&cli.path);
    let mut all: Vec<Component> = Vec::new();

    if root.join("Cargo.lock").exists() {
        all.extend(parse_cargo_lock(root));
    }
    if root.join("package-lock.json").exists() || root.join("package.json").exists() {
        all.extend(parse_npm_lock(root));
    }
    if root.join("requirements.txt").exists() {
        all.extend(parse_requirements(root));
    }

    // Deduplicate by purl
    let mut seen = std::collections::HashSet::new();
    all.retain(|c| seen.insert(c.purl.clone()));

    let (auto_name, auto_ver) = detect_project_name_version(root);
    let name = cli.name.as_deref().unwrap_or(&auto_name);
    let version = cli.version.as_deref().unwrap_or(&auto_ver);

    let output = match cli.format.as_str() {
        "spdx" => render_spdx(&all, name, version),
        _ => render_cyclonedx(&all, name, version),
    };

    if let Some(out_path) = &cli.output {
        std::fs::write(out_path, &output).expect("Failed to write SBOM");
        eprintln!("SBOM written to {} ({} components)", out_path, all.len());
    } else {
        print!("{}", output);
        eprintln!("SBOM generated: {} components", all.len());
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyclonedx_contains_header() {
        let comps = vec![Component {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            license: "MIT OR Apache-2.0".to_string(),
            ecosystem: "cargo".to_string(),
            purl: "pkg:cargo/serde@1.0.0".to_string(),
        }];
        let xml = render_cyclonedx(&comps, "myapp", "1.0.0");
        assert!(xml.contains("cyclonedx.org/schema/bom/1.4"));
        assert!(xml.contains("serde"));
        assert!(xml.contains("MIT OR Apache-2.0"));
    }

    #[test]
    fn test_spdx_contains_header() {
        let comps = vec![Component {
            name: "tokio".to_string(),
            version: "1.0.0".to_string(),
            license: "MIT".to_string(),
            ecosystem: "cargo".to_string(),
            purl: "pkg:cargo/tokio@1.0.0".to_string(),
        }];
        let spdx = render_spdx(&comps, "myapp", "2.0.0");
        assert!(spdx.contains("SPDXVersion: SPDX-2.3"));
        assert!(spdx.contains("tokio"));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("a & <b>"), "a &amp; &lt;b&gt;");
    }

    #[test]
    fn test_iso_timestamp_format() {
        let ts = iso_timestamp();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
    }

    use super::*;

    // ── parse_cargo_lock ──

    #[test]
    fn test_parse_cargo_lock_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let comps = parse_cargo_lock(dir.path());
        assert!(comps.is_empty());
    }

    #[test]
    fn test_parse_cargo_lock_with_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), r#"
[[package]]
name = "serde"
version = "1.0.0"

[[package]]
name = "tokio"
version = "0.2.0"
"#).unwrap();
        let comps = parse_cargo_lock(dir.path());
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].name, "serde");
        assert_eq!(comps[0].ecosystem, "cargo");
        assert!(comps[0].purl.contains("cargo/serde"));
    }

    // ── parse_requirements ──

    #[test]
    fn test_parse_requirements_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let comps = parse_requirements(dir.path());
        assert!(comps.is_empty());
    }

    #[test]
    fn test_parse_requirements_with_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.28.0\nflask>=1.0\n").unwrap();
        let comps = parse_requirements(dir.path());
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].name, "requests");
        assert_eq!(comps[0].version, "2.28.0");
        assert_eq!(comps[0].ecosystem, "pypi");
    }

    // ── detect_project_name_version ──

    #[test]
    fn test_detect_project_name_version_from_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), r#"
[package]
name = "myapp"
version = "1.2.3"
"#).unwrap();
        let (name, ver) = detect_project_name_version(dir.path());
        assert_eq!(name, "myapp");
        assert_eq!(ver, "1.2.3");
    }

    #[test]
    fn test_detect_project_name_version_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "my-app", "version": "2.0.0"}"#).unwrap();
        let (name, ver) = detect_project_name_version(dir.path());
        assert_eq!(name, "my-app");
        assert_eq!(ver, "2.0.0");
    }

    #[test]
    fn test_detect_project_name_version_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let (name, ver) = detect_project_name_version(dir.path());
        assert_eq!(name, "unknown");
        assert_eq!(ver, "0.0.0");
    }

    // ── parse_npm_lock ──

    #[test]
    fn test_parse_npm_lock_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let comps = parse_npm_lock(dir.path());
        assert!(comps.is_empty());
    }

    #[test]
    fn test_parse_npm_lock_with_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{
  "dependencies": {"lodash": "^4.17.0"},
  "devDependencies": {"mocha": "^9.0.0"}
}"#).unwrap();
        let comps = parse_npm_lock(dir.path());
        assert_eq!(comps.len(), 2);
        assert!(comps.iter().any(|c| c.name == "lodash"));
        assert!(comps.iter().any(|c| c.name == "mocha"));
    }

    #[test]
    fn test_parse_npm_lock_with_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package-lock.json"), r#"{
  "packages": {
    "node_modules/lodash": {"version": "4.17.21", "license": "MIT"},
    "node_modules/express": {"version": "4.18.0", "license": "MIT"}
  }
}"#).unwrap();
        let comps = parse_npm_lock(dir.path());
        assert_eq!(comps.len(), 2);
        let lodash = comps.iter().find(|c| c.name == "lodash").unwrap();
        assert_eq!(lodash.version, "4.17.21");
        assert_eq!(lodash.license, "MIT");
        let express = comps.iter().find(|c| c.name == "express").unwrap();
        assert_eq!(express.version, "4.18.0");
    }
}
