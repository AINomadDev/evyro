//! `evyro agents check`: a test kit for one agent definition, built-in or a
//! custom TOML file. Each step is a named check that passes, fails or is
//! skipped with a reason, so a contributor adding an agent sees exactly what
//! Evyro will do with it before anyone runs it on a task.
//!
//! Steps: the definition parses, the program is found, its version (built-in
//! agents), its login check, the argv Evyro would build (one prompt argument,
//! no shell, no re-expansion), and, only when asked, a smoke run of the real
//! headless command in an empty scratch folder.

use crate::adapters::{self, AdapterSpec, LaunchMode};
use crate::custom_agents::{self, CustomAgent, Scope};
use crate::paths::GlobalPaths;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A prompt full of shell and template syntax. If argv building is right it
/// arrives as exactly one argument, unchanged.
const PROBE_PROMPT: &str =
    "evyro check: say OK; echo $(id) `id` | x > y && {cwd} {model} \"q\" 'q'";

/// What the smoke run asks the agent to do.
const SMOKE_PROMPT: &str = "This is an Evyro adapter check. Reply with the single word OK. Do not read, create or change any files.";

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub name: String,
    /// pass | fail | warn | skip
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub agent: String,
    pub display_name: String,
    /// "built-in", or the TOML file the agent came from.
    pub source: String,
    pub steps: Vec<Step>,
    /// True when no step failed.
    pub ok: bool,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Run the real headless command once in a scratch folder.
    pub smoke: bool,
    pub smoke_timeout: Duration,
    pub login_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            smoke: false,
            smoke_timeout: Duration::from_secs(180),
            login_timeout: Duration::from_secs(10),
        }
    }
}

enum Candidate {
    BuiltIn(AdapterSpec),
    Custom(CustomAgent),
}

fn step(name: &str, state: &str, detail: impl Into<String>) -> Step {
    Step {
        name: name.into(),
        state: state.into(),
        detail: detail.into(),
    }
}

/// Check one agent. `target` is a built-in id or alias, a custom agent's name,
/// or a path to a `.toml` file (checked as your own file, so its commands run).
pub fn check(
    paths: &GlobalPaths,
    project_root: Option<&Path>,
    target: &str,
    opts: &Options,
) -> Report {
    let mut steps = Vec::new();
    let found = resolve(paths, project_root, target);
    let cand = match found {
        Ok((c, detail)) => {
            steps.push(step("definition", "pass", detail));
            c
        }
        Err(why) => {
            steps.push(step("definition", "fail", why));
            return finish(target.into(), target.into(), "-".into(), steps);
        }
    };
    let (id, display, source) = match &cand {
        Candidate::BuiltIn(s) => (
            s.id.to_string(),
            s.display_name.to_string(),
            "built-in".to_string(),
        ),
        Candidate::Custom(a) => (
            a.name.clone(),
            a.display_name.clone(),
            a.source.display().to_string(),
        ),
    };

    // Program on PATH.
    let bin: Option<PathBuf> = match &cand {
        Candidate::BuiltIn(s) if s.binaries.is_empty() => {
            steps.push(step("binary", "skip", format!("no CLI: {}", s.notes)));
            None
        }
        Candidate::BuiltIn(s) => {
            let b = s.binaries.iter().find_map(|b| adapters::which(b));
            steps.push(match &b {
                Some(p) => step("binary", "pass", p.display().to_string()),
                None => step(
                    "binary",
                    "fail",
                    format!("`{}` not found on PATH", s.binaries.join("` or `")),
                ),
            });
            b
        }
        Candidate::Custom(a) => {
            let b = a.binary();
            steps.push(match &b {
                Some(p) => step("binary", "pass", p.display().to_string()),
                None => step(
                    "binary",
                    "fail",
                    format!("`{}` not found (PATH or full path)", a.command),
                ),
            });
            b
        }
    };

    // Version: built-in agents declare a minimum; custom ones do not.
    steps.push(match (&cand, &bin) {
        (Candidate::BuiltIn(s), Some(_)) => {
            let d = adapters::detect(s);
            let state = match d.compatibility.as_str() {
                "ok" => "pass",
                "incompatible" => "fail",
                _ => "warn",
            };
            let v = d.version.unwrap_or_else(|| "-".into());
            step(
                "version",
                state,
                d.diagnostic.map_or(v.clone(), |n| format!("{v}: {n}")),
            )
        }
        (Candidate::BuiltIn(_), None) => step("version", "skip", "no program to ask"),
        (Candidate::Custom(_), _) => {
            step("version", "skip", "custom agents declare no version check")
        }
    });

    steps.push(login_step(&cand, bin.is_some(), opts.login_timeout));
    steps.extend(argv_steps(&cand, bin.as_deref()));

    if opts.smoke {
        steps.push(smoke_step(&cand, bin.as_deref(), opts.smoke_timeout));
    } else {
        steps.push(step(
            "smoke",
            "skip",
            "not requested; add --smoke to start the agent once",
        ));
    }
    finish(id, display, source, steps)
}

