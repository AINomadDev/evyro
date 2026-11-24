//! Read account limits from the installed agents without starting a model turn.

use crate::{with_store, CmdResult};
use evyro_core::{adapters, quota, targets, util};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Clone, Copy)]
enum Provider {
    Codex,
    Copilot,
}

#[tauri::command]
pub async fn agent_quota(target: String) -> CmdResult {
    with_store(move |s| {
        let target = targets::resolve_target(&s.paths, &target)?;
        let harness = target.effective_harness().unwrap_or_default();
        // A custom provider or extra account arguments may use a different account.
        if target.kind != targets::TargetKind::Harness {
            return Ok(json!({"state": "unsupported"}));
        }
        let Some(cache) = quota::cache_path(&s.paths, &harness) else {
            return Ok(json!({"state": "unsupported"}));
        };
        if harness == "claude-code" {
            return Ok(read_snapshot(&cache).map_or_else(|| json!({
                "state": "setup",
                "message": "Read the limits reported by your Claude Code status line.",
                "capture_command": format!("{} capture-usage claude-code", crate::threads::quote(&crate::threads::evyro_cli(), cfg!(windows))),
            }), |mut snapshot| {
                snapshot["cached"] = json!(true);
                snapshot["source"] = json!("Status line");
                snapshot
            }));
        }
        if !matches!(target.auth, targets::AuthMode::Auto | targets::AuthMode::AppLogin) {
            return Ok(json!({"state": "unsupported"}));
        }
        let plan = targets::launch_plan(&target)?;
        if !plan.extra_args.is_empty() || target.provider.is_some() {
            return Ok(json!({"state": "unsupported"}));
        }
        let bin_name = if harness == "antigravity-cli" { "agy" } else { &harness };
        let reading = if let Some(bin) = executable(bin_name) {
            if harness == "antigravity-cli" {
                antigravity(&bin)
            } else {
                let provider = if harness == "codex" { Provider::Codex } else { Provider::Copilot };
                let argv = adapters::launchable_argv(vec![bin.to_string_lossy().into_owned()]);
                let mut cmd = Command::new(&argv[0]);
                cmd.args(&argv[1..]);
                for name in &plan.env_remove {
                    cmd.env_remove(name);
                }
                cmd.envs(plan.env_set);
                if let Some(home) = std::env::var_os("HOME") {
                    cmd.current_dir(home);
                }
                match request(cmd, provider) {
                    Ok(value) => summarize(provider, &value),
                    Err(_) => json!({"state": "unavailable", "message": "Could not read quota from the CLI. Check its sign-in and version."}),
                }
            }
        } else {
            json!({"state": "unavailable", "message": "Agent CLI was not found."})
        };
        Ok(remember_reading(&cache, reading))
    })
    .await
}

/// OpenCode records metered usage by project. The connected provider owns the balance.
#[tauri::command]
pub async fn opencode_usage(project_id: String) -> CmdResult {
    with_store(move |s| {
        let project = s.get_project(&project_id)?;
        let Some(bin) = executable("opencode") else {
            return Ok(json!({"state": "unavailable", "message": "OpenCode CLI was not found."}));
        };
        let argv = adapters::launchable_argv(vec![bin.to_string_lossy().into_owned()]);
        let mut child = match Command::new(&argv[0])
            .args(&argv[1..])
            .args(["stats", "--json", "--days", "7", "--project", ".", "--cost"])
            .current_dir(project.root())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return Ok(json!({"state": "unavailable", "message": "OpenCode stats could not start."})),
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let read = stdout.take(1024 * 1024).read_to_end(&mut data);
            let result = read.ok().and_then(|_| serde_json::from_slice::<Value>(&data).ok());
            let _ = tx.send(result);
        });
        let report = rx.recv_timeout(Duration::from_secs(8)).ok().flatten();
        let _ = child.kill();
        let _ = child.wait();
        Ok(report.as_ref().and_then(opencode_stats).unwrap_or_else(|| json!({
            "state": "unavailable",
            "message": "OpenCode usage was unavailable. Try opencode stats --days 7 --project . in this project.",
        })))
    })
    .await
}

