//! Custom agents: a harness Evyro does not know, described by one TOML file.
//!
//! Files are read from `<config dir>/agents/*.toml` (this user) and
//! `<project>/.evyro/agents/*.toml` (this project). Each file names a command,
//! argv templates and environment variables. No shell is involved: every
//! template element becomes exactly one argument, and a placeholder's value is
//! inserted once, so a prompt containing `{cwd}` or `; rm -rf /` stays text.
//!
//! ```toml
//! name = "my-agent"
//! display_name = "My Agent"
//! command = "my-agent"
//! args = ["run", "--cwd", "{cwd}", "{prompt}"]
//! interactive_args = ["{prompt}"]
//! model_args = ["--model", "{model}"]
//! login_check = ["my-agent", "auth", "status"]
//!
//! [env]
//! MY_AGENT_TELEMETRY = "0"
//! ```

use crate::adapters::{self, Capabilities, Detection, LaunchMode};
use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Placeholders a template may use. `{model}` only makes sense in `model_args`.
const PLACEHOLDERS: &[&str] = &["prompt", "cwd", "model"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// From the user's config directory.
    User,
    /// From the project's `.evyro/agents/`. Repository text: Evyro does not run
    /// its login check on its own.
    Project,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentFile {
    name: String,
    display_name: Option<String>,
    command: String,
    args: Vec<String>,
    interactive_args: Option<Vec<String>>,
    model_args: Option<Vec<String>>,
    instruction_file: Option<String>,
    login_check: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomAgent {
    pub name: String,
    pub display_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub interactive_args: Option<Vec<String>>,
    pub model_args: Option<Vec<String>>,
    pub instruction_file: String,
    pub login_check: Option<Vec<String>>,
    pub env: BTreeMap<String, String>,
    pub source: PathBuf,
    pub scope: Scope,
}

/// A file that could not be used, and why.
#[derive(Debug, Clone, Serialize)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Loaded {
    pub agents: Vec<CustomAgent>,
    pub errors: Vec<LoadError>,
}

pub fn user_dir(paths: &GlobalPaths) -> PathBuf {
    paths.config_dir.join("agents")
}

pub fn project_dir(project_root: &Path) -> PathBuf {
    project_root.join(".evyro").join("agents")
}

/// Load every custom agent for this user and, if given, this project. A bad file
/// is reported in `errors` and skipped; it never hides the others.
pub fn load(paths: &GlobalPaths, project_root: Option<&Path>) -> Loaded {
    let mut out = Loaded::default();
    load_dir(&user_dir(paths), Scope::User, &mut out);
    if let Some(root) = project_root {
        load_dir(&project_dir(root), Scope::Project, &mut out);
    }
    out
}

fn load_dir(dir: &Path, scope: Scope, out: &mut Loaded) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return; // no folder: no custom agents
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "toml") && p.is_file())
        .collect();
    files.sort();
    for path in files {
        let parsed = std::fs::read_to_string(&path)
            .map_err(Error::from)
            .and_then(|text| parse(&text, &path, scope));
        match parsed {
            Ok(a) => {
                if let Some(prev) = out.agents.iter().find(|p| p.name == a.name) {
                    out.errors.push(LoadError {
                        message: format!(
                            "agent '{}' is already defined in {}",
                            a.name,
                            prev.source.display()
                        ),
                        path,
                    });
                } else {
                    out.agents.push(a);
                }
            }
            Err(e) => out.errors.push(LoadError {
                path,
                message: e.to_string(),
            }),
        }
    }
}

