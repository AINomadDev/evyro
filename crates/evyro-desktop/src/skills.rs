//! The Skills screen: list, read, turn on or off, create, import, and sync
//! skills into an agent's instruction files. Everything goes through
//! `evyro_core::skills`, the same code the CLI and the MCP server use.

use crate::{with_store, CmdResult};
use evyro_core::skills;
use serde_json::json;
use std::path::PathBuf;

#[tauri::command]
pub async fn list_skills(project_id: String) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let all = s.skills(Some(&p))?;
        Ok(json!({
            "skills": all,
            "project_dir": skills::project_dir(&p),
            "global_dir": skills::global_dir(&s.paths),
            "sync_targets": skills::SYNC_TARGETS.iter().map(|(h, f)| json!({"harness": h, "file": f})).collect::<Vec<_>>(),
        }))
    })
    .await
}

#[tauri::command]
pub async fn skill_text(project_id: String, name: String) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let k = s.skill(Some(&p), &name)?;
        let text = if k.usable() { k.body() } else { String::new() };
        Ok(json!({"skill": k, "text": text}))
    })
    .await
}

#[tauri::command]
pub async fn set_skill_enabled(
    project_id: String,
    name: String,
    enabled: bool,
    global: bool,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        if global {
            s.set_skill_enabled(None, &name, enabled)?;
            // The project follows the global choice from now on.
            s.reset_skill_setting(Some(&p), &name)?;
        } else {
            s.set_skill_enabled(Some(&p), &name, enabled)?;
        }
        Ok(json!(s.skill(Some(&p), &name)?))
    })
    .await
}

#[tauri::command]
pub async fn new_skill(
    project_id: String,
    name: String,
    description: String,
    global: bool,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let f = skills::create(s, if global { None } else { Some(&p) }, &name, &description)?;
        Ok(json!({"path": f}))
    })
    .await
}

#[tauri::command]
pub async fn import_skills(
    project_id: String,
    path: String,
    global: bool,
    force: bool,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        Ok(json!(skills::import(
            s,
            &PathBuf::from(path),
            if global { None } else { Some(&p) },
            force
        )?))
    })
    .await
}

#[tauri::command]
pub async fn sync_skills(project_id: String, harness: String, append: bool) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        // The window never replaces files someone else wrote; the CLI's --force does.
        Ok(json!(skills::sync(s, &p, &harness, &[], append, false)?))
    })
    .await
}

#[tauri::command]
pub async fn pick_skill_folder(app: tauri::AppHandle) -> CmdResult {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Choose a skill folder, or a folder of skills")
            .blocking_pick_folder();
        json!(picked.map(|p| p.to_string()))
    })
    .await
    .map_err(|e| json!({"code": "internal", "message": e.to_string()}))
}
