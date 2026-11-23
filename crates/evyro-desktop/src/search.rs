//! Code search for the window: file names, text or regex, symbols, and a
//! read-only file viewer. All of it goes through `evyro_core::search`, so the
//! window, the CLI and the MCP server skip and redact the same things.

use crate::{with_store, CmdResult};
use evyro_core::search::{self, TextQuery};
use serde_json::json;

#[tauri::command]
pub async fn find_files(project_id: String, query: String, limit: Option<usize>) -> CmdResult {
    with_store(move |s| {
        let root = s.get_project(&project_id)?.root();
        Ok(json!(search::find_files(
            &root,
            &query,
            limit.unwrap_or(50).clamp(1, 500)
        )?))
    })
    .await
}

#[tauri::command]
pub async fn search_text(
    project_id: String,
    pattern: String,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    path: Option<String>,
    limit: Option<usize>,
) -> CmdResult {
    with_store(move |s| {
        let root = s.get_project(&project_id)?.root();
        let path = path.filter(|p| !p.trim().is_empty());
        Ok(json!(search::search_text(
            &root,
            &TextQuery {
                pattern: &pattern,
                regex: regex.unwrap_or(false),
                case_sensitive: case_sensitive.unwrap_or(false),
                path_filter: path.as_deref(),
                limit: limit.unwrap_or(200).clamp(1, 1000),
            }
        )?))
    })
    .await
}

#[tauri::command]
pub async fn find_symbols(project_id: String, query: String, limit: Option<usize>) -> CmdResult {
    with_store(move |s| {
        let root = s.get_project(&project_id)?.root();
        Ok(json!(search::find_symbols(
            &root,
            &query,
            limit.unwrap_or(50).clamp(1, 500)
        )?))
    })
    .await
}

#[tauri::command]
pub async fn view_file(project_id: String, path: String) -> CmdResult {
    with_store(move |s| {
        let root = s.get_project(&project_id)?.root();
        search::view_file(&root, &path)
    })
    .await
}
