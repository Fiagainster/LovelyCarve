//! Bounded installation hints; source executable candidates are verified in the evidence volume.
use lovelycarve_forensic::list;
use lovelycarve_plugin::{Attributes, Error, EvidenceVolume, Result, Value};
use lovelycarve_windows_registry::{load, registry};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub enum Server {
    Apache,
    Nginx,
}

pub fn relative(raw: &str) -> Option<String> {
    let raw = raw.replace('\\', "/");
    let bytes = raw.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'/' {
        return None;
    }
    let parts: Vec<_> = raw.get(3..)?.split('/').collect();
    if parts.iter().any(|part| {
        part.is_empty() || matches!(*part, "." | "..") || part.contains([':', '$', '*', '?', '\0'])
    }) {
        return None;
    }
    Some(parts.join("/"))
}
pub fn canonical(volume: &EvidenceVolume, path: &str) -> Result<Option<String>> {
    let parts: Vec<_> = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if parts.len() > 64 {
        return Err(Error::new("RESOURCE_LIMIT", "安装候选路径超过 64 层"));
    }
    let mut resolved = String::new();
    for (index, component) in parts.iter().enumerate() {
        volume.context().check_cancelled()?;
        let matches: Vec<_> = list(volume, &resolved)?
            .into_iter()
            .filter(|node| !node.deleted && node.name.eq_ignore_ascii_case(component))
            .collect();
        if matches.is_empty() {
            return Ok(None);
        }
        if matches.len() != 1 {
            return Err(Error::new(
                "WEB_AMBIGUOUS",
                "安装候选路径大小写 / 同名目录项不唯一",
            ));
        }
        let entry = matches
            .into_iter()
            .next()
            .ok_or_else(|| Error::new("WEB_AMBIGUOUS", "候选路径目录项为空"))?;
        if (!entry.is_dir() && !entry.is_file()) || (index + 1 < parts.len() && !entry.is_dir()) {
            return Ok(None);
        }
        resolved = if resolved.is_empty() {
            entry.name
        } else {
            format!("{resolved}/{}", entry.name)
        };
    }
    Ok(Some(resolved))
}
pub fn find(
    volume: &EvidenceVolume,
    prefix: &str,
    server: Server,
) -> Result<BTreeMap<String, Value>> {
    let mut found = BTreeMap::new();
    let (executable_name, suffix, main_name, directory_names) = match server {
        Server::Apache => (
            "httpd.exe|",
            "/bin/httpd.exe",
            "httpd.conf",
            &["apache", "httpd"][..],
        ),
        Server::Nginx => ("nginx.exe|", "/nginx.exe", "nginx.conf", &["nginx"][..]),
    };
    let hive_path = format!("{prefix}Windows/appcompat/Programs/Amcache.hve");
    if let Some(loaded) = load::load(volume, &hive_path, 128 * 1024 * 1024)? {
        if let Some(root) = registry::key(&loaded.hive, "Root\\InventoryApplicationFile")? {
            let keys = registry::subkeys(&root)?;
            if keys.len() > 10000 {
                return Err(Error::new(
                    "RESOURCE_LIMIT",
                    "Web 发现 Amcache 清单超过 10000 项",
                ));
            }
            for key in keys {
                volume.context().check_cancelled()?;
                if !key.name().to_ascii_lowercase().starts_with(executable_name) {
                    continue;
                }
                for value in registry::values(&key)? {
                    if !value.name().eq_ignore_ascii_case("LowerCaseLongPath") {
                        continue;
                    }
                    let Some(raw) = registry::string(&value)? else {
                        continue;
                    };
                    let Some(path) = relative(&raw) else {
                        continue;
                    };
                    if !path.to_ascii_lowercase().ends_with(suffix) {
                        continue;
                    }
                    let Some(executable) = canonical(volume, &format!("{prefix}{path}"))? else {
                        continue;
                    };
                    if !volume
                        .stat(&executable)?
                        .is_some_and(|node| node.is_file() && !node.deleted)
                    {
                        continue;
                    }
                    let parent = match server {
                        Server::Apache => executable
                            .rsplit_once('/')
                            .and_then(|(bin, _)| bin.rsplit_once('/'))
                            .map(|(parent, _)| parent),
                        Server::Nginx => executable.rsplit_once('/').map(|(parent, _)| parent),
                    }
                    .ok_or_else(|| registry::error("服务候选路径无父目录"))?;
                    let root = parent.to_owned();
                    let main = format!("{root}/conf/{main_name}");
                    if volume
                        .stat(&main)?
                        .is_some_and(|node| node.is_file() && !node.deleted)
                    {
                        let mut sources = vec![Value::from(hive_path.as_str())];
                        sources.extend(
                            loaded
                                .log_sources
                                .iter()
                                .map(|source| Value::from(source.as_str())),
                        );
                        found.insert(
                            root,
                            Attributes::from_iter([
                                (
                                    "basis",
                                    Value::from(
                                        "Amcache 路径候选映射当前卷并核实文件；原盘符未核实",
                                    ),
                                ),
                                ("path_raw", Value::from(raw)),
                                ("hive", Value::from(hive_path.as_str())),
                                ("sources", Value::Array(sources)),
                                (
                                    "registry_key",
                                    Value::from(format!(
                                        "Root\\InventoryApplicationFile\\{}",
                                        key.name()
                                    )),
                                ),
                                ("offset", Value::from(4096 + u64::from(value.offset().0))),
                                (
                                    "raw_hex",
                                    Value::from(
                                        registry::binary(&value)?
                                            .iter()
                                            .map(|byte| format!("{byte:02x}"))
                                            .collect::<String>(),
                                    ),
                                ),
                            ])
                            .to_value(),
                        );
                    }
                }
            }
        }
    }
    // Direct portable installs and standard Program Files installs, without a whole-volume walk.
    let mut candidates = Vec::new();
    for directory in [
        prefix.trim_end_matches('/').to_owned(),
        format!("{prefix}Program Files"),
        format!("{prefix}Program Files (x86)"),
    ] {
        for node in list(volume, &directory)? {
            if node.is_dir()
                && !node.deleted
                && directory_names
                    .iter()
                    .any(|name| node.name.to_ascii_lowercase().starts_with(name))
            {
                candidates.push(if directory.is_empty() {
                    node.name
                } else {
                    format!("{directory}/{}", node.name)
                });
            }
        }
    }
    if candidates.len() > 256 {
        return Err(Error::new("RESOURCE_LIMIT", "Web 标准安装目录超过 256 项"));
    }
    for candidate in candidates {
        volume.context().check_cancelled()?;
        if found
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(&candidate))
        {
            continue;
        }
        if volume
            .stat(&format!("{candidate}/conf/{main_name}"))?
            .is_some_and(|node| node.is_file() && !node.deleted)
        {
            found.entry(candidate).or_insert_with(|| {
                Attributes::from_iter([(
                    "basis",
                    Value::from("根目录 / Program Files 中服务名称候选，核实主配置文件"),
                )])
                .to_value()
            });
        }
    }
    if found.len() > 256 {
        return Err(Error::new("RESOURCE_LIMIT", "Web 安装候选超过 256 项"));
    }
    Ok(found)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_paths_require_drive_absolute_paths_and_reject_namespace_escapes() {
        assert_eq!(
            relative("C:\\Web Dir\\Apache\\bin\\httpd.exe"),
            Some("Web Dir/Apache/bin/httpd.exe".into())
        );
        for raw in [
            "httpd.exe",
            "C:relative/httpd.exe",
            "\\\\server\\share\\httpd.exe",
            "C:/../bin/httpd.exe",
            "C:/x//bin/httpd.exe",
            "C:/x:stream/httpd.exe",
            "C:/${ROOT}/httpd.exe",
            "C:/x\0y/httpd.exe",
        ] {
            assert_eq!(relative(raw), None);
        }
    }
}