fn finish(agent: String, display_name: String, source: String, steps: Vec<Step>) -> Report {
    let ok = steps.iter().all(|s| s.state != "fail");
    Report {
        agent,
        display_name,
        source,
        steps,
        ok,
    }
}

fn resolve(
    paths: &GlobalPaths,
    project_root: Option<&Path>,
    target: &str,
) -> std::result::Result<(Candidate, String), String> {
    let as_path = Path::new(target);
    if target.ends_with(".toml") || target.contains(['/', '\\']) {
        let text = std::fs::read_to_string(as_path)
            .map_err(|e| format!("cannot read {}: {e}", as_path.display()))?;
        // Named on the command line by the person running the check, so it is
        // treated as their own file (its login check and smoke run may run).
        return custom_agents::parse(&text, as_path, Scope::User)
            .map(|a| {
                (
                    Candidate::Custom(a),
                    format!("{} parses", as_path.display()),
                )
            })
            .map_err(|e| format!("{}: {e}", as_path.display()));
    }
    let id = adapters::normalize_id(target);
    if let Some(s) = adapters::registry().into_iter().find(|a| a.id == id) {
        return Ok((Candidate::BuiltIn(s), "built-in agent".into()));
    }
    let loaded = custom_agents::load(paths, project_root);
    if let Some(a) = loaded.agents.into_iter().find(|a| a.name == target) {
        let d = format!("{} parses", a.source.display());
        return Ok((Candidate::Custom(a), d));
    }
    // A file named after the agent that failed to load explains the miss.
    if let Some(e) = loaded
        .errors
        .iter()
        .find(|e| e.path.file_stem().is_some_and(|s| s == target))
    {
        return Err(format!("{}: {}", e.path.display(), e.message));
    }
    Err(format!(
        "no agent named '{target}'; known built-ins: {} (custom agents: `{}`, `{}`, or pass a .toml path)",
        adapters::registry().iter().map(|a| a.id).collect::<Vec<_>>().join(", "),
        custom_agents::user_dir(paths).display(),
        project_root.map_or(".evyro/agents".into(), |r| custom_agents::project_dir(r).display().to_string()),
    ))
}

fn login_step(cand: &Candidate, have_bin: bool, timeout: Duration) -> Step {
    match cand {
        Candidate::BuiltIn(s) => {
            let c = crate::targets::login_evidence(s.id);
            let state = match c.state.as_str() {
                "ok" => "pass",
                "missing" => "fail",
                _ => "skip",
            };
            step("login", state, c.detail)
        }
        Candidate::Custom(a) => match (&a.login_check, a.scope) {
            (None, _) => step("login", "skip", "no login_check in the file"),
            (Some(c), Scope::Project) => step(
                "login",
                "skip",
                format!(
                    "project file (repository text), not run: run `{}` yourself, or pass the file's path to check it as yours",
                    c.join(" ")
                ),
            ),
            (Some(_), _) if !have_bin => step("login", "skip", "program not found"),
            (Some(c), _) => match a.check_login(timeout) {
                Some(Ok(())) => step("login", "pass", format!("`{}` exited 0", c.join(" "))),
                Some(Err(why)) => step("login", "fail", why),
                None => step("login", "skip", "no login_check in the file"),
            },
        },
    }
}

