//! Switch targets: every way a person uses AI for coding, behind one name.
//!
//! A target is what `evyro switch --to <name>` hands work to. Four kinds:
//!
//! | Kind | Example | How Evyro hands over |
//! |---|---|---|
//! | `harness` | Claude Code, Codex, OpenCode, Aider | launches the CLI in the repo with the resume prompt |
//! | `ide` | Cursor, VS Code + Copilot, Windsurf, Zed | opens the repo in the IDE and copies the resume prompt |
//! | `chat` | ChatGPT, Claude app, Gemini app | builds a self-contained chat pack and copies it |
//! | `api` | any provider + model with your key | runs a provider-neutral harness (OpenCode, Aider, Goose) with that model |
//!
//! How the AI is paid for is the target's `auth`:
//! - `app_login`: the subscription you signed into in the tool (Claude Pro/Max,
//!   ChatGPT plan, Google account, Cursor, GitHub Copilot). Evyro removes API
//!   key variables from the child environment so the tool uses the login.
//! - `api_key`: a provider key. Evyro never stores the key. It reads it at
//!   launch from an environment variable (`key_env`) or, on macOS, from a
//!   Keychain item (`key_keychain`), and passes it only to the child process.
//! - `local`: a local model server (Ollama, LM Studio). No key.
//! - `auto`: leave the tool's own configuration alone.
//!
//! Built-in targets cover the common tools. Your own targets live in
//! `<config>/targets.toml` and never contain secrets.

use crate::adapters;
use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Providers

#[derive(Debug, Clone, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub display_name: &'static str,
    /// The environment variable tools conventionally read the key from.
    pub key_env: Option<&'static str>,
    pub default_base_url: Option<&'static str>,
    pub local: bool,
    pub example_model: &'static str,
}

pub fn providers() -> Vec<Provider> {
    let p = |id, display_name, key_env, base, local, example_model| Provider {
        id,
        display_name,
        key_env,
        default_base_url: base,
        local,
        example_model,
    };
    vec![
        p(
            "anthropic",
            "Anthropic",
            Some("ANTHROPIC_API_KEY"),
            None,
            false,
            "claude-sonnet-4-5",
        ),
        p(
            "openai",
            "OpenAI",
            Some("OPENAI_API_KEY"),
            None,
            false,
            "gpt-5",
        ),
        p(
            "google",
            "Google Gemini API",
            Some("GEMINI_API_KEY"),
            None,
            false,
            "gemini-2.5-pro",
        ),
        p(
            "openrouter",
            "OpenRouter",
            Some("OPENROUTER_API_KEY"),
            None,
            false,
            "moonshotai/kimi-k2",
        ),
        p(
            "groq",
            "Groq",
            Some("GROQ_API_KEY"),
            None,
            false,
            "llama-3.3-70b-versatile",
        ),
        p(
            "mistral",
            "Mistral",
            Some("MISTRAL_API_KEY"),
            None,
            false,
            "devstral-medium-latest",
        ),
        p(
            "deepseek",
            "DeepSeek",
            Some("DEEPSEEK_API_KEY"),
            None,
            false,
            "deepseek-chat",
        ),
        p("xai", "xAI", Some("XAI_API_KEY"), None, false, "grok-4"),
        p(
            "bedrock",
            "AWS Bedrock",
            None,
            None,
            false,
            "anthropic.claude-sonnet-4-5",
        ),
        p(
            "vertex",
            "Google Vertex AI",
            None,
            None,
            false,
            "gemini-2.5-pro",
        ),
        p(
            "ollama",
            "Ollama (local)",
            None,
            Some("http://localhost:11434"),
            true,
            "qwen2.5-coder:14b",
        ),
        p(
            "lmstudio",
            "LM Studio (local)",
            None,
            Some("http://localhost:1234/v1"),
            true,
            "qwen2.5-coder-14b",
        ),
        p(
            "mlx",
            "MLX on Apple silicon (local, mlx_lm.server)",
            None,
            Some("http://localhost:8080/v1"),
            true,
            "mlx-community/Qwen2.5-Coder-14B-Instruct-4bit",
        ),
        p(
            "llamacpp",
            "llama.cpp server (local; Unsloth GGUF models)",
            None,
            Some("http://localhost:8080/v1"),
            true,
            "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF",
        ),
        p(
            "vllm",
            "vLLM (local or your own GPU server)",
            None,
            Some("http://localhost:8000/v1"),
            true,
            "Qwen/Qwen2.5-Coder-32B-Instruct",
        ),
        p(
            "openai-compatible",
            "Any OpenAI-compatible endpoint",
            Some("OPENAI_API_KEY"),
            None,
            false,
            "your-model-id",
        ),
        p(
            "anthropic-compatible",
            "Any Anthropic-compatible endpoint",
            Some("ANTHROPIC_AUTH_TOKEN"),
            None,
            false,
            "your-model-id",
        ),
    ]
}

