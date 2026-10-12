//! Regular configuration/log inventories with deterministic collision and cycle handling.
use lovelycarve_forensic::list;
use lovelycarve_plugin::{Error, EvidenceNode, EvidenceVolume, Result};
use std::collections::{BTreeMap, BTreeSet};
pub fn collect(
    volume: &EvidenceVolume,
    start: &str,
    configs: bool,
) -> Result<Vec<(String, EvidenceNode)>> {
    let mut files = Vec::new();
    let mut stack = vec![(start.to_owned(), 0_usize)];
    let mut seen = BTreeSet::new();
    let mut visited = 0_usize;
    while let Some((directory, depth)) = stack.pop() {
        volume.context().check_cancelled()?;
        let mut groups: BTreeMap<String, Vec<EvidenceNode>> = BTreeMap::new();
        for entry in list(volume, &directory)? {
            visited += 1;
            if visited > 20000 {
                return Err(Error::new(
                    "RESOURCE_LIMIT",
                    "Web 配置 / 日志目录超过 20000 项",
                ));
            }
            if !entry.deleted && (entry.is_dir() || entry.is_file()) {
                groups
                    .entry(entry.name.to_ascii_lowercase())
                    .or_default()
                    .push(entry);
            }
        }
        for group in groups.into_values() {
            if group.len() != 1 {
                return Err(Error::new(
                    "WEB_AMBIGUOUS",
                    "Web 文件 / 目录同名冲突，未猜测选择",
                ));
            }
            let entry = group
                .into_iter()
                .next()
                .ok_or_else(|| Error::new("WEB_AMBIGUOUS", "文件组为空"))?;
            let path = format!("{directory}/{}", entry.name);
            if entry.is_dir() {
                if !seen.insert(entry.node_id.clone()) {
                    return Err(Error::new("WEB_AMBIGUOUS", "Web 目录节点重复或环"));
                }
                if volume
                    .stat(&path)?
                    .is_none_or(|node| node.node_id != entry.node_id)
                {
                    return Err(Error::new("WEB_AMBIGUOUS", "路径定位与 Web 目录节点不符"));
                }
                if depth >= 16 {
                    return Err(Error::new(
                        "RESOURCE_LIMIT",
                        "Web 配置 / 日志目录超过 16 层",
                    ));
                }
                stack.push((path, depth + 1));
            } else if !configs || entry.name.to_ascii_lowercase().ends_with(".conf") {
                if volume
                    .stat(&path)?
                    .is_none_or(|node| node.node_id != entry.node_id)
                {
                    return Err(Error::new("WEB_AMBIGUOUS", "路径定位与配置 / 日志节点不符"));
                }
                files.push((path, entry));
                if files.len() > 4096 {
                    return Err(Error::new(
                        "RESOURCE_LIMIT",
                        "Web 配置 / 日志文件超过 4096 项",
                    ));
                }
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}
