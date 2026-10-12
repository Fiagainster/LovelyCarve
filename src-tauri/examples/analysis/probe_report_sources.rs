//! Inventory report source candidates through the plugin's read-only path API.
use anyhow::Context;
use lovelycarve_lib::core::{
    extensions::plugin::EvidenceRegistry, filesystem::evidence_session::EvidenceSession,
};
use serde_json::json;
use std::{collections::HashSet, sync::Arc};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        arguments.len() == 3,
        "usage: probe_report_sources <image> <root> <output-json>"
    );
    let image = Arc::new(EvidenceSession::open(&arguments[0])?);
    let registry = EvidenceRegistry::default();
    let handle = registry.register("probe", image.id.clone(), image);
    let mut stack = vec![(String::new(), 0)];
    let mut visited = HashSet::new();
    let mut candidates = Vec::new();
    while let Some((path, depth)) = stack.pop() {
        anyhow::ensure!(depth <= 128, "directory depth exceeded");
        let entries = registry
            .list("probe", &handle.evidence_id, &arguments[1], &path)
            .with_context(|| format!("list {path}"))?;
        for entry in entries {
            if entry.deleted {
                continue;
            }
            let path = if path.is_empty() {
                entry.name.clone()
            } else {
                format!("{path}/{}", entry.name)
            };
            if entry.kind == "dir" && visited.insert(entry.node_id.clone()) {
                anyhow::ensure!(visited.len() <= 100_000, "directory count exceeded");
                stack.push((path, depth + 1));
                continue;
            }
            let name = entry.name.to_lowercase();
            if name.ends_with(".desktop")
                || name.ends_with(".eml")
                || [
                    ".doc", ".xls", ".ppt", ".docx", ".xlsx", ".pptx", ".odt", ".ods", ".odp",
                ]
                .iter()
                .any(|suffix| name.ends_with(suffix))
                || name.contains("sys_config")
                || name == "nginx.conf"
                || name == "apache2.conf"
                || name == "config.v2.json"
                || name == "repositories.json"
                || name == "local-kv.db"
            {
                candidates.push(json!({"path":path,"node":entry}));
            }
        }
    }
    std::fs::write(&arguments[2], serde_json::to_vec_pretty(&candidates)?)?;
    println!(
        "{} source candidates in {} directories",
        candidates.len(),
        visited.len()
    );
    Ok(())
}
