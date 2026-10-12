//! Audit every declared source in saved plugin results using the UI's live resolver.
//! Inputs are local probe outputs; the evidence is only opened for reading.
use anyhow::Context;
use lovelycarve_lib::core::{
    filesystem::evidence_session::EvidenceSession, filesystem::fs_source_location::source_chain,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        arguments.len() >= 2,
        "usage: probe_plugin_sources <image> <result-json>..."
    );
    let image = Arc::new(EvidenceSession::open(&arguments[0])?);
    let mut sources: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut total = 0;
    for file in &arguments[1..] {
        let result: Value = serde_json::from_slice(&std::fs::read(file)?)
            .with_context(|| format!("invalid result JSON: {file}"))?;
        let mut records = 0;
        let mut missing = 0;
        for group in ["artifacts", "timeline", "findings"] {
            let records_in_group = result
                .get(group)
                .and_then(Value::as_array)
                .with_context(|| format!("missing result array {group}: {file}"))?;
            for (index, record) in records_in_group.iter().enumerate() {
                records += 1;
                let attributes = &record["attributes"];
                let root = attributes["source.root"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty());
                let path = attributes["source.path"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty());
                let owner = format!("{file}:{group}[{index}]");
                let (Some(root), Some(path)) = (root, path) else {
                    missing += 1;
                    failures.push(format!("missing source.root/source.path: {owner}"));
                    continue;
                };
                sources
                    .entry((root.into(), path.into()))
                    .or_default()
                    .insert(file.clone());
                if let Some(paths) = attributes.get("source.paths") {
                    let Some(paths) = paths.as_array() else {
                        failures.push(format!("source.paths is not an array: {owner}"));
                        continue;
                    };
                    for path in paths {
                        if let Some(path) = path.as_str().filter(|text| !text.trim().is_empty()) {
                            sources
                                .entry((root.into(), path.into()))
                                .or_default()
                                .insert(file.clone());
                        } else {
                            failures.push(format!("invalid source.paths member: {owner}"));
                        }
                    }
                }
            }
        }
        total += records;
        println!("{file}: records={records} missing={missing}");
    }
    let mut resolved = 0;
    for ((root, path), owners) in &sources {
        match source_chain(&image, root, path) {
            Ok(chain) if !chain.is_empty() => resolved += 1,
            Ok(_) => failures.push(format!("empty navigation chain: {root} {path} {owners:?}")),
            Err(error) => failures.push(format!("{root} {path}: {error:#}; {owners:?}")),
        }
        if (resolved + failures.len()) % 100 == 0 {
            println!("resolved={resolved}/{}", sources.len());
        }
    }
    println!(
        "records={total} unique_sources={} resolved={resolved} failures={}",
        sources.len(),
        failures.len()
    );
    for failure in &failures {
        eprintln!("{failure}");
    }
    anyhow::ensure!(failures.is_empty(), "plugin provenance audit failed");
    Ok(())
}
