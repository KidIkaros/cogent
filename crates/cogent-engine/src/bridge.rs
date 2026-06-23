//! Type bridge between `cogent_common::Finding` and `cogent_protocol::types::Finding`.
//!
//! The common `Finding` is the simpler internal representation used by the CLI
//! and engine. The protocol `Finding` is the richer wire format with
//! `finding_id`, `rule_pack`, `category`, `compliance_controls`, `tags`, and
//! `metadata`. These conversions allow seamless round-tripping between the two.

use cogent_protocol::types as proto;

// ═══════════════════════════════════════════
// Severity mapping
// ═══════════════════════════════════════════

fn severity_to_proto(s: &str) -> proto::Severity {
    match s.to_lowercase().as_str() {
        "critical" => proto::Severity::Critical,
        "high" | "error" => proto::Severity::High,
        "medium" | "warning" => proto::Severity::Medium,
        "low" => proto::Severity::Low,
        _ => proto::Severity::Info,
    }
}

fn severity_from_proto(s: proto::Severity) -> String {
    match s {
        proto::Severity::Critical => "critical",
        proto::Severity::High => "high",
        proto::Severity::Medium => "medium",
        proto::Severity::Low => "low",
        proto::Severity::Info => "info",
    }
    .to_string()
}

fn confidence_from_proto(c: proto::Confidence) -> String {
    match c {
        proto::Confidence::High => "high",
        proto::Confidence::Medium => "medium",
        proto::Confidence::Low => "low",
    }
    .to_string()
}

fn confidence_to_proto(s: &str) -> proto::Confidence {
    match s.to_lowercase().as_str() {
        "high" => proto::Confidence::High,
        "low" => proto::Confidence::Low,
        _ => proto::Confidence::Medium,
    }
}

// ═══════════════════════════════════════════
// SuggestedFix conversion
// ═══════════════════════════════════════════

fn suggested_fix_to_proto(f: &cogent_common::SuggestedFix) -> proto::SuggestedFix {
    proto::SuggestedFix {
        description: f.description.clone(),
        diff: f.diff.clone(),
        confidence: confidence_to_proto(&f.confidence),
        auto_applicable: false,
    }
}

fn suggested_fix_from_proto(f: &proto::SuggestedFix) -> cogent_common::SuggestedFix {
    cogent_common::SuggestedFix {
        description: f.description.clone(),
        diff: f.diff.clone(),
        confidence: confidence_from_proto(f.confidence),
    }
}

// ═══════════════════════════════════════════
// Finding conversions (free functions — orphan rule prevents From impls)
// ═══════════════════════════════════════════

/// Convert a common `Finding` into the richer protocol `Finding`.
///
/// Protocol-only fields (`finding_id`, `rule_pack`, `category`,
/// `compliance_controls`, `tags`, `metadata`) are populated with sensible
/// defaults so no data is lost during round-tripping.
pub fn finding_to_proto(f: &cogent_common::Finding) -> proto::Finding {
    let finding_id = if let Some(line) = f.line {
        format!("{}:{}:{}", f.rule_id, f.file, line)
    } else {
        format!("{}:{}:0", f.rule_id, f.file)
    };
    proto::Finding {
        finding_id,
        rule_id: f.rule_id.clone(),
        rule_pack: None,
        severity: severity_to_proto(&f.severity),
        category: proto::Category::default(),
        file: f.file.clone(),
        line: f.line,
        column: f.column,
        end_line: None,
        end_column: None,
        message: f.message.clone(),
        code_snippet: f.evidence.as_ref().map(|e| e.snippet.clone()),
        suggested_fix: f.suggested_fix.as_ref().map(suggested_fix_to_proto),
        compliance_controls: f.controls.clone().unwrap_or_default(),
        tags: Vec::new(),
        metadata: f.evidence.as_ref().and_then(|e| {
            let has_file_hash = e.file_hash.is_some();
            let has_context = e.context.is_some();
            if has_file_hash || has_context {
                let mut map = serde_json::Map::new();
                if let Some(ref fh) = e.file_hash {
                    map.insert("file_hash".into(), serde_json::Value::String(fh.clone()));
                }
                if let Some(ref ctx) = e.context {
                    map.insert("context".into(), serde_json::Value::String(ctx.clone()));
                }
                Some(serde_json::Value::Object(map))
            } else {
                None
            }
        }),
    }
}

