//! Native, bounded name scans resume by node ID rather than resolving every directory path.
use super::{PluginError, Result};
use crate::core::{
    filesystem::evidence_session::EvidenceSession,
    filesystem::tree::fs_tree::{FsSession, NodeKind},
};
use rmpv::Value;
use std::{collections::HashSet, sync::Arc};
use tokio_util::sync::CancellationToken;

pub(super) struct Filters {
    names: Vec<String>,
    prefixes: Vec<String>,
    suffixes: Vec<String>,
}
impl Filters {
    pub fn from_value(params: &Value) -> Result<Self> {
        let invalid = || {
            PluginError::Rpc { code: "INVALID_PARAMS".into(), message: "name filters must contain 1..=32 nonempty names/prefixes/suffixes, each <=128 bytes without path separators".into() }
        };
        let mut groups = Vec::new();
        let mut count = 0;
        for name in ["names", "prefixes", "suffixes"] {
            let values = lovelycarve_plugin::value::get(params, name)
                .and_then(Value::as_array)
                .ok_or_else(invalid)?;
            let mut group = Vec::new();
            for value in values {
                let name = value
                    .as_str()
                    .filter(|name| {
                        !name.is_empty() && name.len() <= 128 && !name.contains(['/', '\\', '\0'])
                    })
                    .ok_or_else(invalid)?;
                group.push(name.to_ascii_lowercase());
                count += 1;
                if count > 32 {
                    return Err(invalid());
                }
            }
            groups.push(group);
        }
        if count == 0 {
            return Err(invalid());
        }
        let mut groups = groups.into_iter();
        Ok(Self {
            names: groups.next().ok_or_else(invalid)?,
            prefixes: groups.next().ok_or_else(invalid)?,
            suffixes: groups.next().ok_or_else(invalid)?,
        })
    }
    fn matches(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.names.contains(&name)
            || self.prefixes.iter().any(|prefix| name.starts_with(prefix))
            || self.suffixes.iter().any(|suffix| name.ends_with(suffix))
    }
}
pub(super) struct NameScan {
    pub root: String,
    filters: Filters,
    stack: Vec<(String, String, u32)>,
    directories: HashSet<String>,
    visited: usize,
    matched: usize,
    result_bytes: usize,
}
pub(super) struct Page {
    pub paths: Vec<String>,
    pub visited: usize,
    pub complete: bool,
}
impl NameScan {
    pub fn new(root: String, filters: Filters) -> Self {
        Self {
            stack: vec![(root.clone(), String::new(), 0)],
            root,
            filters,
            directories: HashSet::new(),
            visited: 0,
            matched: 0,
            result_bytes: 0,
        }
    }
    pub fn page(
        &mut self,
        image: &Arc<EvidenceSession>,
        cancellation: &CancellationToken,
    ) -> Result<Page> {
        let mut session = FsSession::new(image);
        let mut paths = Vec::new();
        // Each host call is a bounded batch; plugin cancellation is checked between calls.
        for _ in 0..64 {
            if cancellation.is_cancelled() {
                return Err(PluginError::Rpc {
                    code: "CANCELLED".into(),
                    message: "name scan cancelled".into(),
                });
            }
            let Some((directory, parent, depth)) = self.stack.pop() else {
                break;
            };
            let children = session
                .children(&directory)
                .map_err(|error| PluginError::Rpc {
                    code: "EVIDENCE_SCAN_FAILED".into(),
                    message: format!("{parent}: {error}"),
                })?;
            for node in children {
                if cancellation.is_cancelled() {
                    return Err(PluginError::Rpc {
                        code: "CANCELLED".into(),
                        message: "name scan cancelled".into(),
                    });
                }
                self.visited += 1;
                if self.visited > 4_000_000 {
                    return Err(PluginError::ResourceLimit(
                        "name scan exceeds 4000000 nodes; no complete result".into(),
                    ));
                }
                if node.deleted || node.link_target.is_some() {
                    continue;
                }
                let path = if parent.is_empty() {
                    node.name.clone()
                } else {
                    format!("{parent}/{}", node.name)
                };
                if path.len() > 4096 {
                    return Err(PluginError::ResourceLimit(
                        "name scan path exceeds 4096 bytes".into(),
                    ));
                }
                match node.kind {
                    NodeKind::Dir => {
                        if !self.directories.insert(node.id.clone()) {
                            continue;
                        }
                        if depth >= 128 || self.directories.len() > 262144 {
                            return Err(PluginError::ResourceLimit(
                                "name scan exceeds directory depth/count budget".into(),
                            ));
                        }
                        self.stack.push((node.id, path, depth + 1));
                    }
                    NodeKind::File if self.filters.matches(&node.name) => {
                        self.matched += 1;
                        self.result_bytes += path.len();
                        if self.matched > 4096 || self.result_bytes > 2 * 1024 * 1024 {
                            return Err(PluginError::ResourceLimit(
                                "name scan exceeds 4096 matches / 2 MiB paths".into(),
                            ));
                        }
                        paths.push(path);
                    }
                    NodeKind::File | NodeKind::Symlink | NodeKind::Unallocated => {}
                }
            }
        }
        Ok(Page {
            paths,
            visited: self.visited,
            complete: self.stack.is_empty(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lovelycarve_plugin::value;
    fn params(names: Vec<Value>) -> Value {
        value::map([
            ("names", Value::Array(names)),
            ("prefixes", Value::Array(vec![])),
            ("suffixes", Value::Array(vec![".hiv".into()])),
        ])
    }
    #[test]
    fn filters_are_literal_bounded_and_case_insensitive() {
        let filters = Filters::from_value(&params(vec!["SYSTEM".into()])).unwrap();
        assert!(filters.matches("system"));
        assert!(filters.matches("sample.HIV"));
        assert!(!filters.matches("SYSTEM32"));
        assert!(Filters::from_value(&params(vec!["../SYSTEM".into()])).is_err());
        assert!(Filters::from_value(&params(vec![Value::from(1)])).is_err());
        assert!(Filters::from_value(&params(vec!["SYSTEM".into(); 33])).is_err());
    }
}
