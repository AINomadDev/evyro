//! Remote access from the Settings screen: run `evyro serve` inside the app
//! (so a paired phone sees and approves this window's chats), pair devices,
//! and revoke them.

use crate::chat::Chats;
use crate::{with_store, CmdResult};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Default)]
pub struct RemoteState {
    running: Mutex<Option<evyro_remote::Running>>,
}

pub type Remote = Arc<RemoteState>;

fn bad(msg: impl std::fmt::Display) -> Value {
    json!({"code": "remote", "message": msg.to_string()})
}

fn status_of(r: &Option<evyro_remote::Running>) -> Value {
    match r {
        Some(r) => json!({"running": true, "url": r.url(), "reach": r.reach}),
        None => json!({"running": false}),
    }
}

#[tauri::command]
pub async fn remote_status(remote: State<'_, Remote>) -> CmdResult {
    let r = remote.running.lock().map_err(bad)?;
    Ok(status_of(&r))
}

#[tauri::command]
pub async fn remote_start(
    remote: State<'_, Remote>,
    chats: State<'_, Chats>,
    host: String,
    port: u16,
    allow_lan: bool,
) -> CmdResult {
    let mut r = remote.running.lock().map_err(bad)?;
    if r.is_none() {
        let running = evyro_remote::start(
            evyro_remote::ServeOpts {
                host,
                port,
                allow_lan,
                evyro_mcp: None,
            },
            Some((chats.host.clone(), chats.feed.clone())),
        )
        .map_err(bad)?;
        *r = Some(running);
    }
    Ok(status_of(&r))
}

#[tauri::command]
pub async fn remote_stop(remote: State<'_, Remote>) -> CmdResult {
    let running = remote.running.lock().map_err(bad)?.take();
    if let Some(r) = running {
        tauri::async_runtime::spawn_blocking(move || r.stop())
            .await
            .map_err(bad)?;
    }
    Ok(json!({"running": false}))
}

#[tauri::command]
pub async fn remote_devices() -> CmdResult {
    with_store(|s| Ok(json!(s.list_devices()?))).await
}

#[tauri::command]
pub async fn remote_pair(name: String, scope: String, minutes: Option<i64>) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.pair_device(
            &name,
            &scope,
            minutes.unwrap_or(10) * 60
        )?))
    })
    .await
}

#[tauri::command]
pub async fn remote_revoke(id: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.revoke_device(&id)?))).await
}

/// When the app quits.
pub fn stop(remote: &Remote) {
    if let Some(r) = remote.running.lock().ok().and_then(|mut r| r.take()) {
        r.stop();
    }
}
