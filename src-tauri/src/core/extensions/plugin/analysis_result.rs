use std::collections::HashMap;

use crate::core::analysis::results::models::*;
use rmpv::Value;
use serde::Deserialize;

use super::{PluginError, Result};

const MAX_RESULT_ITEMS: usize = 10_000;
const MAX_SUMMARY_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_ATTRIBUTES: usize = 64;
const MAX_VALUE_DEPTH: usize = 8;
const MAX_VALUE_ENTRIES: usize = 4_096;
const MAX_RESULT_NODES: usize = 64;
const MAX_NODE_COLUMNS: usize = 64;

#[derive(Deserialize)]
struct AnalyzerResponseV1 {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    nodes: Vec<NodeV1>,
    #[serde(default)]
    artifacts: Vec<ArtifactV1>,
    #[serde(default)]
    timeline: Vec<TimelineEventV1>,
    #[serde(default)]
    findings: Vec<FindingV1>,
}

#[derive(Deserialize)]
struct NodeV1 {
    #[serde(rename = "type")]
    node_type: String,
    name: String,
    #[serde(default)]
    columns: Vec<NodeColumnV1>,
}

#[derive(Deserialize)]
struct NodeColumnV1 {
    key: String,
    label: String,
}

#[derive(Deserialize)]
struct ArtifactV1 {
    #[serde(rename = "type")]
    artifact_type: String,
    title: String,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    attributes: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct TimelineEventV1 {
    timestamp: String,
    #[serde(rename = "type")]
    event_type: String,
    description: String,
    #[serde(default)]
    attributes: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct FindingV1 {
    severity: String,
    title: String,
    description: String,
    #[serde(default)]
    attributes: HashMap<String, Value>,
}

pub(super) fn parse_analysis_result(value: Value) -> Result<PluginAnalysisResult> {
    let response: AnalyzerResponseV1 =
        rmpv::ext::from_value(value).map_err(|error| PluginError::Rpc {
            code: "INVALID_ANALYSIS_RESULT".to_owned(),
            message: error.to_string(),
        })?;
    let item_count = response
        .artifacts
        .len()
        .checked_add(response.timeline.len())
        .and_then(|count| count.checked_add(response.findings.len()))
        .ok_or_else(result_too_large)?;
    if item_count > MAX_RESULT_ITEMS {
        return Err(result_too_large());
    }
    if response.nodes.len() > MAX_RESULT_NODES {
        return Err(result_too_large());
    }
    if let Some(summary) = &response.summary {
        validate_text("summary", summary, MAX_SUMMARY_BYTES)?;
    }

    let nodes = response
        .nodes
        .into_iter()
        .map(|node| {
            validate_type("result node type", &node.node_type)?;
            validate_text("result node name", &node.name, 128)?;
            if node.columns.len() > MAX_NODE_COLUMNS {
                return Err(result_too_large());
            }
            let columns = node
                .columns
                .into_iter()
                .map(|column| {
                    validate_type("result node column key", &column.key)?;
                    validate_text("result node column label", &column.label, 128)?;
                    Ok(PluginResultNodeColumn {
                        key: column.key,
                        label: column.label,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(PluginResultNode {
                node_type: node.node_type,
                display_name: node.name,
                columns,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let artifacts = response
        .artifacts
        .into_iter()
        .map(|artifact| {
            validate_type("artifact type", &artifact.artifact_type)?;
            validate_text("artifact title", &artifact.title, MAX_TEXT_BYTES)?;
            Ok(PluginAnalysisArtifact {
                artifact_type: artifact.artifact_type,
                title: artifact.title,
                value: artifact.value.map(value_to_json).transpose()?,
                attributes: attributes_to_json(artifact.attributes)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let timeline = response
        .timeline
        .into_iter()
        .map(|event| {
            chrono::DateTime::parse_from_rfc3339(&event.timestamp)
                .map_err(|error| invalid_result(format!("invalid timeline timestamp: {error}")))?;
            validate_type("timeline event type", &event.event_type)?;
            validate_text("timeline description", &event.description, MAX_TEXT_BYTES)?;
            Ok(PluginAnalysisTimelineEvent {
                timestamp: event.timestamp,
                event_type: event.event_type,
                description: event.description,
                attributes: attributes_to_json(event.attributes)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let findings = response
        .findings
        .into_iter()
        .map(|finding| {
            validate_text("finding title", &finding.title, MAX_TEXT_BYTES)?;
            validate_text("finding description", &finding.description, MAX_TEXT_BYTES)?;
            Ok(PluginAnalysisFinding {
                severity: parse_finding_severity(&finding.severity)?,
                title: finding.title,
                description: finding.description,
                attributes: attributes_to_json(finding.attributes)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PluginAnalysisResult {
        summary: response.summary,
        nodes,
        artifacts,
        timeline,
        findings,
    })
}

fn parse_finding_severity(value: &str) -> Result<PluginFindingSeverity> {
    match value {
        "info" => Ok(PluginFindingSeverity::Info),
        "low" => Ok(PluginFindingSeverity::Low),
        "medium" => Ok(PluginFindingSeverity::Medium),
        "high" => Ok(PluginFindingSeverity::High),
        "critical" => Ok(PluginFindingSeverity::Critical),
        _ => Err(invalid_result(format!(
            "finding severity must be one of info, low, medium, high, or critical; got {value}"
        ))),
    }
}

fn attributes_to_json(
    attributes: HashMap<String, Value>,
) -> Result<HashMap<String, serde_json::Value>> {
    if attributes.len() > MAX_ATTRIBUTES {
        return Err(result_too_large());
    }
    attributes
        .into_iter()
        .map(|(key, value)| {
            validate_type("attribute key", &key)?;
            Ok((key, value_to_json(value)?))
        })
        .collect()
}

fn value_to_json(value: Value) -> Result<serde_json::Value> {
    let mut entries = 0_usize;
    value_to_json_inner(value, 0, &mut entries)
}

fn value_to_json_inner(
    value: Value,
    depth: usize,
    entries: &mut usize,
) -> Result<serde_json::Value> {
    if depth > MAX_VALUE_DEPTH {
        return Err(result_too_large());
    }
    *entries = entries.checked_add(1).ok_or_else(result_too_large)?;
    if *entries > MAX_VALUE_ENTRIES {
        return Err(result_too_large());
    }
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(value) => Ok(value.into()),
        Value::Integer(value) => {
            if let Some(value) = value.as_i64() {
                Ok(value.into())
            } else if let Some(value) = value.as_u64() {
                Ok(value.into())
            } else {
                Err(invalid_result("integer is outside JSON range"))
            }
        }
        Value::F32(value) => finite_number(f64::from(value)),
        Value::F64(value) => finite_number(value),
        Value::String(value) => value
            .into_str()
            .map(serde_json::Value::String)
            .ok_or_else(|| invalid_result("string is not valid UTF-8")),
        Value::Binary(_) => Err(invalid_result(
            "binary values are not allowed in structured results",
        )),
        Value::Array(values) => values
            .into_iter()
            .map(|value| value_to_json_inner(value, depth + 1, entries))
            .collect::<Result<Vec<_>>>()
            .map(serde_json::Value::Array),
        Value::Map(values) => {
            let mut object = serde_json::Map::new();
            for (key, value) in values {
                let key = key
                    .as_str()
                    .ok_or_else(|| invalid_result("object keys must be strings"))?;
                if object
                    .insert(
                        key.to_owned(),
                        value_to_json_inner(value, depth + 1, entries)?,
                    )
                    .is_some()
                {
                    return Err(invalid_result("object contains duplicate keys"));
                }
            }
            Ok(serde_json::Value::Object(object))
        }
        Value::Ext(_, _) => Err(invalid_result(
            "extension values are not allowed in structured results",
        )),
    }
}

fn finite_number(value: f64) -> Result<serde_json::Value> {
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .ok_or_else(|| invalid_result("floating-point value must be finite"))
}

fn validate_type(label: &str, value: &str) -> Result<()> {
    validate_text(label, value, 128)?;
    if !value.contains('.')
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return Err(invalid_result(format!(
            "{label} must be a namespaced lowercase identifier"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, maximum: usize) -> Result<()> {
    if value.is_empty() || value.len() > maximum {
        return Err(invalid_result(format!(
            "{label} length must be within 1..={maximum} bytes"
        )));
    }
    Ok(())
}

fn invalid_result(message: impl Into<String>) -> PluginError {
    PluginError::Rpc {
        code: "INVALID_ANALYSIS_RESULT".to_owned(),
        message: message.into(),
    }
}

fn result_too_large() -> PluginError {
    PluginError::ResourceLimit("plugin analysis result exceeds LPP v1 limits".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_wire_result_preserves_host_json_projection() {
        use lovelycarve_plugin::{AnalysisResult, Artifact, Attributes, ResultColumn, ResultNode};
        let result = AnalysisResult {
            summary: Some("发现用户".to_owned()),
            nodes: vec![ResultNode {
                node_type: "linux.user".to_owned(),
                name: "用户".to_owned(),
                columns: vec![ResultColumn {
                    key: "linux.uid".to_owned(),
                    label: "UID".to_owned(),
                }],
            }],
            artifacts: vec![Artifact {
                artifact_type: "linux.user".to_owned(),
                title: "root".to_owned(),
                value: Some(Value::from(0)),
                attributes: Attributes::from_iter([("linux.uid", Value::from(0))]),
            }],
            ..AnalysisResult::default()
        };
        let projection = parse_analysis_result(result.to_value()).expect("SDK result");
        assert_eq!(
            serde_json::to_value(projection).expect("IPC projection"),
            serde_json::json!({
                "summary": "发现用户",
                "nodes": [{"nodeType": "linux.user", "displayName": "用户",
                    "columns": [{"key": "linux.uid", "label": "UID"}]}],
                "artifacts": [{"artifactType": "linux.user", "title": "root",
                    "value": 0, "attributes": {"linux.uid": 0}}],
                "timeline": [],
                "findings": []
            })
        );
    }

    #[test]
    fn parses_structured_analysis_result() {
        let value = Value::Map(vec![
            (Value::from("summary"), Value::from("done")),
            (
                Value::from("nodes"),
                Value::Array(vec![Value::Map(vec![
                    (Value::from("type"), Value::from("linux.user")),
                    (Value::from("name"), Value::from("用户列表")),
                    (
                        Value::from("columns"),
                        Value::Array(vec![
                            Value::Map(vec![
                                (Value::from("key"), Value::from("linux.username")),
                                (Value::from("label"), Value::from("用户名")),
                            ]),
                            Value::Map(vec![
                                (Value::from("key"), Value::from("linux.uid")),
                                (Value::from("label"), Value::from("UID")),
                            ]),
                        ]),
                    ),
                ])]),
            ),
            (
                Value::from("findings"),
                Value::Array(vec![Value::Map(vec![
                    (Value::from("severity"), Value::from("high")),
                    (Value::from("title"), Value::from("Token")),
                    (Value::from("description"), Value::from("Found a token")),
                    (
                        Value::from("attributes"),
                        Value::Map(vec![(Value::from("auth.kind"), Value::from("bearer"))]),
                    ),
                ])]),
            ),
        ]);
        let result = parse_analysis_result(value).expect("result");
        assert_eq!(result.summary.as_deref(), Some("done"));
        assert_eq!(result.findings[0].severity, PluginFindingSeverity::High);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].display_name, "用户列表");
        assert_eq!(result.nodes[0].columns[0].key, "linux.username");
        assert_eq!(result.nodes[0].columns[1].label, "UID");
    }

    #[test]
    fn rejects_unnamespaced_result_nodes() {
        let value = Value::Map(vec![(
            Value::from("nodes"),
            Value::Array(vec![Value::Map(vec![
                (Value::from("type"), Value::from("plain")),
                (Value::from("name"), Value::from("无命名空间")),
            ])]),
        )]);
        assert!(parse_analysis_result(value).is_err());
    }

    #[test]
    fn rejects_binary_or_unnamespaced_values() {
        assert!(value_to_json(Value::Binary(vec![1])).is_err());
        assert!(attributes_to_json(HashMap::from([("plain".to_owned(), Value::Nil)])).is_err());
    }
}