fn opencode_stats(report: &Value) -> Option<Value> {
    Some(json!({
        "state": "stats",
        "sessions": report["sessions"].as_u64()?,
        "input": report["tokens"]["input"].as_u64()?,
        "output": report["tokens"]["output"].as_u64()?,
        "cost": report["cost"].as_f64(),
        "observed_at": util::now(),
        "source": "OpenCode stats",
    }))
}

fn read_snapshot(path: &Path) -> Option<Value> {
    let data = std::fs::read(path).ok()?;
    let snapshot: Value = serde_json::from_slice(&data).ok()?;
    (snapshot["state"] == "ready").then_some(snapshot)
}

fn remember_reading(cache: &Path, mut reading: Value) -> Value {
    if reading["state"] == "ready" {
        if reading["observed_at"].is_null() {
            reading["observed_at"] = json!(util::now());
        }
        if let Err(err) = util::write_atomic(cache, reading.to_string().as_bytes()) {
            eprintln!("Could not store quota reading: {err}");
        }
        return reading;
    }
    if let Some(mut snapshot) = read_snapshot(cache) {
        snapshot["stale"] = json!(true);
        snapshot
    } else {
        reading
    }
}

#[cfg(unix)]
fn antigravity(bin: &Path) -> Value {
    match antigravity_snapshot(bin) {
        Some(mut snapshot) => {
            snapshot["source"] = json!("Antigravity CLI");
            snapshot
        }
        None => {
            json!({"state": "unavailable", "message": "No quota reported by Antigravity CLI. Check sign-in and CLI version, or run /usage in its terminal."})
        }
    }
}

#[cfg(unix)]
fn antigravity_snapshot(bin: &Path) -> Option<Value> {
    use crate::threads::{evyro_cli, quote};
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::time::Instant;

    let dir = tempfile::tempdir().ok()?;
    let output = dir.path().join("quota.json");
    let cli = evyro_cli();
    let hook = format!(
        "{} capture-usage antigravity-cli --output {}",
        quote(&cli, false),
        quote(&output.to_string_lossy(), false)
    );
    let settings = json!({"statusLine": {"type": "command", "command": hook, "enabled": true}});
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 25,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .ok()?;
    let mut reader = pair.master.try_clone_reader().ok()?;
    let mut writer = pair.master.take_writer().ok()?;
    let mut cmd = CommandBuilder::new(bin);
    cmd.args(["--settings", &settings.to_string()]);
    cmd.cwd(dir.path());
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd).ok()?;
    drop(pair.slave);
    // Drain the private TUI and answer its cursor position query. No prompts are sent.
    std::thread::spawn(move || {
        let mut data = [0; 4096];
        while let Ok(n @ 1..) = reader.read(&mut data) {
            if data[..n].windows(4).any(|s| s == b"\x1b[6n") {
                let _ = writer.write_all(b"\x1b[1;1R");
            }
        }
    });
    let end = Instant::now() + Duration::from_secs(8);
    let reading = loop {
        if let Some(snapshot) = read_snapshot(&output) {
            break Some(snapshot);
        }
        if Instant::now() >= end || child.try_wait().ok().flatten().is_some() {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = child.kill();
    let _ = child.wait();
    reading
}

#[cfg(not(unix))]
fn antigravity(_bin: &Path) -> Value {
    json!({"state": "unsupported", "message": "Run /usage in the Antigravity terminal for account limits."})
}

fn executable(harness: &str) -> Option<PathBuf> {
    adapters::which(harness).or_else(|| {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        [
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
        .into_iter()
        .map(|dir| dir.join(harness))
        .find(|bin| bin.is_file())
    })
}

fn request(mut cmd: Command, provider: Provider) -> io::Result<Value> {
    match provider {
        Provider::Codex => cmd.arg("app-server"),
        Provider::Copilot => cmd.args(["--headless", "--no-auto-update", "--stdio"]),
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| {
            let version = env!("CARGO_PKG_VERSION");
            let send = |input: &mut dyn Write, message: Value| -> io::Result<()> {
                let message = message.to_string();
                if matches!(provider, Provider::Copilot) {
                    write!(input, "Content-Length: {}\r\n\r\n{message}", message.len())?;
                } else {
                    writeln!(input, "{message}")?;
                }
                input.flush()
            };
            let mut reply = |id| loop {
                let value = read_message(&mut output, provider)?;
                if value["id"].as_i64() == Some(id) {
                    return Ok::<_, io::Error>(value);
                }
            };
            match provider {
                Provider::Codex => {
                    send(
                        &mut input,
                        json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "evyro", "version": version}}}),
                    )?;
                    reply(1)?;
                    send(&mut input, json!({"method": "initialized"}))?;
                    send(
                        &mut input,
                        json!({"id": 2, "method": "account/rateLimits/read"}),
                    )?;
                }
                Provider::Copilot => {
                    send(
                        &mut input,
                        json!({"jsonrpc":"2.0", "id": 1, "method": "connect", "params": {}}),
                    )?;
                    reply(1)?;
                    send(
                        &mut input,
                        json!({"jsonrpc":"2.0", "id": 2, "method": "account.getQuota", "params": {}}),
                    )?;
                }
            }
            reply(2)?
                .get("result")
                .cloned()
                .ok_or_else(|| io::Error::other("Quota reply unavailable"))
        })();
        let _ = tx.send(result);
    });
    let result = rx.recv_timeout(Duration::from_secs(8));
    let _ = child.kill();
    let _ = child.wait();
    result.map_err(io::Error::other)?
}