/// Parse and validate one agent file.
pub fn parse(text: &str, source: &Path, scope: Scope) -> Result<CustomAgent> {
    let f: AgentFile = toml::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let bad = |m: String| Err(Error::Invalid(m));

    adapters::validate_harness_name(&f.name)?;
    if adapters::registry().iter().any(|a| a.id == f.name)
        || adapters::normalize_id(&f.name) != f.name
    {
        return bad(format!(
            "'{}' is a built-in agent's name; pick another name",
            f.name
        ));
    }
    if f.command.trim().is_empty() || f.command.chars().any(char::is_control) {
        return bad("command must be a program name or path".into());
    }
    check_template("args", &f.args, false)?;
    if !f.args.iter().any(|a| a.contains("{prompt}")) {
        return bad("args must pass the prompt: add \"{prompt}\"".into());
    }
    if let Some(t) = &f.interactive_args {
        check_template("interactive_args", t, false)?;
    }
    if let Some(t) = &f.model_args {
        check_template("model_args", t, true)?;
        if !t.iter().any(|a| a.contains("{model}")) {
            return bad("model_args must pass the model: add \"{model}\"".into());
        }
    }
    if let Some(c) = &f.login_check {
        if c.first().map_or(true, |p| p.trim().is_empty()) {
            return bad(
                "login_check needs a command, e.g. [\"my-agent\", \"auth\", \"status\"]".into(),
            );
        }
        if c.iter().any(|a| a.contains('{')) {
            return bad("login_check cannot use placeholders".into());
        }
    }
    for (k, v) in &f.env {
        let valid = k
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return bad(format!("env: '{k}' is not a valid variable name"));
        }
        if v.contains('\0') {
            return bad(format!("env: {k} contains a NUL byte"));
        }
    }
    let instruction_file = f.instruction_file.unwrap_or_else(|| "AGENTS.md".into());
    if instruction_file.contains(['/', '\\']) || instruction_file.trim().is_empty() {
        return bad("instruction_file must be a file name in the repository root".into());
    }
    Ok(CustomAgent {
        display_name: f.display_name.unwrap_or_else(|| f.name.clone()),
        name: f.name,
        command: f.command,
        args: f.args,
        interactive_args: f.interactive_args,
        model_args: f.model_args,
        instruction_file,
        login_check: f.login_check,
        env: f.env,
        source: source.to_path_buf(),
        scope,
    })
}

