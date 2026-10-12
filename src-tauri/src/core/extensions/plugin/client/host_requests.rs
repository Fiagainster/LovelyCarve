//! Reverse host RPC dispatch, permission checks and wire projection.
use super::*;

/// Serves one plugin → host LPP call. Shared by the stdio router and the
/// in-process host table.
#[cfg(test)]
pub(in crate::core::extensions::plugin) fn dispatch_host_request(
    method: &str,
    params: Value,
    services: &HostRpcServices,
    plugin_id: &str,
    progress_sink: &Option<PluginProgressSink>,
) -> Result<Value> {
    dispatch_host_request_with_cancellation(
        method,
        params,
        services,
        plugin_id,
        progress_sink,
        &CancellationToken::new(),
    )
}

pub(in crate::core::extensions::plugin) fn dispatch_host_request_with_cancellation(
    method: &str,
    params: Value,
    services: &HostRpcServices,
    plugin_id: &str,
    progress_sink: &Option<PluginProgressSink>,
    cancellation: &CancellationToken,
) -> Result<Value> {
    match method {
        "source.info" => {
            let source_id = string_param(&params, "source_id")?;
            let info = services.sources.info(source_id)?;
            Ok(Value::Map(vec![
                (Value::from("source_id"), Value::from(info.source_id)),
                (Value::from("size"), Value::from(info.size)),
                (Value::from("sector_size"), Value::from(info.sector_size)),
                (Value::from("read_only"), Value::from(info.read_only)),
            ]))
        }
        "source.read" => {
            let source_id = string_param(&params, "source_id")?;
            let offset = u64_param(&params, "offset")?;
            let length = u64_param(&params, "length")?;
            let length = usize::try_from(length).map_err(|_| PluginError::Rpc {
                code: "INVALID_PARAMS".to_owned(),
                message: "length does not fit this host".to_owned(),
            })?;
            Ok(Value::Binary(
                services.sources.read(source_id, offset, length)?,
            ))
        }
        "source.readv" => {
            let source_id = string_param(&params, "source_id")?;
            let ranges = ranges_param(&params)?;
            Ok(Value::Array(
                services
                    .sources
                    .readv(source_id, &ranges)?
                    .into_iter()
                    .map(Value::Binary)
                    .collect(),
            ))
        }
        "file.info" => {
            let file_id = string_param(&params, "file_id")?;
            Ok(file_handle_value(services.files.info(plugin_id, file_id)?))
        }
        "file.read" => {
            let file_id = string_param(&params, "file_id")?;
            let offset = u64_param(&params, "offset")?;
            let length = usize_param(&params, "length")?;
            Ok(Value::Binary(
                services.files.read(plugin_id, file_id, offset, length)?,
            ))
        }
        "file.readv" => {
            let file_id = string_param(&params, "file_id")?;
            let ranges = ranges_param(&params)?;
            Ok(Value::Array(
                services
                    .files
                    .readv(plugin_id, file_id, &ranges)?
                    .into_iter()
                    .map(Value::Binary)
                    .collect(),
            ))
        }
        "file.materialize" => {
            require_permission(services, PluginPermission::FileMaterialize)?;
            let file_id = string_param(&params, "file_id")?;
            Ok(materialized_lease_value(
                services.files.materialize(plugin_id, file_id)?,
            ))
        }
        "file.release" => {
            let lease_id = string_param(&params, "lease_id")?;
            services.files.release(plugin_id, lease_id)?;
            Ok(Value::Nil)
        }
        "evidence.roots" => {
            let evidence_id = string_param(&params, "evidence_id")?;
            Ok(Value::Array(
                services
                    .evidences
                    .roots(plugin_id, evidence_id)?
                    .into_iter()
                    .map(evidence_root_value)
                    .collect(),
            ))
        }
        "evidence.stat" => {
            let evidence_id = string_param(&params, "evidence_id")?;
            let root_id = string_param(&params, "root")?;
            let path = string_param(&params, "path")?;
            Ok(services
                .evidences
                .stat(plugin_id, evidence_id, root_id, path)?
                .map(evidence_node_value)
                .unwrap_or(Value::Nil))
        }
        "evidence.list" => {
            let evidence_id = string_param(&params, "evidence_id")?;
            let root_id = string_param(&params, "root")?;
            let path = string_param(&params, "path")?;
            Ok(Value::Array(
                services
                    .evidences
                    .list(plugin_id, evidence_id, root_id, path)?
                    .into_iter()
                    .map(evidence_node_value)
                    .collect(),
            ))
        }
        "evidence.list_page" => {
            let page = services.evidences.list_page(
                plugin_id,
                string_param(&params, "evidence_id")?,
                string_param(&params, "root")?,
                string_param(&params, "path")?,
                usize_param(&params, "skip")?,
                usize_param(&params, "limit")?,
            )?;
            Ok(Value::Map(vec![
                (Value::from("total"), Value::from(page.total as u64)),
                (
                    Value::from("nodes"),
                    Value::Array(page.nodes.into_iter().map(evidence_node_value).collect()),
                ),
            ]))
        }
        "evidence.find_names_page" => {
            let filters =
                crate::core::extensions::plugin::evidence_name_scan::Filters::from_value(&params)?;
            let cursor = optional_string_param(&params, "cursor")?;
            let (page, next) = services.evidences.name_scan_page(
                plugin_id,
                string_param(&params, "evidence_id")?,
                string_param(&params, "root")?,
                cursor.as_deref(),
                filters,
                cancellation,
            )?;
            if let Some(sink) = progress_sink {
                sink(PluginProgressEvent {
                    plugin_id: plugin_id.to_owned(),
                    current: page.visited as u64,
                    total: None,
                    message: Some("扫描文件名候选".into()),
                });
            }
            Ok(lovelycarve_plugin::value::map([
                (
                    "paths",
                    Value::Array(page.paths.into_iter().map(Value::from).collect()),
                ),
                ("visited", Value::from(page.visited as u64)),
                ("cursor", next.map(Value::from).unwrap_or(Value::Nil)),
            ]))
        }
        "evidence.read" => {
            let evidence_id = string_param(&params, "evidence_id")?;
            let root_id = string_param(&params, "root")?;
            let path = string_param(&params, "path")?;
            let offset = u64_param(&params, "offset")?;
            let length = usize_param(&params, "length")?;
            if length > MAX_EVIDENCE_READ {
                return Err(PluginError::ResourceLimit(format!(
                    "evidence.read length {length} exceeds {MAX_EVIDENCE_READ}"
                )));
            }
            Ok(Value::Binary(services.evidences.read(
                plugin_id,
                evidence_id,
                root_id,
                path,
                offset,
                length,
            )?))
        }
        "evidence.readv" => {
            let evidence_id = string_param(&params, "evidence_id")?;
            let root_id = string_param(&params, "root")?;
            let path = string_param(&params, "path")?;
            let ranges = ranges_param(&params)?;
            if ranges.len() > MAX_EVIDENCE_READV_RANGES {
                return Err(PluginError::ResourceLimit(format!(
                    "evidence.readv has more than {MAX_EVIDENCE_READV_RANGES} ranges"
                )));
            }
            Ok(Value::Array(
                services
                    .evidences
                    .readv(plugin_id, evidence_id, root_id, path, &ranges)?
                    .into_iter()
                    .map(Value::Binary)
                    .collect(),
            ))
        }
        "host.progress" => {
            let current = u64_param(&params, "current")?;
            let total = optional_u64_param(&params, "total")?;
            let message = optional_string_param(&params, "message")?;
            if let Some(progress_sink) = progress_sink {
                progress_sink(PluginProgressEvent {
                    plugin_id: plugin_id.to_owned(),
                    current,
                    total,
                    message,
                });
            }
            Ok(Value::Nil)
        }
        "host.log" => {
            tracing::debug!(params = ?params, "plugin structured log");
            Ok(Value::Nil)
        }
        _ => Err(PluginError::Rpc {
            code: "METHOD_NOT_FOUND".to_owned(),
            message: format!("host method {method} is not available"),
        }),
    }
}