fn read_message(reader: &mut impl BufRead, provider: Provider) -> io::Result<Value> {
    const MAX_MESSAGE: u64 = 1024 * 1024;
    let mut line = String::new();
    if matches!(provider, Provider::Codex) {
        reader.take(MAX_MESSAGE).read_line(&mut line)?;
        return Ok(serde_json::from_str(&line)?);
    }
    let mut size = None;
    loop {
        line.clear();
        if reader.take(1024).read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(length) = line.strip_prefix("Content-Length:") {
            size = length
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|n| *n <= MAX_MESSAGE);
        }
    }
    let size = size.ok_or_else(|| io::Error::other("Invalid quota response framing"))?;
    let mut body = vec![0; size as usize];
    reader.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

fn summarize(provider: Provider, response: &Value) -> Value {
    let mut windows = Vec::new();
    let mut credits = None;
    match provider {
        Provider::Codex => {
            let primary = &response["rateLimits"];
            let buckets: Vec<_> = response["rateLimitsByLimitId"]
                .as_object()
                .filter(|m| !m.is_empty())
                .map_or_else(|| vec![primary], |m| m.values().collect());
            for bucket in buckets {
                for key in ["primary", "secondary"] {
                    let window = &bucket[key];
                    if let Some(used) = window["usedPercent"].as_f64() {
                        windows.push(json!({"label": bucket["limitName"].as_str(), "remaining": (100.0 - used).clamp(0.0, 100.0), "minutes": window["windowDurationMins"], "resets_at": window["resetsAt"]}));
                    }
                }
            }
            credits = primary["credits"]["balance"].as_str().map(String::from);
            if primary["credits"]["unlimited"].as_bool() == Some(true) {
                credits = Some("Unlimited".into());
            }
        }
        Provider::Copilot => {
            // This SDK method may return a cached provider snapshot. Its
            // timestamp varies by CLI version, so do not present it as a reset.
            if let Some(quota) = response["quotaSnapshots"]["premium_interactions"].as_object() {
                if let Some(left) = quota.get("remainingPercentage").and_then(Value::as_f64) {
                    windows.push(json!({"label": "Plan", "remaining": left.clamp(0.0, 100.0)}));
                }
            }
        }
    }
    if windows.is_empty() && credits.is_none() {
        return json!({"state": "unavailable", "message": "The CLI returned no supported quota for this account."});
    }
    json!({"state": "ready", "windows": windows, "credits": credits, "cached": matches!(provider, Provider::Copilot)})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_reported_quota_without_treating_missing_data_as_zero() {
        let codex = summarize(
            Provider::Codex,
            &json!({"rateLimits": {"primary": {"usedPercent": 27, "windowDurationMins": 300}, "credits": {"balance": "0"}}}),
        );
        assert_eq!(codex["windows"][0]["remaining"], 73.0);
        assert_eq!(codex["credits"], "0");
        let copilot = summarize(
            Provider::Copilot,
            &json!({"quotaSnapshots":{"premium_interactions":{"remainingPercentage":54}}}),
        );
        assert_eq!(copilot["windows"][0]["remaining"], 54.0);
        assert_eq!(copilot["cached"], true);
        assert_eq!(
            summarize(Provider::Codex, &Value::Null)["state"],
            "unavailable"
        );
    }
}
