//! `grep` delegates matching, cancellation and persistence to global search.
use clap::ArgMatches;
use serde_json::json;
use tauri::Manager;

use super::{CliError, Output};
use crate::commands::search::global_search::{global_search_hits_page, global_search_start};

pub(super) async fn grep(
    app: &tauri::AppHandle,
    image_id: &str,
    node_id: &str,
    args: &ArgMatches,
) -> Result<Output, CliError> {
    let pattern = args
        .get_one::<String>("pattern")
        .ok_or_else(|| CliError::invalid("grep 需要关键词或正则"))?;
    let codepages = args
        .get_many::<String>("codepage")
        .map(|values| values.cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let options = serde_json::from_value(json!({
        "keys": if args.get_flag("regex") { Vec::<String>::new() } else { vec![pattern.clone()] },
        "regexes": if args.get_flag("regex") { vec![pattern.clone()] } else { Vec::<String>::new() },
        "codepages": codepages,
        "match_case": !args.get_flag("ignore-case"),
        "search_deleted": !args.get_flag("exclude-deleted"),
        "scope": { "tree_nodes": if node_id.is_empty() { Vec::<String>::new() } else { vec![node_id.to_owned()] } },
    }))?;
    let search_id = args
        .get_one::<String>("search-id")
        .cloned()
        .unwrap_or_else(|| format!("grep-{}", uuid::Uuid::new_v4()));
    let summary = global_search_start(
        app.clone(),
        app.state(),
        image_id.to_owned(),
        search_id.clone(),
        options,
    )
    .await
    .map_err(super::command_error)?;
    let limit = u64::from(*args.get_one::<u32>("limit").unwrap_or(&10000));
    let mut hits = Vec::new();
    let total = loop {
        let page = global_search_hits_page(
            app.state(),
            image_id.to_owned(),
            search_id.clone(),
            Some(hits.len() as u64),
            Some((limit - hits.len() as u64).min(2000)),
            None,
            None,
            None,
        )
        .await
        .map_err(super::command_error)?;
        let empty = page.items.is_empty();
        hits.extend(page.items);
        if empty || hits.len() as u64 >= page.total || hits.len() as u64 >= limit {
            break page.total;
        }
    };
    let truncated = summary.truncated || summary.status != "success" || (hits.len() as u64) < total;
    Ok(Output::Json(
        json!({"summary":summary,"hits":hits,"total":total,"truncated":truncated}),
    ))
}
