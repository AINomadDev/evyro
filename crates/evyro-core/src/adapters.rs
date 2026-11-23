//! Agent adapter framework: explicit, versioned capability declarations per
//! harness, install/version detection, and safe argv construction.
//!
//! Provider (inference source), model, harness (tool-running agent) and
//! IDE/client are distinct concepts; adapters describe *harnesses*.
//!
//! Capability flags describe what Evyro uses, and `evidence` says how each
//! adapter was validated. Nothing here claims a native API that does not exist:
//! harnesses without one fall back to instruction files + MCP + CLI.

use crate::error::{Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const ADAPTER_SPEC_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct Capabilities {
    pub install_detection: bool,
    pub version_detection: bool,
    /// Non-interactive "run this prompt" mode.
    pub headless_launch: bool,
    /// Start the interactive TUI with an initial prompt.
    pub interactive_launch: bool,
    /// Reads a repo instruction file automatically (CLAUDE.md / AGENTS.md / GEMINI.md).
    pub instruction_file: bool,
    /// Can be configured to use Evyro's MCP server.
    pub mcp: bool,
    /// Speaks the Agent Client Protocol.
    pub acp: bool,
    /// Has lifecycle hooks Evyro can use for automatic checkpoints.
    pub checkpoint_hooks: bool,
    /// Structured (JSON) event stream in headless mode.
    pub structured_events: bool,
    /// Evyro can read official quota/usage telemetry. False everywhere today.
    pub quota_metadata: bool,
    pub model_selection: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdapterSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub binaries: &'static [&'static str],
    pub instruction_file: &'static str,
    pub capabilities: Capabilities,
    pub min_version: Option<&'static str>,
    /// Shell-free MCP registration command(s) or config hint.
    pub mcp_setup: &'static str,
    pub model_flag: Option<&'static str>,
    /// How the declarations were validated: "documented" (from official docs /
    /// reference implementations) until a real install passes `evyro agents --probe`.
    pub evidence: &'static str,
    pub notes: &'static str,
    #[serde(skip)]
    headless: Option<fn(&str, &str) -> Vec<String>>,
    #[serde(skip)]
    interactive: Option<fn(&str, &str) -> Vec<String>>,
}

const FULL_CLI: Capabilities = Capabilities {
    install_detection: true,
    version_detection: true,
    headless_launch: true,
    interactive_launch: true,
    instruction_file: true,
    mcp: true,
    acp: false,
    checkpoint_hooks: false,
    structured_events: false,
    quota_metadata: false,
    model_selection: true,
};