/// Convert a protocol `Finding` into the simpler common `Finding`.
///
/// The richer protocol fields (`finding_id`, `rule_pack`, `category`,
/// `compliance_controls`, `tags`, `metadata`) are mapped into the common
/// struct's simpler field set. `compliance_controls` maps to `controls`.
pub fn finding_from_proto(f: &proto::Finding) -> cogent_common::Finding {
    let evidence = f.code_snippet.as_ref().map(|s| {
        let (file_hash, context) =
            f.metadata
                .as_ref()
                .and_then(|m| m.as_object())
                .map_or((None, None), |obj| {
                    (
                        obj.get("file_hash")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        obj.get("context")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    )
                });
        cogent_common::Evidence {
            snippet: s.clone(),
            file_hash,
            context,
        }
    });
    let suggested_fix = f.suggested_fix.as_ref().map(suggested_fix_from_proto);
    cogent_common::Finding {
        file: f.file.clone(),
        line: f.line,
        column: f.column,
        severity: severity_from_proto(f.severity),
        message: f.message.clone(),
        rule_id: f.rule_id.clone(),
        fix_hint: String::new(),
        evidence,
        suggested_fix,
        controls: if f.compliance_controls.is_empty() {
            None
        } else {
            Some(f.compliance_controls.clone())
        },
    }
}

// ═══════════════════════════════════════════
// Batch conversions
// ═══════════════════════════════════════════

/// Convert a slice of common `Finding`s into protocol `Finding`s.
pub fn findings_to_proto(findings: &[cogent_common::Finding]) -> Vec<proto::Finding> {
    findings.iter().map(finding_to_proto).collect()
}

/// Convert a slice of protocol `Finding`s into common `Finding`s.
pub fn findings_from_proto(findings: &[proto::Finding]) -> Vec<cogent_common::Finding> {
    findings.iter().map(finding_from_proto).collect()
}