/// Build argv the way a launch would and check its shape.
fn build(
    cand: &Candidate,
    bin: &str,
    mode: LaunchMode,
    prompt: &str,
    cwd: &Path,
    model: Option<&str>,
) -> crate::error::Result<Vec<String>> {
    match cand {
        Candidate::BuiltIn(s) => s.launch_argv(bin, mode, prompt, model),
        Candidate::Custom(a) => a.launch_argv(bin, mode, prompt, cwd, model),
    }
}

fn supports_model(cand: &Candidate) -> bool {
    match cand {
        Candidate::BuiltIn(s) => s.capabilities.model_selection && s.model_flag.is_some(),
        Candidate::Custom(a) => a.model_args.is_some(),
    }
}

/// Problems with one argv, or none.
fn argv_problems(argv: &[String], plain: &[String], bin: &str, model: Option<&str>) -> Vec<String> {
    let mut v = Vec::new();
    if argv.first().map(String::as_str) != Some(bin) {
        v.push(format!(
            "argv[0] is {:?}, expected the program {bin:?}",
            argv.first()
        ));
    }
    let holders = argv.iter().filter(|a| a.contains(PROBE_PROMPT)).count();
    if holders != 1 {
        v.push(format!(
            "the prompt appears unchanged in {holders} arguments, expected exactly 1"
        ));
    }
    if argv.len() != plain.len() {
        v.push(format!(
            "a prompt with spaces and quotes changed the argument count ({} vs {})",
            argv.len(),
            plain.len()
        ));
    }
    if let Some(m) = model {
        if !argv.iter().any(|a| a.contains(m)) {
            v.push(format!("model {m:?} is not passed"));
        }
    }
    v
}

fn argv_steps(cand: &Candidate, bin: Option<&Path>) -> Vec<Step> {
    let bin = match (bin, cand) {
        (Some(p), _) => p.to_string_lossy().into_owned(),
        (None, Candidate::BuiltIn(s)) => match s.binaries.first() {
            Some(b) => (*b).to_string(),
            None => return vec![step("argv", "skip", "no CLI to launch")],
        },
        (None, Candidate::Custom(a)) => a.command.clone(),
    };
    let cwd = Path::new("/work/repo");
    let model = supports_model(cand).then_some("test-model-1");
    let mut cases = vec![
        ("argv headless", LaunchMode::Headless, None),
        ("argv interactive", LaunchMode::Interactive, None),
    ];
    if model.is_some() {
        cases.push(("argv headless + model", LaunchMode::Headless, model));
    }
    cases
        .into_iter()
        .map(|(name, mode, m)| {
            let r = build(cand, &bin, mode, PROBE_PROMPT, cwd, m)
                .and_then(|a| build(cand, &bin, mode, "x", cwd, m).map(|p| (a, p)));
            let (argv, plain) = match r {
                Ok(x) => x,
                Err(e) => return step(name, "skip", e.to_string()),
            };
            let problems = argv_problems(&argv, &plain, &bin, m);
            // Show the shape with readable placeholders.
            let shown = build(cand, &bin, mode, "{prompt}", Path::new("{cwd}"), m)
                .map(|a| format!("{a:?}"))
                .unwrap_or_default();
            if problems.is_empty() {
                step(name, "pass", shown)
            } else {
                step(
                    name,
                    "fail",
                    format!("{}; argv: {shown}", problems.join("; ")),
                )
            }
        })
        .collect()
}

