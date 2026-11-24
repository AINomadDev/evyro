//! Remote access from the Settings screen: run `evyro serve` inside the app
//! (so a paired phone sees and approves this window's chats), pair devices,
//! and revoke them.

use crate::chat::Chats;
use crate::{with_store, CmdResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
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
pub async fn remote_tailscale(remote: State<'_, Remote>, enable: Option<bool>) -> CmdResult {
    let port = remote
        .running
        .lock()
        .map_err(bad)?
        .as_ref()
        .filter(|r| r.addr.ip() == std::net::Ipv4Addr::LOCALHOST)
        .map(|r| r.addr.port());
    let Some(port) = port else {
        return Ok(json!({"state":"start_loopback"}));
    };
    tauri::async_runtime::spawn_blocking(move || {
        let bin = evyro_core::adapters::which("tailscale").or_else(|| {
            [
                "/usr/local/bin/tailscale",
                "/opt/homebrew/bin/tailscale",
                "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
            ]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        });
        let Some(bin) = bin else {
            return json!({"state":"missing"});
        };
        let output = |args: &[&str]| -> Option<Value> {
            let out = Command::new(&bin).args(args).output().ok()?;
            if !out.status.success() {
                return None;
            }
            serde_json::from_slice(&out.stdout).ok()
        };
        let Some(status) = output(&["status", "--json"]) else {
            return json!({"state":"unavailable"});
        };
        if status["BackendState"] != "Running" {
            return json!({"state":"disconnected"});
        }
        let Some(config) = output(&["serve", "status", "--json"]) else { return json!({"state":"unavailable"}); };
        let mut route = tailscale_route(&config, port);
        if enable == Some(true) && route["state"] == "setup" {
            let https = format!("--https={}", route["https_port"]);
            let local = format!("http://127.0.0.1:{port}");
            let enabled = Command::new(&bin).args(["serve", "--bg", &https, &local])
                .stdin(std::process::Stdio::null()).output().is_ok_and(|out| out.status.success());
            if enabled {
                if let Some(updated) = output(&["serve", "status", "--json"]) {
                    return tailscale_route(&updated, port);
                }
            }
            route["feedback"] = json!("Tailscale did not confirm the route. Run the command in Terminal to see its setup prompt.");
        }
        route
    })
    .await
    .map_err(bad)
}

fn tailscale_route(config: &Value, port: u16) -> Value {
    let expected = format!("http://127.0.0.1:{port}");
    let mut private_url = None;
    if let Some(web) = config["Web"].as_object() {
        for (endpoint, site) in web {
            let Some((host, https_port)) = endpoint.rsplit_once(':').filter(|(h, p)| {
                h.ends_with(".ts.net")
                    && h.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
                    && p.parse::<u16>().is_ok()
            }) else {
                continue;
            };
            if config["TCP"][https_port]["HTTPS"] == true
                && site["Handlers"]["/"]["Proxy"]
                    .as_str()
                    .is_some_and(|p| p.trim_end_matches('/') == expected)
            {
                if config["AllowFunnel"][endpoint] == true {
                    return json!({"state": "public"});
                }
                let authority = if https_port == "443" { host } else { endpoint };
                private_url = Some(format!("https://{authority}/"));
            }
        }
    }
    if let Some(url) = private_url {
        return json!({"state": "ready", "url": url});
    }
    let https_port = [8443, 10000, 443]
        .into_iter()
        .find(|p| config["TCP"][p.to_string()].is_null());
    https_port.map_or_else(
        || json!({"state": "ports_busy"}),
        |p| json!({"state": "setup", "port": port, "https_port": p}),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_private_https_routes_to_this_listener() {
        let mut config = json!({"TCP":{"8443":{"HTTPS":true}}, "Web":{"mac.example.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:7420"}}}}});
        assert_eq!(
            tailscale_route(&config, 7420)["url"],
            "https://mac.example.ts.net:8443/"
        );
        assert_eq!(tailscale_route(&config, 7400)["state"], "setup");
        assert_eq!(tailscale_route(&config, 7400)["https_port"], 10000);
        config["AllowFunnel"] = json!({"mac.example.ts.net:8443":true});
        assert_eq!(tailscale_route(&config, 7420)["state"], "public");
    }
}