pub(super) fn map_param<'a>(params: &'a Value, name: &str) -> Result<&'a Value> {
    let Value::Map(entries) = params else {
        return Err(invalid_params("params must be a map"));
    };
    entries
        .iter()
        .find_map(|(key, value)| (key.as_str() == Some(name)).then_some(value))
        .ok_or_else(|| invalid_params(&format!("missing {name}")))
}

pub(super) fn string_param<'a>(params: &'a Value, name: &str) -> Result<&'a str> {
    map_param(params, name)?
        .as_str()
        .ok_or_else(|| invalid_params(&format!("{name} must be a string")))
}

pub(super) fn u64_param(params: &Value, name: &str) -> Result<u64> {
    map_param(params, name)?
        .as_u64()
        .ok_or_else(|| invalid_params(&format!("{name} must be an unsigned integer")))
}

pub(super) fn usize_param(params: &Value, name: &str) -> Result<usize> {
    usize::try_from(u64_param(params, name)?)
        .map_err(|_| invalid_params(&format!("{name} does not fit this host")))
}

pub(super) fn ranges_param(params: &Value) -> Result<Vec<(u64, usize)>> {
    let Value::Array(values) = map_param(params, "ranges")? else {
        return Err(invalid_params("ranges must be an array"));
    };
    values
        .iter()
        .map(|value| {
            let Value::Array(range) = value else {
                return Err(invalid_params("each range must be [offset, length]"));
            };
            if range.len() != 2 {
                return Err(invalid_params("each range must contain two integers"));
            }
            let offset = range[0]
                .as_u64()
                .ok_or_else(|| invalid_params("range offset must be unsigned"))?;
            let length = range[1]
                .as_u64()
                .ok_or_else(|| invalid_params("range length must be unsigned"))?;
            let length = usize::try_from(length)
                .map_err(|_| invalid_params("range length does not fit this host"))?;
            Ok((offset, length))
        })
        .collect()
}

