//! Checking for and installing a new version of the desktop app.
//!
//! Releases carry a `latest.json` and installers signed with Evyro's update
//! key; the app holds the public key (`plugins.updater.pubkey` in
//! tauri.conf.json) and refuses an update whose signature does not match.
//! Nothing is downloaded until the person clicks Install.

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

type CmdResult = Result<Value, Value>;

/// Builds made before the release key exists carry this placeholder instead of
/// a public key. They cannot verify an update, so they never offer one.
const KEY_PLACEHOLDER: &str = "UPDATE_PUBLIC_KEY_PENDING";

fn key_configured() -> bool {
    !include_str!("../tauri.conf.json").contains(KEY_PLACEHOLDER)
}

fn bad(msg: impl std::fmt::Display) -> Value {
    json!({"code": "update", "message": msg.to_string()})
}

/// Is a newer version published? Returns `{configured, available, current, version, notes, date}`.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult {
    let current = app.package_info().version.to_string();
    if !key_configured() {
        return Ok(json!({"configured": false, "available": false, "current": current}));
    }
    let updater = app.updater().map_err(bad)?;
    match updater.check().await.map_err(bad)? {
        Some(u) => Ok(json!({
            "configured": true,
            "available": true,
            "current": current,
            "version": u.version,
            "notes": u.body,
            "date": u.date.map(|d| d.to_string()),
        })),
        None => Ok(json!({"configured": true, "available": false, "current": current})),
    }
}

/// Download, verify and install the newer version, then restart into it.
/// The restart goes through the normal exit path, which stops running agents.
/// (On Windows the installer closes the app itself.)
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult {
    if !key_configured() {
        return Err(bad(
            "this build has no update key, so it cannot install updates",
        ));
    }
    let updater = app.updater().map_err(bad)?;
    let Some(u) = updater.check().await.map_err(bad)? else {
        return Err(bad("no newer version is available"));
    };
    u.download_and_install(|_, _| {}, || {})
        .await
        .map_err(bad)?;
    app.request_restart();
    Ok(Value::Null)
}