pub fn registry() -> Vec<AdapterSpec> {
    vec![
        AdapterSpec {
            id: "claude-code",
            display_name: "Claude Code",
            binaries: &["claude"],
            instruction_file: "CLAUDE.md",
            capabilities: Capabilities { checkpoint_hooks: true, structured_events: true, ..FULL_CLI },
            min_version: Some("1.0.0"),
            mcp_setup: "claude mcp add evyro -- evyro mcp",
            model_flag: Some("--model"),
            evidence: "documented",
            notes: "Hooks (SessionStart, PostToolUse, PreCompact, Stop) can call `evyro checkpoint --auto`; see `evyro integrate claude-code`.",
            headless: Some(|bin, p| vec![bin.into(), "-p".into(), p.into()]),
            interactive: Some(|bin, p| vec![bin.into(), p.into()]),
        },
        AdapterSpec {
            id: "codex",
            display_name: "OpenAI Codex CLI",
            binaries: &["codex"],
            instruction_file: "AGENTS.md",
            capabilities: FULL_CLI,
            min_version: Some("0.1.0"),
            mcp_setup: "codex mcp add evyro -- evyro mcp",
            model_flag: Some("--model"),
            evidence: "documented",
            notes: "Reads AGENTS.md. `codex exec` is the non-interactive mode.",
            headless: Some(|bin, p| vec![bin.into(), "exec".into(), p.into()]),
            interactive: Some(|bin, p| vec![bin.into(), p.into()]),
        },
        AdapterSpec {
            id: "gemini",
            display_name: "Gemini CLI",
            binaries: &["gemini"],
            instruction_file: "GEMINI.md",
            capabilities: Capabilities { structured_events: true, ..FULL_CLI },
            min_version: Some("0.1.0"),
            mcp_setup: "gemini mcp add evyro evyro mcp",
            model_flag: Some("--model"),
            evidence: "documented",
            notes: "Reads GEMINI.md. `-p` is headless; `-i` starts interactive with a prompt.",
            headless: Some(|bin, p| vec![bin.into(), "-p".into(), p.into()]),
            interactive: Some(|bin, p| vec![bin.into(), "-i".into(), p.into()]),
        },
        AdapterSpec {
            id: "opencode",
            display_name: "OpenCode",
            binaries: &["opencode"],
            instruction_file: "AGENTS.md",
            capabilities: FULL_CLI,
            min_version: Some("0.1.0"),
            mcp_setup: "add {\"mcp\": {\"evyro\": {\"type\": \"local\", \"command\": [\"evyro\", \"mcp\"]}}} to opencode.json",
            model_flag: Some("--model"),
            evidence: "documented (opencode source inspected at 4332e24)",
            notes: "Reads AGENTS.md. `opencode run <message>` is headless; `--prompt` seeds the TUI.",
            headless: Some(|bin, p| vec![bin.into(), "run".into(), p.into()]),
            interactive: Some(|bin, p| vec![bin.into(), "--prompt".into(), p.into()]),
        },
        AdapterSpec {
            id: "cursor",
            display_name: "Cursor (cursor-agent CLI / IDE)",
            binaries: &["cursor-agent"],
            instruction_file: "AGENTS.md",
            capabilities: Capabilities { interactive_launch: false, ..FULL_CLI },
            min_version: None,
            mcp_setup: "add evyro to .cursor/mcp.json: {\"mcpServers\": {\"evyro\": {\"command\": \"evyro\", \"args\": [\"mcp\"]}}}",
            model_flag: Some("--model"),
            evidence: "documented (px0 dispatch code inspected at b4e324b)",
            notes: "IDE usage is fallback: open the repo, the agent reads AGENTS.md and the Evyro MCP server.",
            headless: Some(|bin, p| vec![bin.into(), "-p".into(), p.into()]),
            interactive: None,
        },
        AdapterSpec {
            id: "copilot",
            display_name: "GitHub Copilot CLI",
            binaries: &["copilot"],
            instruction_file: "AGENTS.md",
            capabilities: Capabilities { interactive_launch: false, ..FULL_CLI },
            min_version: None,
            mcp_setup: "configure via `copilot` /mcp add (interactive)",
            model_flag: Some("--model"),
            evidence: "documented",
            notes: "Headless `-p` mode; IDE integrations use the fallback path.",
            headless: Some(|bin, p| vec![bin.into(), "-p".into(), p.into()]),
            interactive: None,
        },
        AdapterSpec {
            id: "goose",
            display_name: "Goose",
            binaries: &["goose"],
            instruction_file: "AGENTS.md",
            capabilities: Capabilities { interactive_launch: false, acp: true, ..FULL_CLI },
            min_version: None,
            mcp_setup: "goose configure -> Add Extension -> Command-line: evyro mcp",
            model_flag: None,
            evidence: "documented (goose source inspected at 36b2911)",
            notes: "`goose run -t <text>` is headless.",
            headless: Some(|bin, p| vec![bin.into(), "run".into(), "-t".into(), p.into()]),
            interactive: None,
        },
        AdapterSpec {
            id: "aider",
            display_name: "Aider",
            binaries: &["aider"],
            instruction_file: "AGENTS.md",
            capabilities: Capabilities { interactive_launch: false, mcp: false, ..FULL_CLI },
            min_version: None,
            mcp_setup: "aider has no MCP client; context is passed with --read",
            model_flag: Some("--model"),
            evidence: "documented",
            notes: "Evyro passes the handoff file with `--read`.",
            headless: Some(|bin, p| vec![bin.into(), "--read".into(), ".evyro/CURRENT_TASK.md".into(), "--message".into(), p.into()]),
            interactive: None,
        },
        AdapterSpec {
            id: "antigravity",
            display_name: "Google Antigravity",
            binaries: &[],
            instruction_file: "AGENTS.md",
            capabilities: Capabilities { instruction_file: true, mcp: true, ..Default::default() },
            min_version: None,
            mcp_setup: "add evyro as an MCP server in Antigravity settings (command: evyro, args: [mcp])",
            model_flag: None,
            evidence: "fallback only",
            notes: "IDE: fallback path only (instruction file + MCP + copyable resume prompt).",
            headless: None,
            interactive: None,
        },
    ]
}