fn smoke_step(cand: &Candidate, bin: Option<&Path>, timeout: Duration) -> Step {
    let Some(bin) = bin else {
        return step("smoke", "skip", "program not found");
    };
    if let Candidate::Custom(a) = cand {
        if a.scope == Scope::Project {
            return step(
                "smoke",
                "skip",
                "project file (repository text), not run: pass the file's path to check it as yours",
            );
        }
    }
    let (Ok(work), Ok(logs)) = (tempfile::tempdir(), tempfile::tempdir()) else {
        return step("smoke", "fail", "could not create a scratch folder");
    };
    let bin_s = bin.to_string_lossy().into_owned();
    let argv = match build(
        cand,
        &bin_s,
        LaunchMode::Headless,
        SMOKE_PROMPT,
        work.path(),
        None,
    ) {
        Ok(a) => adapters::launchable_argv(a),
        Err(e) => return step("smoke", "skip", e.to_string()),
    };
    let out_path = logs.path().join("output.txt");
    let Ok((out, err)) = std::fs::File::create(&out_path).and_then(|f| Ok((f.try_clone()?, f)))
    else {
        return step("smoke", "fail", "could not create the output file");
    };
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(work.path())
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    if let Candidate::Custom(a) = cand {
        cmd.envs(&a.env);
    }
    let start = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return step(
                "smoke",
                "fail",
                format!("could not start `{}`: {e}", argv[0]),
            )
        }
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("did not exit within {timeout:?}; killed"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => break Err(e.to_string()),
        }
    };
    let text = std::fs::read_to_string(&out_path).unwrap_or_default();
    let tail: Vec<&str> = text
        .lines()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let (tail, _) = crate::redact::redact(&tail.join("\n"));
    let secs = start.elapsed().as_secs_f32();
    match status {
        Ok(s) if s.success() => step(
            "smoke",
            "pass",
            format!("exited 0 in {secs:.1}s, {} bytes of output", text.len()),
        ),
        Ok(s) => step(
            "smoke",
            "fail",
            format!("exited with {s} after {secs:.1}s; last output:\n{tail}"),
        ),
        Err(why) => step("smoke", "fail", format!("{why}; last output:\n{tail}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> GlobalPaths {
        GlobalPaths {
            data_dir: dir.join("data"),
            config_dir: dir.join("config"),
        }
    }

    fn state<'a>(r: &'a Report, name: &str) -> &'a str {
        r.steps
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.state.as_str())
            .unwrap_or("absent")
    }

    #[test]
    fn unknown_agent_fails_definition_only() {
        let d = tempfile::tempdir().unwrap();
        let r = check(&paths(d.path()), None, "nope-agent", &Options::default());
        assert!(!r.ok);
        assert_eq!(r.steps.len(), 1);
        assert_eq!(state(&r, "definition"), "fail");
        assert!(r.steps[0].detail.contains("claude-code"));
    }

    #[test]
    fn bad_toml_file_reports_the_parse_error() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("bad.toml");
        std::fs::write(&f, "name = \"bad\"\ncommand = \"x\"\nargs = [\"run\"]").unwrap();
        let r = check(
            &paths(d.path()),
            None,
            f.to_str().unwrap(),
            &Options::default(),
        );
        assert!(!r.ok);
        assert!(r.steps[0].detail.contains("{prompt}"), "{:?}", r.steps);
    }

    #[test]
    fn broken_file_in_agents_dir_explains_a_missing_name() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(d.path());
        let dir = custom_agents::user_dir(&p);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mine.toml"),
            "name = \"mine\"\ncommand = \"x\"\nargs = [\"{task}\"]",
        )
        .unwrap();
        let r = check(&p, None, "mine", &Options::default());
        assert!(
            r.steps[0].detail.contains("unknown placeholder"),
            "{:?}",
            r.steps
        );
    }

    #[test]
    fn built_in_argv_keeps_the_prompt_in_one_argument() {
        // argv only: `check` would run the real program's `--version`.
        let spec = adapters::get("claude").unwrap();
        let steps = argv_steps(&Candidate::BuiltIn(spec), Some(Path::new("/x/claude")));
        let names: Vec<_> = steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["argv headless", "argv interactive", "argv headless + model"]
        );
        assert!(steps.iter().all(|s| s.state == "pass"), "{steps:?}");
        assert!(
            steps[0].detail.contains("\"{prompt}\""),
            "{}",
            steps[0].detail
        );
    }

    #[test]
    fn argv_problems_catch_split_and_missing_prompts() {
        let bin = "/bin/a";
        let plain = vec![bin.to_string(), "x".into()];
        let split: Vec<String> = std::iter::once(bin.to_string())
            .chain(PROBE_PROMPT.split(' ').map(String::from))
            .collect();
        let p = argv_problems(&split, &plain, bin, None);
        assert!(p.iter().any(|x| x.contains("exactly 1")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("argument count")), "{p:?}");
        let good = vec![bin.to_string(), PROBE_PROMPT.to_string()];
        assert!(argv_problems(&good, &plain, bin, None).is_empty());
        assert!(!argv_problems(&good, &plain, bin, Some("m1")).is_empty());
    }

    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn custom_file_end_to_end_with_smoke() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(
            d.path(),
            "fake-agent",
            "echo \"args: $#\"\n[ \"$1\" = run ] || exit 3\nexit 0\n",
        );
        let login = script(d.path(), "fake-login", "exit 0\n");
        let f = d.path().join("fake.toml");
        std::fs::write(
            &f,
            format!(
                "name = \"fake\"\ncommand = \"{}\"\nargs = [\"run\", \"--cwd={{cwd}}\", \"{{prompt}}\"]\nmodel_args = [\"-m\", \"{{model}}\"]\nlogin_check = [\"{}\"]\n",
                bin.display(),
                login.display()
            ),
        )
        .unwrap();
        let opts = Options {
            smoke: true,
            ..Options::default()
        };
        let r = check(&paths(d.path()), None, f.to_str().unwrap(), &opts);
        assert!(r.ok, "{:#?}", r.steps);
        for s in [
            "definition",
            "binary",
            "login",
            "argv headless",
            "argv headless + model",
            "argv interactive",
            "smoke",
        ] {
            assert_eq!(state(&r, s), "pass", "{s}: {:#?}", r.steps);
        }
        assert_eq!(state(&r, "version"), "skip");

        // A failing login check and a program that exits non-zero both fail.
        std::fs::write(&login, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::write(&bin, "#!/bin/sh\necho boom >&2\nexit 2\n").unwrap();
        let r = check(&paths(d.path()), None, f.to_str().unwrap(), &opts);
        assert!(!r.ok);
        assert_eq!(state(&r, "login"), "fail");
        assert_eq!(state(&r, "smoke"), "fail");
        let smoke = r.steps.iter().find(|s| s.name == "smoke").unwrap();
        assert!(smoke.detail.contains("boom"), "{}", smoke.detail);
    }

    #[cfg(unix)]
    #[test]
    fn project_files_are_not_run() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("repo");
        let dir = custom_agents::project_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = d.path().join("ran");
        let bin = script(
            d.path(),
            "proj-agent",
            &format!("touch {}\n", marker.display()),
        );
        std::fs::write(
            dir.join("proj.toml"),
            format!(
                "name = \"proj\"\ncommand = \"{b}\"\nargs = [\"{{prompt}}\"]\nlogin_check = [\"{b}\"]\n",
                b = bin.display()
            ),
        )
        .unwrap();
        let opts = Options {
            smoke: true,
            ..Options::default()
        };
        let r = check(&paths(d.path()), Some(&root), "proj", &opts);
        assert_eq!(state(&r, "login"), "skip");
        assert_eq!(state(&r, "smoke"), "skip");
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn smoke_times_out() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(d.path(), "slow", "sleep 5\n");
        let f = d.path().join("slow.toml");
        std::fs::write(
            &f,
            format!(
                "name = \"slow\"\ncommand = \"{}\"\nargs = [\"{{prompt}}\"]\n",
                bin.display()
            ),
        )
        .unwrap();
        let opts = Options {
            smoke: true,
            smoke_timeout: Duration::from_millis(300),
            ..Options::default()
        };
        let r = check(&paths(d.path()), None, f.to_str().unwrap(), &opts);
        let smoke = r.steps.iter().find(|s| s.name == "smoke").unwrap();
        assert_eq!(smoke.state, "fail");
        assert!(smoke.detail.contains("did not exit"), "{}", smoke.detail);
    }
}