pub fn provider(id: &str) -> Result<Provider> {
    providers().into_iter().find(|p| p.id == id).ok_or_else(|| {
        Error::Invalid(format!(
            "unknown provider '{id}'; known: {}",
            providers()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// Which providers a harness can be pointed at through Evyro, and how.
/// "documented" support: from each tool's docs, not yet run with real keys.
pub fn harness_providers(harness: &str) -> &'static [&'static str] {
    match harness {
        "claude-code" => &["anthropic", "bedrock", "vertex", "anthropic-compatible"],
        "codex" => &["openai", "ollama"],
        "gemini" => &["google", "vertex"],
        "cursor" => &["cursor"],
        "copilot" => &[],
        "opencode" | "aider" => &[
            "anthropic",
            "openai",
            "google",
            "openrouter",
            "groq",
            "mistral",
            "deepseek",
            "xai",
            "bedrock",
            "vertex",
            "ollama",
            "lmstudio",
            "mlx",
            "llamacpp",
            "vllm",
            "openai-compatible",
        ],
        "goose" => &[
            "anthropic",
            "openai",
            "google",
            "openrouter",
            "groq",
            "mistral",
            "deepseek",
            "xai",
            "bedrock",
            "vertex",
            "ollama",
            "openai-compatible",
        ],
        _ => &[],
    }
}

/// Which harnesses support signing in with an app/subscription account.
pub fn supports_app_login(harness: &str) -> bool {
    matches!(
        harness,
        "claude-code"
            | "codex"
            | "gemini"
            | "antigravity-cli"
            | "cursor"
            | "copilot"
            | "goose"
            | "amp"
            | "qwen"
            | "kiro"
            | "droid"
            | "cline"
            | "crush"
    )
}

// ---------------------------------------------------------------------------
// IDEs and chat apps

#[derive(Debug, Clone, Serialize)]
pub struct Ide {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Command-line launcher that opens a folder (`cursor .`).
    pub cli: &'static [&'static str],
    /// macOS application name for `open -a`.
    pub mac_app: &'static str,
    pub instruction_file: &'static str,
    pub mcp_setup: &'static str,
}

pub fn ides() -> Vec<Ide> {
    vec![
        Ide { id: "cursor-ide", display_name: "Cursor", cli: &["cursor"], mac_app: "Cursor", instruction_file: "AGENTS.md",
              mcp_setup: "evyro integrate cursor-ide --write  (adds .cursor/mcp.json)" },
        Ide { id: "vscode", display_name: "VS Code (Copilot, Cline, Continue, Roo)", cli: &["code"], mac_app: "Visual Studio Code", instruction_file: "AGENTS.md",
              mcp_setup: "evyro integrate vscode --write  (adds .vscode/mcp.json)" },
        Ide { id: "windsurf", display_name: "Windsurf", cli: &["windsurf"], mac_app: "Windsurf", instruction_file: "AGENTS.md",
              mcp_setup: "Windsurf Settings > MCP: add command `evyro` with args `mcp`" },
        Ide { id: "zed", display_name: "Zed", cli: &["zed"], mac_app: "Zed", instruction_file: "AGENTS.md",
              mcp_setup: "Zed settings.json > context_servers: {\"evyro\": {\"command\": {\"path\": \"evyro\", \"args\": [\"mcp\"]}}}" },
        Ide { id: "antigravity", display_name: "Google Antigravity", cli: &["antigravity", "agy"], mac_app: "Antigravity", instruction_file: "AGENTS.md",
              mcp_setup: "Antigravity settings > MCP servers: command `evyro`, args `mcp`" },
        Ide { id: "jetbrains", display_name: "JetBrains IDEs (AI Assistant, Junie)", cli: &["idea", "pycharm", "webstorm", "goland", "rustrover"], mac_app: "IntelliJ IDEA", instruction_file: "AGENTS.md",
              mcp_setup: "Settings > Tools > AI Assistant > MCP: command `evyro`, args `mcp`" },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatApp {
    pub id: &'static str,
    pub display_name: &'static str,
    pub url: &'static str,
    pub mac_app: &'static str,
    /// The desktop app can use Evyro's MCP server directly.
    pub mcp_setup: Option<&'static str>,
}

pub fn chat_apps() -> Vec<ChatApp> {
    vec![
        ChatApp { id: "chatgpt", display_name: "ChatGPT", url: "https://chatgpt.com/", mac_app: "ChatGPT", mcp_setup: None },
        ChatApp { id: "claude-app", display_name: "Claude (app or claude.ai)", url: "https://claude.ai/new", mac_app: "Claude",
                  mcp_setup: Some("Claude desktop > Settings > Developer > Edit Config: add {\"mcpServers\": {\"evyro\": {\"command\": \"evyro\", \"args\": [\"mcp\", \"--project\", \"<project-id>\"]}}}") },
        ChatApp { id: "gemini-app", display_name: "Gemini", url: "https://gemini.google.com/app", mac_app: "", mcp_setup: None },
        ChatApp { id: "copilot-chat", display_name: "Microsoft Copilot", url: "https://copilot.microsoft.com/", mac_app: "Copilot", mcp_setup: None },
    ]
}

// ---------------------------------------------------------------------------
// Targets

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Harness,
    Ide,
    Chat,
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    #[default]
    Auto,
    AppLogin,
    ApiKey,
    Local,
}

impl AuthMode {
    pub fn label(&self) -> &'static str {
        match self {
            AuthMode::Auto => "tool's own setup",
            AuthMode::AppLogin => "signed-in app account",
            AuthMode::ApiKey => "API key",
            AuthMode::Local => "local model",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub name: String,
    pub kind: TargetKind,
    /// Harness id for `harness` and `api` targets (`api` defaults to opencode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    /// IDE id for `ide` targets, chat app id for `chat` targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default)]
    pub auth: AuthMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Name of the environment variable that holds the key (not the key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// macOS Keychain service name holding the key (`security add-generic-password -s <name> -a evyro -w`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_keychain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing)]
    pub builtin: bool,
}

impl Target {
    pub fn display(&self) -> String {
        let what = match self.kind {
            TargetKind::Harness | TargetKind::Api => {
                let h = self.effective_harness().unwrap_or_default();
                adapters::get(&h)
                    .map(|a| a.display_name.to_string())
                    .unwrap_or(h)
            }
            TargetKind::Ide => self
                .app
                .as_deref()
                .and_then(|a| ides().into_iter().find(|i| i.id == a))
                .map(|i| i.display_name.to_string())
                .unwrap_or_default(),
            TargetKind::Chat => self
                .app
                .as_deref()
                .and_then(|a| chat_apps().into_iter().find(|i| i.id == a))
                .map(|i| i.display_name.to_string())
                .unwrap_or_default(),
        };
        match (&self.provider, &self.model) {
            (Some(p), Some(m)) => format!("{what} · {p}/{m}"),
            (None, Some(m)) => format!("{what} · {m}"),
            (Some(p), None) => format!("{what} · {p}"),
            _ => what,
        }
    }

    /// The harness that does the work for harness and api targets.
    pub fn effective_harness(&self) -> Option<String> {
        match self.kind {
            TargetKind::Harness => self.harness.clone(),
            TargetKind::Api => Some(self.harness.clone().unwrap_or_else(|| "opencode".into())),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TargetsFile {
    #[serde(default)]
    target: Vec<Target>,
}

pub fn targets_path(paths: &GlobalPaths) -> PathBuf {
    paths.config_dir.join("targets.toml")
}

/// Built-in targets: every known harness (using its own setup), every IDE and chat app.
pub fn builtin_targets() -> Vec<Target> {
    let mut v = Vec::new();
    for a in adapters::registry() {
        if a.binaries.is_empty() {
            continue; // IDE-only entries are covered by `ides()`
        }
        v.push(Target {
            name: a.id.into(),
            kind: TargetKind::Harness,
            harness: Some(a.id.into()),
            app: None,
            auth: AuthMode::Auto,
            provider: None,
            model: None,
            key_env: None,
            key_keychain: None,
            base_url: None,
            description: format!(
                "{} with whatever account or key it is set up with",
                a.display_name
            ),
            builtin: true,
        });
    }
    for i in ides() {
        v.push(Target {
            name: i.id.into(),
            kind: TargetKind::Ide,
            harness: None,
            app: Some(i.id.into()),
            auth: AuthMode::Auto,
            provider: None,
            model: None,
            key_env: None,
            key_keychain: None,
            base_url: None,
            description: format!(
                "Open the repository in {} with the resume prompt on the clipboard",
                i.display_name
            ),
            builtin: true,
        });
    }
    for c in chat_apps() {
        v.push(Target {
            name: c.id.into(),
            kind: TargetKind::Chat,
            harness: None,
            app: Some(c.id.into()),
            auth: AuthMode::AppLogin,
            provider: None,
            model: None,
            key_env: None,
            key_keychain: None,
            base_url: None,
            description: format!("Copy a self-contained chat pack for {}", c.display_name),
            builtin: true,
        });
    }
    v
}

fn read_user_targets(paths: &GlobalPaths) -> Result<Vec<Target>> {
    let p = targets_path(paths);
    match std::fs::read_to_string(&p) {
        Ok(s) => {
            let f: TargetsFile =
                toml::from_str(&s).map_err(|e| Error::Invalid(format!("{}: {e}", p.display())))?;
            Ok(f.target)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

fn write_user_targets(paths: &GlobalPaths, ts: &[Target]) -> Result<()> {
    let body = toml::to_string_pretty(&TargetsFile {
        target: ts.to_vec(),
    })
    .map_err(|e| Error::Other(e.to_string()))?;
    let header = "# Evyro switch targets. No secrets here: keys are referenced by environment\n# variable name (key_env) or macOS Keychain service (key_keychain).\n\n";
    crate::util::write_atomic(&targets_path(paths), format!("{header}{body}").as_bytes())?;
    Ok(())
}

/// All targets: built-ins, with your own targets added (same name overrides).
pub fn list_targets(paths: &GlobalPaths) -> Result<Vec<Target>> {
    let mut all = builtin_targets();
    for t in read_user_targets(paths)? {
        all.retain(|b| b.name != t.name);
        all.push(t);
    }
    Ok(all)
}

/// Resolve a target by name. Harness aliases (`claude`) resolve to their target.
pub fn resolve_target(paths: &GlobalPaths, name: &str) -> Result<Target> {
    let all = list_targets(paths)?;
    let norm = adapters::normalize_id(name);
    // A user target named exactly wins; then the normalized harness id.
    all.iter()
        .find(|t| t.name == name.trim())
        .or_else(|| all.iter().find(|t| t.name == norm))
        .cloned()
        .ok_or_else(|| {
            Error::NotFound(format!(
                "no switch target '{name}'. See `evyro targets`, or add one with `evyro targets add`"
            ))
        })
}

pub fn validate_target(t: &Target) -> Result<()> {
    adapters::validate_harness_name(&t.name)?;
    let bad = |m: String| Err(Error::Invalid(m));
    match t.kind {
        TargetKind::Harness | TargetKind::Api => {
            let h = t.effective_harness().unwrap_or_default();
            let spec = adapters::get(&h)?;
            if spec.binaries.is_empty() {
                return bad(format!(
                    "{} has no CLI; use an `ide` target",
                    spec.display_name
                ));
            }
            if t.kind == TargetKind::Api && t.provider.is_none() {
                return bad("an api target needs --provider".into());
            }
            if let Some(p) = &t.provider {
                if spec.id != "cursor" {
                    provider(p)?;
                }
                if !harness_providers(spec.id).contains(&p.as_str()) {
                    return bad(format!(
                        "{} cannot use provider '{p}' through Evyro; it supports: {}",
                        spec.display_name,
                        if harness_providers(spec.id).is_empty() {
                            "its own sign-in only".into()
                        } else {
                            harness_providers(spec.id).join(", ")
                        }
                    ));
                }
            }
            if t.auth == AuthMode::AppLogin && !supports_app_login(spec.id) {
                return bad(format!(
                    "{} has no app sign-in; use an API key or local model",
                    spec.display_name
                ));
            }
            if t.auth == AuthMode::ApiKey && t.key_env.is_none() && t.key_keychain.is_none() {
                let conventional = t
                    .provider
                    .as_deref()
                    .and_then(|p| provider(p).ok())
                    .and_then(|p| p.key_env);
                if conventional.is_none() && spec.id != "cursor" {
                    return bad(
                        "api_key auth needs --key-env <VAR> or --key-keychain <service>".into(),
                    );
                }
            }
            if let Some(m) = &t.model {
                if m.trim().is_empty() || m.chars().any(|c| c.is_control()) {
                    return bad("invalid model id".into());
                }
            }
        }
        TargetKind::Ide => {
            let a = t.app.as_deref().unwrap_or("");
            if !ides().iter().any(|i| i.id == a) {
                return bad(format!(
                    "unknown IDE '{a}'; known: {}",
                    ides().iter().map(|i| i.id).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        TargetKind::Chat => {
            let a = t.app.as_deref().unwrap_or("");
            if !chat_apps().iter().any(|i| i.id == a) {
                return bad(format!(
                    "unknown chat app '{a}'; known: {}",
                    chat_apps()
                        .iter()
                        .map(|i| i.id)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    if let Some(k) = &t.key_env {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return bad(format!(
                "key_env must be an environment variable name, not a key: '{}'",
                crate::redact::redact(k).0
            ));
        }
        if k.len() > 40 && k.chars().any(|c| c.is_ascii_lowercase()) {
            return bad("key_env looks like a key; give the variable name instead".into());
        }
    }
    if let Some(u) = &t.base_url {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return bad("base_url must start with http:// or https://".into());
        }
    }
    Ok(())
}

/// Fill in the sign-in mode when the person left it on `auto` but the rest of
/// the target says what they mean: a key source means an API key, a local
/// provider means a local model, and an `api` target always uses a key.
pub fn infer_auth(t: &mut Target) {
    if t.auth != AuthMode::Auto || !matches!(t.kind, TargetKind::Api | TargetKind::Harness) {
        return;
    }
    let local = t
        .provider
        .as_deref()
        .and_then(|p| provider(p).ok())
        .is_some_and(|p| p.local);
    if local {
        t.auth = AuthMode::Local;
    } else if t.key_env.is_some() || t.key_keychain.is_some() || t.kind == TargetKind::Api {
        t.auth = AuthMode::ApiKey;
    }
}

pub fn add_target(paths: &GlobalPaths, t: Target) -> Result<Target> {
    let mut t = t;
    infer_auth(&mut t);
    validate_target(&t)?;
    let mut ts = read_user_targets(paths)?;
    ts.retain(|x| x.name != t.name);
    t.builtin = false;
    ts.push(t.clone());
    write_user_targets(paths, &ts)?;
    Ok(t)
}

pub fn remove_target(paths: &GlobalPaths, name: &str) -> Result<()> {
    let mut ts = read_user_targets(paths)?;
    let before = ts.len();
    ts.retain(|x| x.name != name);
    if ts.len() == before {
        return Err(Error::NotFound(format!(
            "no user target '{name}' (built-in targets cannot be removed)"
        )));
    }
    write_user_targets(paths, &ts)
}

// ---------------------------------------------------------------------------
// Readiness checks. These look for evidence only: they never read key values
// or credential file contents.

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    /// ok | missing | unknown
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetStatus {
    pub target: Target,
    pub display: String,
    /// ready | needs_setup | unknown
    pub state: String,
    pub checks: Vec<Check>,
    pub next_step: Option<String>,
}

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

fn ck(name: &str, state: &str, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        state: state.into(),
        detail: detail.into(),
    }
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn keychain_has(service: &str) -> Option<bool> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    // Without -w, `security` prints item attributes, never the secret.
    std::process::Command::new("security")
        .args(["find-generic-password", "-s", service])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|s| s.success())
}

/// Evidence that a harness has a signed-in app account. File existence only.
pub(crate) fn login_evidence(harness: &str) -> Check {
    let Some(h) = home() else {
        return ck("app sign-in", "unknown", "no home directory");
    };
    let files: &[&str] = match harness {
        "claude-code" => &[".claude/.credentials.json"],
        "codex" => &[".codex/auth.json"],
        "gemini" => &[".gemini/oauth_creds.json"],
        "copilot" => &[".copilot/config.json"],
        "cursor" => &[".cursor/cli-config.json"],
        "goose" => &[".config/goose/config.yaml"],
        "qwen" => &[".qwen/oauth_creds.json", ".qwen/settings.json"],
        "amp" => &[".local/share/amp/secrets.json", ".config/amp/settings.json"],
        "kiro" => &[".kiro/settings/cli.json", ".aws/sso/cache"],
        "droid" => &[".factory/auth.json", ".factory/settings.json"],
        "cline" => &[".cline/data/settings/providers.json", ".cline/data"],
        "crush" => &[
            ".local/share/crush/providers.json",
            ".config/crush/crush.json",
        ],
        _ => &[],
    };
    if let Some(f) = files.iter().find(|f| h.join(f).exists()) {
        // A login file says someone signed in once, not that the account can
        // run now (expired plan, usage limit, a forced account migration).
        return ck(
            "app sign-in",
            "ok",
            format!("found ~/{f}; the account itself is checked only when the tool runs"),
        );
    }
    if harness == "claude-code" {
        if let Some(true) = keychain_has("Claude Code-credentials") {
            return ck(
                "app sign-in",
                "ok",
                "found Claude Code login in the macOS Keychain",
            );
        }
    }
    if files.is_empty() {
        return ck(
            "app sign-in",
            "unknown",
            "Evyro cannot tell whether this tool is signed in",
        );
    }
    ck(
        "app sign-in",
        "missing",
        format!(
            "no login found; run the tool once and sign in (looked for ~/{})",
            files.join(", ~/")
        ),
    )
}

fn key_check(t: &Target) -> Check {
    if let Some(s) = &t.key_keychain {
        return match keychain_has(s) {
            Some(true) => ck("API key", "ok", format!("Keychain item '{s}' exists")),
            Some(false) => ck("API key", "missing", format!("no Keychain item '{s}'. Add it: security add-generic-password -s {s} -a evyro -w")),
            None => ck("API key", "missing", "key_keychain works on macOS only; use key_env here"),
        };
    }
    let var = t.key_env.clone().or_else(|| {
        t.provider
            .as_deref()
            .and_then(|p| provider(p).ok())
            .and_then(|p| p.key_env.map(String::from))
    });
    match var {
        Some(v) if env_set(&v) => ck("API key", "ok", format!("${v} is set")),
        Some(v) => ck(
            "API key",
            "missing",
            format!("${v} is not set in this environment"),
        ),
        None => ck(
            "API key",
            "unknown",
            "no key variable known for this provider",
        ),
    }
}

pub fn check_target(t: &Target) -> TargetStatus {
    let mut checks = Vec::new();
    let mut next = None;
    match t.kind {
        TargetKind::Harness | TargetKind::Api => {
            let h = t.effective_harness().unwrap_or_default();
            match adapters::get(&h) {
                Ok(spec) => {
                    let d = adapters::detect(&spec);
                    if d.installed {
                        checks.push(ck(
                            "installed",
                            if d.compatibility == "incompatible" {
                                "missing"
                            } else {
                                "ok"
                            },
                            format!(
                                "{} {}",
                                d.binary.clone().unwrap_or_default(),
                                d.version.clone().unwrap_or_default()
                            ),
                        ));
                    } else {
                        checks.push(ck(
                            "installed",
                            "missing",
                            d.diagnostic.clone().unwrap_or_default(),
                        ));
                        next = Some(format!(
                            "Install {} and make sure `{}` is on PATH",
                            spec.display_name,
                            spec.binaries.join("` or `")
                        ));
                    }
                    match t.auth {
                        AuthMode::AppLogin => checks.push(login_evidence(spec.id)),
                        AuthMode::ApiKey => checks.push(key_check(t)),
                        AuthMode::Local => {
                            let p = t.provider.as_deref().unwrap_or("ollama");
                            let bin = match p {
                                "lmstudio" => "lms",
                                "mlx" => "mlx_lm.server",
                                "llamacpp" => "llama-server",
                                "vllm" => "vllm",
                                _ => "ollama",
                            };
                            checks.push(match adapters::which(bin) {
                                Some(b) => ck(
                                    "local server",
                                    "ok",
                                    format!("{} found (make sure it is running)", b.display()),
                                ),
                                None => ck(
                                    "local server",
                                    "unknown",
                                    format!(
                                        "`{bin}` not on PATH; fine if the server runs elsewhere"
                                    ),
                                ),
                            });
                        }
                        AuthMode::Auto => {
                            let mut c = login_evidence(spec.id);
                            if c.state != "ok" {
                                let keys: Vec<&str> = harness_providers(spec.id)
                                    .iter()
                                    .filter_map(|p| provider(p).ok().and_then(|p| p.key_env))
                                    .filter(|k| env_set(k))
                                    .collect();
                                if !keys.is_empty() {
                                    c = ck(
                                        "account",
                                        "ok",
                                        format!("API key in environment: ${}", keys.join(", $")),
                                    );
                                } else if c.state == "missing" {
                                    c.state = "unknown".into();
                                    c.detail =
                                        format!("{}; it may be configured another way", c.detail);
                                }
                            }
                            checks.push(c);
                        }
                    }
                }
                Err(e) => checks.push(ck("harness", "missing", e.to_string())),
            }
        }
        TargetKind::Ide => {
            let ide = ides().into_iter().find(|i| Some(i.id) == t.app.as_deref());
            if let Some(i) = ide {
                let cli = i.cli.iter().find_map(|c| adapters::which(c));
                let app = mac_app_installed(i.mac_app);
                match (cli, app) {
                    (Some(c), _) => {
                        checks.push(ck("installed", "ok", format!("launcher {}", c.display())))
                    }
                    (None, Some(true)) => {
                        checks.push(ck("installed", "ok", format!("{}.app", i.mac_app)))
                    }
                    _ => {
                        checks.push(ck(
                            "installed",
                            "missing",
                            format!(
                                "no `{}` launcher on PATH{}",
                                i.cli.join("`/`"),
                                if cfg!(target_os = "macos") {
                                    " and no app in /Applications"
                                } else {
                                    ""
                                }
                            ),
                        ));
                        next = Some(format!("Install {} (or its shell command)", i.display_name));
                    }
                }
                checks.push(ck("Evyro MCP (optional)", "unknown", i.mcp_setup));
            }
        }
        TargetKind::Chat => {
            checks.push(ck(
                "app sign-in",
                "unknown",
                "Sign in to the app or website yourself; Evyro only prepares the chat pack",
            ));
            if let Some(c) = chat_apps()
                .into_iter()
                .find(|c| Some(c.id) == t.app.as_deref())
            {
                if let Some(m) = c.mcp_setup {
                    checks.push(ck("Evyro MCP (optional)", "unknown", m));
                }
            }
        }
    }
    let state = if checks.iter().any(|c| c.state == "missing") {
        "needs_setup"
    } else if checks
        .iter()
        .all(|c| c.state == "ok" || c.name.ends_with("(optional)"))
    {
        "ready"
    } else {
        "unknown"
    };
    if next.is_none() {
        next = checks
            .iter()
            .find(|c| c.state == "missing")
            .map(|c| c.detail.clone());
    }
    TargetStatus {
        display: t.display(),
        target: t.clone(),
        state: state.into(),
        checks,
        next_step: next,
    }
}

fn mac_app_installed(name: &str) -> Option<bool> {
    // EVYRO_NO_APP_LOOKUP keeps tests from finding (and opening) real apps.
    if !cfg!(target_os = "macos")
        || name.is_empty()
        || std::env::var_os("EVYRO_NO_APP_LOOKUP").is_some()
    {
        return None;
    }
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(h) = home() {
        dirs.push(h.join("Applications"));
    }
    Some(dirs.iter().any(|d| d.join(format!("{name}.app")).exists()))
}

// ---------------------------------------------------------------------------
// Launch plans for harness and api targets.

/// How to start a harness for a target: model argument, extra args and the
/// child environment changes. Key values only exist inside this struct.
#[derive(Debug, Default)]
pub struct LaunchPlan {
    pub harness: String,
    pub model: Option<String>,
    pub extra_args: Vec<String>,
    pub env_set: Vec<(String, String)>,
    pub env_remove: Vec<String>,
}

const KEY_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "CURSOR_API_KEY",
];

fn read_key(t: &Target, conventional: Option<&str>) -> Result<Option<String>> {
    if let Some(s) = &t.key_keychain {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported(
                "key_keychain is macOS-only; use key_env".into(),
            ));
        }
        let out = std::process::Command::new("security")
            .args(["find-generic-password", "-s", s, "-w"])
            .stderr(std::process::Stdio::null())
            .output()?;
        if !out.status.success() {
            return Err(Error::NotFound(format!("Keychain item '{s}' not found")));
        }
        return Ok(Some(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ));
    }
    let var = t.key_env.as_deref().or(conventional);
    match var {
        Some(v) => match std::env::var(v) {
            Ok(k) if !k.is_empty() => Ok(Some(k)),
            _ => Err(Error::NotFound(format!(
                "${v} is not set; export it or use key_keychain"
            ))),
        },
        None => Ok(None),
    }
}

pub fn launch_plan(t: &Target) -> Result<LaunchPlan> {
    validate_target(t)?;
    let harness = t
        .effective_harness()
        .ok_or_else(|| Error::Unsupported(format!("{} is not launched as a CLI", t.name)))?;
    let mut plan = LaunchPlan {
        harness: harness.clone(),
        model: t.model.clone(),
        ..Default::default()
    };
    if t.auth == AuthMode::AppLogin {
        // Make the tool use the signed-in account, not a stray key.
        plan.env_remove = KEY_VARS.iter().map(|s| s.to_string()).collect();
        return Ok(plan);
    }
    let prov = t.provider.as_deref().map(provider).transpose();
    let prov = if harness == "cursor" { None } else { prov? };
    let conventional = prov.as_ref().and_then(|p| p.key_env);
    let key = if t.auth == AuthMode::ApiKey
        || (t.auth == AuthMode::Auto && (t.key_env.is_some() || t.key_keychain.is_some()))
    {
        read_key(t, conventional)?
    } else {
        None
    };
    let base = t.base_url.clone().or_else(|| {
        prov.as_ref()
            .and_then(|p| p.default_base_url.map(String::from))
    });
    let pid = prov.as_ref().map(|p| p.id).unwrap_or("");
    let model = t.model.clone();
    let set = |plan: &mut LaunchPlan, k: &str, v: String| plan.env_set.push((k.into(), v));

    match harness.as_str() {
        "claude-code" => match pid {
            "bedrock" => set(&mut plan, "CLAUDE_CODE_USE_BEDROCK", "1".into()),
            "vertex" => set(&mut plan, "CLAUDE_CODE_USE_VERTEX", "1".into()),
            "anthropic-compatible" => {
                let b = base.clone().ok_or_else(|| {
                    Error::Invalid("anthropic-compatible needs --base-url".into())
                })?;
                set(&mut plan, "ANTHROPIC_BASE_URL", b);
                if let Some(k) = &key {
                    set(&mut plan, "ANTHROPIC_AUTH_TOKEN", k.clone());
                }
            }
            _ => {
                if let Some(k) = &key {
                    set(&mut plan, "ANTHROPIC_API_KEY", k.clone());
                }
            }
        },
        "codex" => {
            if pid == "ollama" {
                plan.extra_args.push("--oss".into());
            } else if let Some(k) = &key {
                set(&mut plan, "OPENAI_API_KEY", k.clone());
            }
        }
        "gemini" => {
            if pid == "vertex" {
                set(&mut plan, "GOOGLE_GENAI_USE_VERTEXAI", "true".into());
            } else if let Some(k) = &key {
                set(&mut plan, "GEMINI_API_KEY", k.clone());
            }
        }
        "cursor" => {
            if let Some(k) = &key {
                set(&mut plan, "CURSOR_API_KEY", k.clone());
            }
        }
        "opencode" => {
            if let Some(m) = &model {
                plan.model = Some(if m.contains('/') && !matches!(pid, "openrouter") {
                    m.clone()
                } else {
                    format!("{}/{m}", opencode_provider(pid))
                });
            }
            put_key(&mut plan, pid, key, base, true);
        }
        "aider" => {
            if let Some(m) = &model {
                plan.model = Some(format!("{}/{m}", aider_prefix(pid)));
            }
            put_key(&mut plan, pid, key, base, false);
        }
        "goose" => {
            set(&mut plan, "GOOSE_PROVIDER", goose_provider(pid).into());
            if let Some(m) = &model {
                set(&mut plan, "GOOSE_MODEL", m.clone());
                plan.model = None; // goose takes the model from the environment
            }
            put_key(&mut plan, pid, key, base, false);
        }
        _ => {}
    }
    Ok(plan)
}

fn put_key(
    plan: &mut LaunchPlan,
    pid: &str,
    key: Option<String>,
    base: Option<String>,
    opencode: bool,
) {
    let var = match pid {
        p if openai_style(p) => "OPENAI_API_KEY",
        p => provider(p).ok().and_then(|p| p.key_env).unwrap_or(""),
    };
    if let (Some(k), false) = (key, var.is_empty()) {
        plan.env_set.push((var.into(), k));
    }
    match pid {
        "ollama" => {
            if let Some(b) = base {
                plan.env_set.push(("OLLAMA_HOST".into(), b.clone()));
                plan.env_set.push(("OLLAMA_API_BASE".into(), b));
            }
        }
        p if openai_style(p) => {
            if let Some(b) = base {
                plan.env_set.push((
                    (if opencode {
                        "OPENAI_BASE_URL"
                    } else {
                        "OPENAI_API_BASE"
                    })
                    .into(),
                    b.clone(),
                ));
                plan.env_set.push(("OPENAI_BASE_URL".into(), b));
            }
            // Local servers take any key, but OpenAI clients refuse to start without one.
            if pid != "openai-compatible"
                && !plan.env_set.iter().any(|(k, _)| k == "OPENAI_API_KEY")
            {
                let k = if pid == "lmstudio" {
                    "lm-studio"
                } else {
                    "local"
                };
                plan.env_set.push(("OPENAI_API_KEY".into(), k.into()));
            }
        }
        _ => {}
    }
    plan.env_set.dedup_by(|a, b| a.0 == b.0);
}

/// Providers spoken to through the OpenAI API shape: any compatible endpoint
/// and the local servers (LM Studio, MLX, llama.cpp, vLLM).
fn openai_style(p: &str) -> bool {
    matches!(
        p,
        "openai-compatible" | "lmstudio" | "mlx" | "llamacpp" | "vllm"
    )
}

fn opencode_provider(p: &str) -> &str {
    match p {
        p if openai_style(p) => "openai",
        "bedrock" => "amazon-bedrock",
        "vertex" => "google-vertex",
        other => other,
    }
}

fn aider_prefix(p: &str) -> &str {
    match p {
        "google" => "gemini",
        "ollama" => "ollama_chat",
        p if openai_style(p) => "openai",
        "vertex" => "vertex_ai",
        other => other,
    }
}

fn goose_provider(p: &str) -> &str {
    match p {
        "google" => "google",
        "vertex" => "gcp_vertex_ai",
        "bedrock" => "aws_bedrock",
        "openai-compatible" => "openai",
        "" => "",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// IDE and chat handover helpers.

/// argv that opens `dir` in an IDE: the CLI launcher if present, else `open -a` on macOS.
pub fn ide_open_argv(ide_id: &str, dir: &Path) -> Result<Vec<String>> {
    let i = ides()
        .into_iter()
        .find(|i| i.id == ide_id)
        .ok_or_else(|| Error::NotFound(format!("unknown IDE '{ide_id}'")))?;
    let d = dir.to_string_lossy().to_string();
    if let Some(c) = i.cli.iter().find_map(|c| adapters::which(c)) {
        return Ok(vec![c.to_string_lossy().to_string(), d]);
    }
    if mac_app_installed(i.mac_app) == Some(true) {
        return Ok(vec!["open".into(), "-a".into(), i.mac_app.into(), d]);
    }
    Err(Error::Unsupported(format!(
        "{} is not installed (no `{}` launcher on PATH)",
        i.display_name,
        i.cli.join("`/`")
    )))
}

/// argv that opens a chat app or its website.
pub fn chat_open_argv(chat_id: &str) -> Result<Vec<String>> {
    let c = chat_apps()
        .into_iter()
        .find(|c| c.id == chat_id)
        .ok_or_else(|| Error::NotFound(format!("unknown chat app '{chat_id}'")))?;
    if cfg!(target_os = "macos") {
        if mac_app_installed(c.mac_app) == Some(true) {
            return Ok(vec!["open".into(), "-a".into(), c.mac_app.into()]);
        }
        return Ok(vec!["open".into(), c.url.into()]);
    }
    if cfg!(windows) {
        return Ok(vec![
            "cmd".into(),
            "/C".into(),
            "start".into(),
            String::new(),
            c.url.into(),
        ]);
    }
    Ok(vec!["xdg-open".into(), c.url.into()])
}

/// Copy text to the system clipboard. Returns false when no clipboard tool exists.
pub fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    let cands: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(windows) {
        &[&["clip"]]
    } else {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "--clipboard", "--input"],
        ]
    };
    for c in cands {
        if adapters::which(c[0]).is_none() {
            continue;
        }
        if let Ok(mut child) = std::process::Command::new(c[0])
            .args(&c[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut s) = child.stdin.take() {
                let _ = s.write_all(text.as_bytes());
            }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(kind: TargetKind) -> Target {
        Target {
            name: "x".into(),
            kind,
            harness: None,
            app: None,
            auth: AuthMode::Auto,
            provider: None,
            model: None,
            key_env: None,
            key_keychain: None,
            base_url: None,
            description: String::new(),
            builtin: false,
        }
    }

    #[test]
    fn builtins_cover_every_kind() {
        let b = builtin_targets();
        for k in [TargetKind::Harness, TargetKind::Ide, TargetKind::Chat] {
            assert!(b.iter().any(|t| t.kind == k));
        }
        for t in &b {
            validate_target(t).unwrap_or_else(|e| panic!("{}: {e}", t.name));
        }
    }

    #[test]
    fn api_target_maps_model_and_key() {
        let mut a = t(TargetKind::Api);
        a.provider = Some("openrouter".into());
        a.model = Some("moonshotai/kimi-k2".into());
        a.auth = AuthMode::ApiKey;
        a.key_env = Some("EVYRO_TEST_OR_KEY".into());
        std::env::set_var("EVYRO_TEST_OR_KEY", "sk-or-test");
        let p = launch_plan(&a).unwrap();
        assert_eq!(p.harness, "opencode");
        assert_eq!(p.model.as_deref(), Some("openrouter/moonshotai/kimi-k2"));
        assert!(p
            .env_set
            .contains(&("OPENROUTER_API_KEY".into(), "sk-or-test".into())));

        a.harness = Some("aider".into());
        let p = launch_plan(&a).unwrap();
        assert_eq!(p.model.as_deref(), Some("openrouter/moonshotai/kimi-k2"));
    }

    #[test]
    fn app_login_strips_keys() {
        let mut a = t(TargetKind::Harness);
        a.harness = Some("claude-code".into());
        a.auth = AuthMode::AppLogin;
        let p = launch_plan(&a).unwrap();
        assert!(p.env_remove.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(p.env_set.is_empty());
    }

    #[test]
    fn incompatible_combinations_are_rejected() {
        let mut a = t(TargetKind::Harness);
        a.harness = Some("codex".into());
        a.provider = Some("anthropic".into());
        assert!(validate_target(&a).is_err());
        let mut b = t(TargetKind::Harness);
        b.harness = Some("aider".into());
        b.auth = AuthMode::AppLogin;
        assert!(validate_target(&b).is_err());
        let mut c = t(TargetKind::Api);
        c.provider = Some("openai".into());
        c.auth = AuthMode::ApiKey;
        c.key_env = Some("sk-proj-abcdefghijklmnopqrstuvwxyz0123456789abcdef".into());
        assert!(validate_target(&c).is_err(), "a pasted key is refused");
    }

    #[test]
    fn local_model_needs_no_key() {
        let mut a = t(TargetKind::Api);
        a.harness = Some("aider".into());
        a.provider = Some("ollama".into());
        a.model = Some("qwen2.5-coder:14b".into());
        a.auth = AuthMode::Local;
        let p = launch_plan(&a).unwrap();
        assert_eq!(p.model.as_deref(), Some("ollama_chat/qwen2.5-coder:14b"));
        assert!(p.env_set.iter().any(|(k, _)| k == "OLLAMA_API_BASE"));
    }

    #[test]
    fn mlx_llamacpp_and_vllm_are_local_openai_servers() {
        for (pid, port) in [("mlx", "8080"), ("llamacpp", "8080"), ("vllm", "8000")] {
            let mut a = t(TargetKind::Api);
            a.harness = Some("opencode".into());
            a.provider = Some(pid.into());
            a.model = Some("my-model".into());
            a.auth = AuthMode::Local;
            let p = launch_plan(&a).unwrap();
            assert_eq!(p.model.as_deref(), Some("openai/my-model"), "{pid}");
            let get = |k: &str| {
                p.env_set
                    .iter()
                    .find(|(x, _)| x == k)
                    .map(|(_, v)| v.clone())
            };
            assert!(get("OPENAI_BASE_URL").unwrap().contains(port), "{pid}");
            // OpenAI clients need some key even for a local server.
            assert_eq!(get("OPENAI_API_KEY").as_deref(), Some("local"), "{pid}");
        }
    }
}