pub fn get(id: &str) -> Result<AdapterSpec> {
    let id = normalize_id(id);
    registry().into_iter().find(|a| a.id == id).ok_or_else(|| {
        Error::NotFound(format!(
            "unknown agent '{id}'; known: {}",
            registry()
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

pub fn normalize_id(id: &str) -> String {
    match id.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-code" | "claudecode" => "claude-code".into(),
        "gemini" | "gemini-cli" => "gemini".into(),
        "cursor" | "cursor-agent" => "cursor".into(),
        "copilot" | "github-copilot" => "copilot".into(),
        "agy" | "antigravity" => "antigravity".into(),
        other => other.into(),
    }
}

/// Harness names are free-form (manual/IDE sessions) but restricted to a safe charset.
pub fn validate_harness_name(name: &str) -> Result<()> {
    if !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "invalid agent/harness name '{name}'"
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaunchMode {
    Headless,
    Interactive,
}

impl AdapterSpec {
    /// Build argv for launching. The prompt is a single argv element; no shell
    /// is involved, so task text and filenames cannot inject commands.
    pub fn launch_argv(
        &self,
        bin: &str,
        mode: LaunchMode,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<Vec<String>> {
        let f = match mode {
            LaunchMode::Headless => self.headless,
            LaunchMode::Interactive => self.interactive.or(self.headless),
        }
        .ok_or_else(|| Error::Unsupported(format!(
            "{} has no launchable CLI mode in Evyro; use the fallback: open the repo in {} and paste the resume prompt",
            self.display_name, self.display_name
        )))?;
        let mut argv = f(bin, prompt);
        if let (Some(m), Some(flag)) = (model, self.model_flag) {
            argv.insert(1, flag.into());
            argv.insert(2, m.into());
        } else if model.is_some() {
            return Err(Error::Unsupported(format!(
                "{} does not support model selection through Evyro",
                self.display_name
            )));
        }
        Ok(argv)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub id: String,
    pub display_name: String,
    pub installed: bool,
    pub binary: Option<String>,
    pub version: Option<String>,
    /// ok | incompatible | unknown_version | not_installed | fallback_only
    pub compatibility: String,
    pub diagnostic: Option<String>,
    pub capabilities: Capabilities,
    pub evidence: String,
    pub instruction_file: String,
    pub mcp_setup: String,
}

/// Find an executable on PATH (honours PATHEXT on Windows).
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or(".EXE;.CMD;.BAT;.COM".into())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let p = dir.join(format!("{bin}{ext}"));
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    match std::fs::metadata(p) {
        Ok(m) if m.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                m.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
        _ => false,
    }
}

/// Run `<bin> --version` with a timeout; returns stdout+stderr.
pub fn probe_version(bin: &Path, timeout: Duration) -> Option<String> {
    let mut child = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(s)
}

/// Extract the first `x.y.z` (or `x.y`) version from text.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let re = regex::Regex::new(r"(\d+)\.(\d+)(?:\.(\d+))?").ok()?;
    let c = re.captures(s)?;
    Some((
        c[1].parse().ok()?,
        c[2].parse().ok()?,
        c.get(3)
            .map(|m| m.as_str().parse().unwrap_or(0))
            .unwrap_or(0),
    ))
}

pub fn detect(spec: &AdapterSpec) -> Detection {
    let mut d = Detection {
        id: spec.id.into(),
        display_name: spec.display_name.into(),
        installed: false,
        binary: None,
        version: None,
        compatibility: "not_installed".into(),
        diagnostic: None,
        capabilities: spec.capabilities,
        evidence: spec.evidence.into(),
        instruction_file: spec.instruction_file.into(),
        mcp_setup: spec.mcp_setup.into(),
    };
    if spec.binaries.is_empty() {
        d.compatibility = "fallback_only".into();
        d.diagnostic = Some(spec.notes.into());
        return d;
    }
    let Some((bin, path)) = spec.binaries.iter().find_map(|b| which(b).map(|p| (*b, p))) else {
        d.diagnostic = Some(format!(
            "`{}` not found on PATH",
            spec.binaries.join("` or `")
        ));
        return d;
    };
    let _ = bin;
    d.installed = true;
    d.binary = Some(path.to_string_lossy().to_string());
    let raw = probe_version(&path, Duration::from_secs(8));
    let ver = raw.as_deref().and_then(parse_version);
    d.version = ver.map(|(a, b, c)| format!("{a}.{b}.{c}"));
    d.compatibility = match (ver, spec.min_version.and_then(parse_version)) {
        (None, _) => {
            d.diagnostic = Some(
                "could not read a version from `--version`; integration may still work".into(),
            );
            "unknown_version".into()
        }
        (Some(v), Some(min)) if v < min => {
            d.diagnostic = Some(format!(
                "{} {}.{}.{} is older than the minimum Evyro supports ({}). Upgrade it, then re-run `evyro agents`.",
                spec.display_name, v.0, v.1, v.2, spec.min_version.unwrap()
            ));
            "incompatible".into()
        }
        _ => "ok".into(),
    };
    d
}

pub fn detect_all() -> Vec<Detection> {
    registry().iter().map(detect).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_is_shell_free() {
        let a = get("codex").unwrap();
        let p = "do X; rm -rf / && echo $(whoami)";
        let argv = a
            .launch_argv("codex", LaunchMode::Headless, p, None)
            .unwrap();
        assert_eq!(argv, vec!["codex".to_string(), "exec".into(), p.into()]);
    }
    #[test]
    fn model_flag_inserted() {
        let a = get("claude").unwrap();
        let argv = a
            .launch_argv("claude", LaunchMode::Headless, "hi", Some("opus"))
            .unwrap();
        assert_eq!(
            argv,
            ["claude", "--model", "opus", "-p", "hi"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn unsupported_is_structured() {
        let a = get("antigravity").unwrap();
        let e = a
            .launch_argv("x", LaunchMode::Headless, "hi", None)
            .unwrap_err();
        assert_eq!(e.code(), "unsupported");
    }
    #[test]
    fn versions() {
        assert_eq!(parse_version("codex-cli 0.48.0"), Some((0, 48, 0)));
        assert_eq!(parse_version("2.1 (Claude Code)"), Some((2, 1, 0)));
        assert_eq!(parse_version("none"), None);
    }
    #[test]
    fn harness_names() {
        assert!(validate_harness_name("claude-code").is_ok());
        assert!(validate_harness_name("a;b").is_err());
    }
}