/// Every `{word}` in a template must be a known placeholder.
fn check_template(field: &str, t: &[String], allow_model: bool) -> Result<()> {
    for arg in t {
        if arg.chars().any(|c| c == '\0') {
            return Err(Error::Invalid(format!("{field}: NUL byte in '{arg}'")));
        }
        for name in placeholders(arg) {
            if !PLACEHOLDERS.contains(&name) {
                return Err(Error::Invalid(format!(
                    "{field}: unknown placeholder {{{name}}}; use {}",
                    PLACEHOLDERS
                        .iter()
                        .map(|p| format!("{{{p}}}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            if name == "model" && !allow_model {
                return Err(Error::Invalid(format!(
                    "{field}: {{model}} belongs in model_args"
                )));
            }
        }
    }
    Ok(())
}

/// The `{word}` names in one template element.
fn placeholders(arg: &str) -> Vec<&str> {
    let mut v = Vec::new();
    let mut rest = arg;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        if let Some(j) = rest.find('}') {
            let name = &rest[..j];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                v.push(name);
                rest = &rest[j + 1..];
            }
        }
    }
    v
}

/// Replace placeholders in one element in a single pass, so a value that itself
/// contains `{...}` is never expanded again.
fn expand(arg: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    'outer: while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        for (k, v) in vars {
            if let Some(tail) = after.strip_prefix(k).and_then(|t| t.strip_prefix('}')) {
                out.push_str(v);
                rest = tail;
                continue 'outer;
            }
        }
        out.push('{');
        rest = after;
    }
    out.push_str(rest);
    out
}

impl CustomAgent {
    /// The program to run: an absolute path, or found on PATH.
    pub fn binary(&self) -> Option<PathBuf> {
        let p = Path::new(&self.command);
        if p.is_absolute() {
            p.is_file().then(|| p.to_path_buf())
        } else {
            adapters::which(&self.command)
        }
    }

    /// Build argv: `bin`, then the model arguments (if a model is given), then
    /// the mode's template. Interactive falls back to `args` when no
    /// `interactive_args` is set.
    pub fn launch_argv(
        &self,
        bin: &str,
        mode: LaunchMode,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
    ) -> Result<Vec<String>> {
        let template = match mode {
            LaunchMode::Headless => &self.args,
            LaunchMode::Interactive => self.interactive_args.as_ref().unwrap_or(&self.args),
        };
        let cwd = cwd.to_string_lossy();
        let mut argv = vec![bin.to_string()];
        if let Some(m) = model {
            let t = self.model_args.as_ref().ok_or_else(|| {
                Error::Unsupported(format!(
                    "{} has no model_args, so Evyro cannot pick a model for it",
                    self.display_name
                ))
            })?;
            argv.extend(t.iter().map(|a| expand(a, &[("model", m)])));
        }
        let vars = [("prompt", prompt), ("cwd", cwd.as_ref())];
        argv.extend(template.iter().map(|a| expand(a, &vars)));
        Ok(argv)
    }

    /// The same shape `evyro agents` shows for built-in agents. Nothing is run.
    pub fn detect(&self) -> Detection {
        let bin = self.binary();
        Detection {
            id: self.name.clone(),
            display_name: self.display_name.clone(),
            installed: bin.is_some(),
            binary: bin.as_ref().map(|p| p.to_string_lossy().into_owned()),
            version: None,
            compatibility: if bin.is_some() { "ok" } else { "not_installed" }.into(),
            diagnostic: if bin.is_some() {
                None
            } else {
                Some(format!("`{}` not found on PATH", self.command))
            },
            capabilities: Capabilities {
                install_detection: true,
                headless_launch: true,
                interactive_launch: true,
                instruction_file: true,
                model_selection: self.model_args.is_some(),
                ..Default::default()
            },
            evidence: format!("custom ({})", self.source.display()),
            instruction_file: self.instruction_file.clone(),
            mcp_setup: String::new(),
        }
    }

    /// Run `login_check` with the agent's environment. `None` when there is no
    /// check; `Some(Ok(()))` when it exits 0; otherwise the reason.
    pub fn check_login(&self, timeout: Duration) -> Option<std::result::Result<(), String>> {
        let argv = self.login_check.as_ref()?;
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return Some(Err(format!("could not run `{}`: {e}", argv[0]))),
        };
        let start = Instant::now();
        Some(loop {
            match child.try_wait() {
                Ok(Some(s)) if s.success() => break Ok(()),
                Ok(Some(s)) => break Err(format!("`{}` exited with {s}", argv.join(" "))),
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!(
                        "`{}` did not finish in {timeout:?}",
                        argv.join(" ")
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => break Err(e.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "my-agent"
display_name = "My Agent"
command = "my-agent"
args = ["run", "--cwd={cwd}", "{prompt}"]
interactive_args = ["{prompt}"]
model_args = ["--model", "{model}"]
login_check = ["my-agent", "auth", "status"]

[env]
MY_AGENT_TELEMETRY = "0"
"#;

    fn p(text: &str) -> Result<CustomAgent> {
        parse(text, Path::new("a.toml"), Scope::User)
    }

    fn err(text: &str) -> String {
        p(text).unwrap_err().to_string()
    }

    #[test]
    fn parses_a_full_file() {
        let a = p(GOOD).unwrap();
        assert_eq!(a.name, "my-agent");
        assert_eq!(a.display_name, "My Agent");
        assert_eq!(a.instruction_file, "AGENTS.md");
        assert_eq!(a.env["MY_AGENT_TELEMETRY"], "0");
        assert_eq!(a.login_check.as_deref().unwrap().len(), 3);
        let d = a.detect();
        assert!(d.capabilities.model_selection);
        assert!(d.evidence.starts_with("custom"));
    }

    #[test]
    fn minimal_file_defaults() {
        let a = p("name = \"x\"\ncommand = \"x\"\nargs = [\"{prompt}\"]").unwrap();
        assert_eq!(a.display_name, "x");
        assert!(a.env.is_empty() && a.login_check.is_none());
    }

    #[test]
    fn bad_files_say_what_is_wrong() {
        let base = "name = \"x\"\ncommand = \"x\"\n";
        for (text, want) in [
            (format!("{base}args = [\"run\"]"), "add \"{prompt}\""),
            (
                format!("{base}args = [\"{{prompt}}\", \"{{task}}\"]"),
                "unknown placeholder {task}",
            ),
            (
                format!("{base}args = [\"{{prompt}}\", \"{{model}}\"]"),
                "belongs in model_args",
            ),
            (
                format!("{base}args = [\"{{prompt}}\"]\nmodel_args = [\"-m\"]"),
                "add \"{model}\"",
            ),
            (
                format!("{base}args = [\"{{prompt}}\"]\nlogin_check = []"),
                "login_check needs a command",
            ),
            (
                format!("{base}args = [\"{{prompt}}\"]\n[env]\n\"BAD-NAME\" = \"1\""),
                "not a valid variable name",
            ),
            (
                format!("{base}args = [\"{{prompt}}\"]\nargz = []"),
                "unknown field",
            ),
            (
                format!("{base}args = [\"{{prompt}}\"]\ninstruction_file = \"../x.md\""),
                "instruction_file",
            ),
            (
                "command = \"x\"\nargs = [\"{prompt}\"]".into(),
                "missing field `name`",
            ),
            (
                "name = \"a;b\"\ncommand = \"x\"\nargs = [\"{prompt}\"]".into(),
                "invalid agent/harness name",
            ),
            (
                "name = \"codex\"\ncommand = \"x\"\nargs = [\"{prompt}\"]".into(),
                "built-in",
            ),
            (
                "name = \"claude\"\ncommand = \"x\"\nargs = [\"{prompt}\"]".into(),
                "built-in",
            ),
            (
                "name = \"x\"\ncommand = \" \"\nargs = [\"{prompt}\"]".into(),
                "command must be",
            ),
        ] {
            let e = err(&text);
            assert!(e.contains(want), "{text:?}: got {e:?}, want {want:?}");
        }
    }

    #[test]
    fn argv_expansion_is_shell_free_and_single_pass() {
        let a = p(GOOD).unwrap();
        let prompt = "do {cwd}; rm -rf / && echo $(whoami)";
        let argv = a
            .launch_argv(
                "/bin/my-agent",
                LaunchMode::Headless,
                prompt,
                Path::new("/repo"),
                None,
            )
            .unwrap();
        assert_eq!(argv, ["/bin/my-agent", "run", "--cwd=/repo", prompt]);

        let argv = a
            .launch_argv(
                "ma",
                LaunchMode::Interactive,
                "hi",
                Path::new("/r"),
                Some("big"),
            )
            .unwrap();
        assert_eq!(argv, ["ma", "--model", "big", "hi"]);

        // Interactive falls back to args; no model_args means no model.
        let b = p("name = \"x\"\ncommand = \"x\"\nargs = [\"-p\", \"{prompt}\"]").unwrap();
        let argv = b
            .launch_argv("x", LaunchMode::Interactive, "hi", Path::new("/r"), None)
            .unwrap();
        assert_eq!(argv, ["x", "-p", "hi"]);
        let e = b
            .launch_argv("x", LaunchMode::Headless, "hi", Path::new("/r"), Some("m"))
            .unwrap_err();
        assert_eq!(e.code(), "unsupported");

        // Braces that are not placeholders stay as they are.
        assert_eq!(
            expand("{} {x} {prompt", &[("prompt", "P")]),
            "{} {x} {prompt"
        );
    }

    #[test]
    fn loads_user_and_project_files_and_reports_bad_ones() {
        let home = tempfile::tempdir().unwrap();
        let paths = GlobalPaths {
            data_dir: home.path().join("data"),
            config_dir: home.path().join("config"),
        };
        let proj = tempfile::tempdir().unwrap();
        let ud = user_dir(&paths);
        let pd = project_dir(proj.path());
        std::fs::create_dir_all(&ud).unwrap();
        std::fs::create_dir_all(&pd).unwrap();
        std::fs::write(ud.join("mine.toml"), GOOD).unwrap();
        std::fs::write(ud.join("notes.txt"), "ignored").unwrap();
        std::fs::write(pd.join("dup.toml"), GOOD).unwrap();
        std::fs::write(pd.join("broken.toml"), "name = ").unwrap();
        std::fs::write(
            pd.join("team.toml"),
            "name = \"team\"\ncommand = \"team\"\nargs = [\"{prompt}\"]",
        )
        .unwrap();

        let l = load(&paths, Some(proj.path()));
        let names: Vec<_> = l
            .agents
            .iter()
            .map(|a| (a.name.as_str(), a.scope))
            .collect();
        assert_eq!(names, [("my-agent", Scope::User), ("team", Scope::Project)]);
        assert_eq!(l.errors.len(), 2, "{:?}", l.errors);
        assert!(l.errors.iter().any(|e| e.path.ends_with("broken.toml")));
        assert!(l
            .errors
            .iter()
            .any(|e| e.path.ends_with("dup.toml") && e.message.contains("already defined")));

        // No folders: nothing, and no errors.
        let empty = tempfile::tempdir().unwrap();
        let l = load(
            &GlobalPaths {
                data_dir: empty.path().into(),
                config_dir: empty.path().into(),
            },
            None,
        );
        assert!(l.agents.is_empty() && l.errors.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn login_check_uses_exit_status_and_env() {
        let mut a = p("name = \"x\"\ncommand = \"x\"\nargs = [\"{prompt}\"]\nlogin_check = [\"sh\", \"-c\", \"test \\\"$TOKEN\\\" = yes\"]\n[env]\nTOKEN = \"yes\"").unwrap();
        assert_eq!(a.check_login(Duration::from_secs(5)), Some(Ok(())));
        a.env.insert("TOKEN".into(), "no".into());
        assert!(matches!(
            a.check_login(Duration::from_secs(5)),
            Some(Err(_))
        ));
        a.login_check = None;
        assert_eq!(a.check_login(Duration::from_secs(5)), None);
    }
}