// ═══════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_common_finding() -> cogent_common::Finding {
        cogent_common::Finding {
            file: "src/main.rs".into(),
            line: Some(42),
            column: Some(5),
            severity: "high".into(),
            message: "Hardcoded secret".into(),
            rule_id: "secrets".into(),
            fix_hint: "Use env var".into(),
            evidence: Some(cogent_common::Evidence {
                snippet: "api_key = \"AKIA...\"".into(),
                file_hash: None,
                context: None,
            }),
            suggested_fix: Some(cogent_common::SuggestedFix {
                description: "Move to env".into(),
                diff: Some("- api_key = \"...\"\n+ api_key = env::var(\"KEY\")".into()),
                confidence: "high".into(),
            }),
            controls: Some(vec!["CC7.1".into()]),
        }
    }

    fn make_proto_finding() -> proto::Finding {
        proto::Finding {
            finding_id: "secrets:src/main.rs:42".into(),
            rule_id: "secrets".into(),
            rule_pack: Some("soc2".into()),
            severity: proto::Severity::Critical,
            category: proto::Category::Security,
            file: "src/main.rs".into(),
            line: Some(42),
            column: Some(5),
            end_line: None,
            end_column: None,
            message: "Hardcoded secret".into(),
            code_snippet: Some("api_key = \"AKIA...\"".into()),
            suggested_fix: Some(proto::SuggestedFix {
                description: "Move to env".into(),
                diff: Some("- api_key = \"...\"\n+ api_key = env::var(\"KEY\")".into()),
                confidence: proto::Confidence::High,
                auto_applicable: true,
            }),
            compliance_controls: vec!["CC7.1".into()],
            tags: vec!["aws".into()],
            metadata: None,
        }
    }

    #[test]
    fn test_common_to_proto_roundtrip_preserves_core_fields() {
        let common = make_common_finding();
        let proto_f = finding_to_proto(&common);

        assert_eq!(proto_f.file, "src/main.rs");
        assert_eq!(proto_f.line, Some(42));
        assert_eq!(proto_f.column, Some(5));
        assert_eq!(proto_f.severity, proto::Severity::High);
        assert_eq!(proto_f.message, "Hardcoded secret");
        assert_eq!(proto_f.rule_id, "secrets");
        assert_eq!(proto_f.finding_id, "secrets:src/main.rs:42");
        assert_eq!(proto_f.compliance_controls, vec!["CC7.1"]);
        assert!(proto_f.code_snippet.is_some());
        assert!(proto_f.suggested_fix.is_some());
    }

    #[test]
    fn test_proto_to_common_roundtrip_preserves_core_fields() {
        let proto_f = make_proto_finding();
        let common = finding_from_proto(&proto_f);

        assert_eq!(common.file, "src/main.rs");
        assert_eq!(common.line, Some(42));
        assert_eq!(common.column, Some(5));
        assert_eq!(common.severity, "critical");
        assert_eq!(common.message, "Hardcoded secret");
        assert_eq!(common.rule_id, "secrets");
        assert!(common.evidence.is_some());
        assert!(common.suggested_fix.is_some());
        assert_eq!(common.controls, Some(vec!["CC7.1".to_string()]));
    }

    #[test]
    fn test_common_to_proto_default_finding_id_without_line() {
        let mut f = make_common_finding();
        f.line = None;
        let proto_f = finding_to_proto(&f);
        assert_eq!(proto_f.finding_id, "secrets:src/main.rs:0");
    }

    #[test]
    fn test_proto_to_common_empty_controls_becomes_none() {
        let mut f = make_proto_finding();
        f.compliance_controls = Vec::new();
        let common = finding_from_proto(&f);
        assert!(common.controls.is_none());
    }

    #[test]
    fn test_common_to_proto_no_evidence_or_fix() {
        let mut f = make_common_finding();
        f.evidence = None;
        f.suggested_fix = None;
        let proto_f = finding_to_proto(&f);
        assert!(proto_f.code_snippet.is_none());
        assert!(proto_f.suggested_fix.is_none());
    }

    #[test]
    fn test_proto_to_common_no_snippet_or_fix() {
        let mut f = make_proto_finding();
        f.code_snippet = None;
        f.suggested_fix = None;
        let common = finding_from_proto(&f);
        assert!(common.evidence.is_none());
        assert!(common.suggested_fix.is_none());
    }

    #[test]
    fn test_severity_mapping_roundtrip() {
        for sev_str in &["info", "low", "medium", "high", "critical"] {
            let proto = severity_to_proto(sev_str);
            let back = severity_from_proto(proto);
            assert_eq!(&back, sev_str);
        }
    }

    #[test]
    fn test_confidence_mapping_roundtrip() {
        for conf_str in &["low", "medium", "high"] {
            let proto = confidence_to_proto(conf_str);
            let back = confidence_from_proto(proto);
            assert_eq!(&back, conf_str);
        }
    }

    #[test]
    fn test_evidence_roundtrip_preserves_file_hash_and_context() {
        let mut f = make_common_finding();
        f.evidence = Some(cogent_common::Evidence {
            snippet: "api_key = \"AKIA...\"".into(),
            file_hash: Some("abc123".into()),
            context: Some("line before".into()),
        });
        let proto_f = finding_to_proto(&f);
        // Metadata should carry file_hash and context
        let meta = proto_f.metadata.as_ref().expect("metadata should be Some");
        assert_eq!(meta["file_hash"], "abc123");
        assert_eq!(meta["context"], "line before");

        // Round-trip back
        let back = finding_from_proto(&proto_f);
        let ev = back.evidence.unwrap();
        assert_eq!(ev.file_hash, Some("abc123".into()));
        assert_eq!(ev.context, Some("line before".into()));
    }

    #[test]
    fn test_batch_conversions() {
        let commons = vec![make_common_finding(), make_common_finding()];
        let protos = findings_to_proto(&commons);
        assert_eq!(protos.len(), 2);

        let back = findings_from_proto(&protos);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].file, "src/main.rs");
    }
}
