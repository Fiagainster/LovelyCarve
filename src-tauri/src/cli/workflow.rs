use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Write},
    path::Path,
    time::Duration,
};

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::Manager;

use super::{input, shell::Shell, CliError, Output};
use crate::state::AppState;

const MAX_LINE: usize = 16 * 1024 * 1024;

pub(super) async fn wait_idle(app: &tauri::AppHandle) -> Result<(), CliError> {
    let state = app.state::<AppState>();
    loop {
        let scheduler_busy = if state.active_case.lock().await.is_some() {
            state
                .scheduler
                .list_for_image(None, true)
                .map_err(super::internal_error)?
                .iter()
                .any(|task| task.status.is_active())
        } else {
            false
        };
        if state.tasks.list().is_empty() && !scheduler_busy {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub(super) fn save_output(path: &Path, output: Output) -> Result<(), CliError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    match output {
        Output::Json(value) => {
            serde_json::to_writer_pretty(&mut file, &value)?;
            writeln!(file)?;
        }
        Output::Binary(bytes) => file.write_all(&bytes)?,
    }
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|error| CliError::from(error.error))?;
    Ok(())
}

pub(super) fn print_output(output: Output, json_mode: bool) -> Result<(), CliError> {
    let mut stdout = std::io::stdout().lock();
    let value = output.json();
    if json_mode {
        serde_json::to_writer(&mut stdout, &value)?;
        writeln!(stdout)?;
    } else if let Some(strings) = value.get("strings").and_then(Value::as_array) {
        writeln!(stdout, "字节偏移\t字符串")?;
        for string in strings {
            writeln!(
                stdout,
                "{}\t{}{}",
                string["offset"],
                safe_text(string["text"].as_str().unwrap_or("")),
                if string["truncated"] == true {
                    " [已截断]"
                } else {
                    ""
                }
            )?;
        }
        if value["truncated"] == true {
            writeln!(stdout, "字符串结果受到扫描范围、数量或输出大小限制")?;
        }
    } else if value["encoding"] == "hex" && value.get("bytes_read").is_some() {
        let bytes =
            hex::decode(value["data"].as_str().unwrap_or("")).map_err(super::internal_error)?;
        let offset = value["offset"].as_u64().unwrap_or(0);
        for (row, chunk) in bytes.chunks(16).enumerate() {
            write!(stdout, "{:016x}  ", offset + (row * 16) as u64)?;
            for column in 0..16 {
                match chunk.get(column) {
                    Some(byte) => write!(stdout, "{byte:02x} ")?,
                    None => write!(stdout, "   ")?,
                }
                if column == 7 {
                    write!(stdout, " ")?;
                }
            }
            let text = chunk
                .iter()
                .map(|byte| {
                    if byte.is_ascii_graphic() || *byte == b' ' {
                        char::from(*byte)
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            writeln!(stdout, " |{text}|")?;
        }
        if value["truncated"] == true {
            writeln!(stdout, "预览已截断；使用 --offset / --length 查看后续内容")?;
        }
    } else if value.get("mime_type").is_some() && value.get("inferred").is_some() {
        writeln!(
            stdout,
            "{} ({})；类型：{}；置信度：{:.1}%",
            safe_text(value["description"].as_str().unwrap_or("")),
            safe_text(value["mime_type"].as_str().unwrap_or("")),
            safe_text(value["label"].as_str().unwrap_or("")),
            value["score"].as_f64().unwrap_or(0.0) * 100.0
        )?;
    } else if let Some(items) = value.get("items").and_then(Value::as_array) {
        writeln!(stdout, "类型\t大小\t删除\t路径\tNODE ID")?;
        for item in items {
            let node = &item["node"];
            writeln!(
                stdout,
                "{}\t{}\t{}\t{}\t{}",
                node["kind"].as_str().unwrap_or("?"),
                node["size"],
                node["deleted"],
                safe_text(item["path"].as_str().unwrap_or("")),
                safe_text(node["id"].as_str().unwrap_or(""))
            )?;
        }
        if value["truncated"] == true {
            writeln!(stdout, "结果受到 --limit / --depth 限制")?;
        }
    } else if let Some(hits) = value.get("hits").and_then(Value::as_array) {
        writeln!(stdout, "路径\t字节偏移\t编码\t上下文\tNODE ID")?;
        for hit in hits {
            writeln!(
                stdout,
                "{}\t{}\t{}\t{}\t{}",
                safe_text(hit["file_path"].as_str().unwrap_or("")),
                hit["file_offset"],
                safe_text(hit["encoding"].as_str().unwrap_or("")),
                safe_text(hit["context_preview"].as_str().unwrap_or("")),
                safe_text(hit["node_id"].as_str().unwrap_or("")),
            )?;
        }
        writeln!(
            stdout,
            "搜索 ID：{}；状态：{}；显示 {} / {} 条命中；读取错误：{}",
            safe_text(value["summary"]["search_id"].as_str().unwrap_or("")),
            safe_text(value["summary"]["status"].as_str().unwrap_or("")),
            hits.len(),
            value["total"],
            value["summary"]["read_errors"],
        )?;
        if value["truncated"] == true {
            writeln!(
                stdout,
                "结果不完整或显示受到 --limit 限制；可用 global-search-hits-page 查询已保存命中"
            )?;
        }
    } else if let Some(text) = value.as_str() {
        writeln!(stdout, "{}", safe_text(text))?;
    } else {
        serde_json::to_writer_pretty(&mut stdout, &value)?;
        writeln!(stdout)?;
    }
    stdout.flush()?;
    Ok(())
}

fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

fn line(reader: &mut impl BufRead) -> Result<Option<String>, CliError> {
    let mut text = String::new();
    let bytes = std::io::Read::take(&mut *reader, (MAX_LINE + 1) as u64).read_line(&mut text)?;
    if bytes > MAX_LINE {
        return Err(CliError::invalid("单行请求超过 16 MiB"));
    }
    Ok((bytes != 0).then_some(text))
}

pub(super) async fn shell_lines(
    app: &tauri::AppHandle,
    shell: &mut Shell,
    reader: &mut impl BufRead,
    interactive: bool,
    json_mode: bool,
) -> Result<(), CliError> {
    loop {
        if interactive {
            eprint!("lovelycarve:{}> ", safe_text(&shell.pwd()));
            std::io::stderr().flush()?;
        }
        let Some(line) = line(reader)? else {
            break;
        };
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }
        let result = async {
            let words = input::tokenize(line)?;
            let parser = input::parser()?;
            let matches = match parser
                .try_get_matches_from(std::iter::once("lovelycarve-cli".to_owned()).chain(words))
            {
                Ok(matches) => matches,
                Err(error)
                    if matches!(
                        error.kind(),
                        clap::error::ErrorKind::DisplayHelp
                            | clap::error::ErrorKind::DisplayVersion
                    ) =>
                {
                    error.print()?;
                    return Ok(());
                }
                Err(error) => return Err(CliError::invalid(error.to_string())),
            };
            if matches!(matches.subcommand_name(), Some("shell" | "session" | "run")) {
                return Err(CliError::invalid("不能嵌套终端或脚本"));
            }
            if matches.get_one::<String>("image").is_some()
                || matches.get_one::<String>("case").is_some()
                || matches.get_flag("events")
            {
                return Err(CliError::invalid(
                    "--image / --case / --events 仅用于启动参数；会话内请用 open / case-open",
                ));
            }
            if let Some(output) = shell.execute(app, &matches).await? {
                print_output(output, json_mode || matches.get_flag("json"))?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            if !interactive {
                return Err(error);
            }
            eprintln!("{error}");
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    command: String,
    #[serde(default)]
    args: Map<String, Value>,
    #[serde(default)]
    output: Option<String>,
}

/// References use {"$ref":"request-id#/json/pointer"}. Literal strings are
/// never expanded, so passwords, Windows paths and evidence text stay exact.
fn resolve_refs(value: &mut Value, results: &HashMap<String, Value>) -> Result<(), CliError> {
    match value {
        Value::Object(object) if object.len() == 1 && object.contains_key("$ref") => {
            let reference = object["$ref"]
                .as_str()
                .ok_or_else(|| CliError::invalid("$ref 必须是字符串"))?;
            let (id, pointer) = reference.split_once('#').unwrap_or((reference, ""));
            *value = results
                .get(id)
                .and_then(|result| result.pointer(pointer))
                .cloned()
                .ok_or_else(|| CliError::invalid(format!("无效结果引用: {reference}")))?;
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                resolve_refs(value, results)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                resolve_refs(value, results)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

pub(super) async fn json_lines(
    app: &tauri::AppHandle,
    shell: &mut Shell,
    reader: &mut impl BufRead,
) -> Result<bool, CliError> {
    let mut results = HashMap::new();
    let mut request_ids = HashSet::new();
    let mut failed = false;
    while let Some(line) = line(reader)? {
        if line.trim().is_empty() {
            continue;
        }
        let request: Request = match serde_json::from_str(line.trim_start_matches('\u{feff}')) {
            Ok(request) => request,
            Err(error) => {
                print_output(
                    Output::Json(json!({"id":null,"error":CliError::from(error)})),
                    true,
                )?;
                failed = true;
                continue;
            }
        };
        let result = async {
            if request.id.is_empty() || !request_ids.insert(request.id.clone()) {
                return Err(CliError::invalid("请求 ID 必须非空且唯一"));
            }
            if request_ids.len() > 10000 {
                return Err(CliError::invalid("单会话最多保留 10000 个结果引用"));
            }
            let mut args = Value::Object(request.args);
            resolve_refs(&mut args, &results)?;
            let Value::Object(args) = args else {
                return Err(CliError::invalid("args 必须为对象"));
            };
            let output = if request.command == "wait" {
                if !args.is_empty() {
                    return Err(CliError::invalid("wait 不接受参数"));
                }
                wait_idle(app).await?;
                Output::Json(Value::Null)
            } else if let Some(matches) = input::shell_matches(&request.command, &args)? {
                shell
                    .execute(app, &matches)
                    .await?
                    .unwrap_or(Output::Json(Value::Null))
            } else {
                shell.invoke(app, &request.command, args).await?
            };
            if let Some(path) = request.output {
                save_output(Path::new(&path), output)?;
                Ok(json!({"output":path}))
            } else {
                Ok(output.json())
            }
        }
        .await;
        let response = match result {
            Ok(value) => {
                results.insert(request.id.clone(), value.clone());
                json!({"id":request.id,"result":value})
            }
            Err(error) => {
                failed = true;
                json!({"id":request.id,"error":error})
            }
        };
        print_output(Output::Json(response), true)?;
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_are_structural_and_errors_are_not_silenced() {
        let results = HashMap::from([("open".into(), json!({"id":"image-1"}))]);
        let mut args = json!({"imageId":{"$ref":"open#/id"},"text":"$open/id"});
        resolve_refs(&mut args, &results).unwrap();
        assert_eq!(args, json!({"imageId":"image-1","text":"$open/id"}));
        assert!(resolve_refs(&mut json!({"$ref":"missing#/id"}), &results).is_err());
    }
    #[test]
    fn output_never_overwrites_existing_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.bin");
        std::fs::write(&path, b"original").unwrap();
        assert!(save_output(&path, Output::Binary(b"replacement".to_vec())).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
}