pub(super) fn require_permission(
    services: &HostRpcServices,
    permission: PluginPermission,
) -> Result<()> {
    if services.permissions.contains(&permission) {
        Ok(())
    } else {
        Err(PluginError::Rpc {
            code: "PERMISSION_DENIED".to_owned(),
            message: format!("plugin permission {permission:?} was not granted"),
        })
    }
}

pub(super) fn file_handle_value(handle: FileHandle) -> Value {
    Value::Map(vec![
        (Value::from("file_id"), Value::from(handle.file_id)),
        (Value::from("name"), Value::from(handle.name)),
        (Value::from("size"), Value::from(handle.size)),
        (Value::from("mime"), Value::from(handle.mime)),
        (Value::from("read_only"), Value::from(handle.read_only)),
        (Value::from("max_read"), Value::from(MAX_FILE_READ)),
        (
            Value::from("max_readv_ranges"),
            Value::from(MAX_FILE_READV_RANGES),
        ),
    ])
}

pub(super) fn materialized_lease_value(lease: MaterializedFileLease) -> Value {
    Value::Map(vec![
        (Value::from("lease_id"), Value::from(lease.lease_id)),
        (Value::from("path"), Value::from(lease.path)),
        (Value::from("size"), Value::from(lease.size)),
    ])
}

pub(super) fn evidence_root_value(root: EvidenceRoot) -> Value {
    Value::Map(vec![
        (Value::from("root_id"), Value::from(root.root_id)),
        (Value::from("fs_prefix"), Value::from(root.fs_prefix)),
        (Value::from("kind"), Value::from(root.kind)),
        (Value::from("size"), Value::from(root.size)),
    ])
}

pub(super) fn evidence_node_value(node: EvidenceNode) -> Value {
    Value::Map(vec![
        (Value::from("node_id"), Value::from(node.node_id)),
        (Value::from("name"), Value::from(node.name)),
        (Value::from("kind"), Value::from(node.kind)),
        (Value::from("size"), Value::from(node.size)),
        (Value::from("deleted"), Value::from(node.deleted)),
        (
            Value::from("link_target"),
            node.link_target.map(Value::from).unwrap_or(Value::Nil),
        ),
        (Value::from("timestamps"), timestamps_value(node.timestamps)),
        (Value::from("max_read"), Value::from(MAX_EVIDENCE_READ)),
    ])
}

pub(super) fn timestamps_value(timestamps: crate::core::filesystem::tree::fs_tree::Mactb) -> Value {
    Value::Map(vec![
        (
            Value::from("modified"),
            timestamps.modified.map(Value::from).unwrap_or(Value::Nil),
        ),
        (
            Value::from("accessed"),
            timestamps.accessed.map(Value::from).unwrap_or(Value::Nil),
        ),
        (
            Value::from("created"),
            timestamps.created.map(Value::from).unwrap_or(Value::Nil),
        ),
        (
            Value::from("mft_changed"),
            timestamps
                .mft_changed
                .map(Value::from)
                .unwrap_or(Value::Nil),
        ),
        (
            Value::from("birth"),
            timestamps.birth.map(Value::from).unwrap_or(Value::Nil),
        ),
    ])
}

pub(super) fn optional_u64_param(params: &Value, name: &str) -> Result<Option<u64>> {
    match map_param(params, name)? {
        Value::Nil => Ok(None),
        value => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid_params(&format!("{name} must be an unsigned integer or nil"))),
    }
}

pub(super) fn optional_string_param(params: &Value, name: &str) -> Result<Option<String>> {
    match map_param(params, name)? {
        Value::Nil => Ok(None),
        value => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| invalid_params(&format!("{name} must be a string or nil"))),
    }
}

pub(super) fn invalid_params(message: &str) -> PluginError {
    PluginError::Rpc {
        code: "INVALID_PARAMS".to_owned(),
        message: message.to_owned(),
    }
}

pub(in crate::core::extensions::plugin) fn plugin_error_to_rpc(error: PluginError) -> RpcError {
    match error {
        PluginError::Rpc { code, message } => RpcError::new(code, message),
        PluginError::ResourceLimit(message) => RpcError::new("RESOURCE_LIMIT", message),
        error => RpcError::new("HOST_ERROR", error.to_string()),
    }
}
