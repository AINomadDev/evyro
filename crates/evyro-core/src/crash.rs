//! Crash logs and bug reports.
//!
//! When Evyro panics, a short report is written to `<data>/logs/` with the
//! version, the system, where it happened and a backtrace, all passed through
//! [`redact`](crate::redact). `evyro bug-report` gathers those logs and the
//! doctor checks into one Markdown file a person can read, then attach to an
//! issue. Nothing is ever sent anywhere.

use crate::{brand, db::Store, paths::GlobalPaths, redact};
use std::path::{Path, PathBuf};

/// How many crash logs to keep.
const KEEP_LOGS: usize = 20;

fn logs_dir() -> Option<PathBuf> {
    GlobalPaths::resolve().ok().map(|p| p.data_dir.join("logs"))
}

/// Replace the home folder with `~`, so reports do not carry a user name.
pub fn tidy_paths(s: &str) -> String {
    match std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        Some(h) if !h.is_empty() => s.replace(&*h.to_string_lossy(), "~"),
        _ => s.to_string(),
    }
}

fn clean(s: &str) -> String {
    tidy_paths(&redact::redact(s).0)
}

/// Write a crash log for a panic. Returns where it went.
pub fn write_crash_log(
    app: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> Option<PathBuf> {
    let dir = logs_dir()?;
    write_crash_log_in(&dir, app, message, location, backtrace)
}

fn write_crash_log_in(
    dir: &Path,
    app: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let now = crate::util::now();
    let stamp = now.replace([':', '-'], "").replace('.', "_");
    let path = dir.join(format!("crash-{stamp}-{}.txt", std::process::id()));
    let body = format!(
        "{app} {version} crashed\ntime: {now}\nsystem: {os} {arch}\nat: {location}\nmessage: {message}\n\nbacktrace:\n{backtrace}\n",
        version = brand::VERSION,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        location = clean(location),
        message = clean(message),
        backtrace = clean(backtrace),
    );
    std::fs::write(&path, body).ok()?;
    prune(dir);
    Some(path)
}

fn crash_logs_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn prune(dir: &Path) {
    let mut v = crash_logs_in(dir);
    while v.len() > KEEP_LOGS {
        let _ = std::fs::remove_file(v.remove(0));
    }
}

/// Install a panic hook that writes a crash log, says where it is, then runs
/// the hook that was there before (which prints the panic as usual).
pub fn install_panic_hook(app: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("(no message)");
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        if let Some(p) = write_crash_log(app, message, &location, &bt) {
            eprintln!(
                "{app} crashed. A report was saved to {}.\nRun `{} bug-report` to gather it for an issue.",
                p.display(),
                brand::CLI_NAME
            );
        }
        previous(info);
    }));
}

/// A Markdown bug report: versions, system, doctor checks and recent crash
/// logs, with secrets redacted and the home folder shown as `~`. It holds no
/// task text, code or chat. The person reads it before sharing it.
pub fn bug_report(store: Option<&Store>, store_error: Option<&str>, cwd: &Path) -> String {
    let dir = logs_dir();
    bug_report_with(store, store_error, cwd, dir.as_deref())
}

fn bug_report_with(
    store: Option<&Store>,
    store_error: Option<&str>,
    cwd: &Path,
    logs: Option<&Path>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {} bug report\n\n", brand::PRODUCT_NAME));
    out.push_str("Read this before you share it. Secrets are redacted and your home folder is shown as `~`, but project names and paths remain.\n\n");
    out.push_str("## Version\n\n");
    out.push_str(&format!(
        "| | |\n|---|---|\n| {} | {} |\n| Database schema | v{} |\n| Contract schema | {} |\n| System | {} {} |\n\n",
        brand::CLI_NAME,
        brand::VERSION,
        crate::db::SCHEMA_VERSION,
        crate::contract::SCHEMA_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str("## Doctor\n\n");
    match store {
        Some(s) => {
            out.push_str("| Check | Status | Detail |\n|---|---|---|\n");
            for c in crate::doctor::run(s, cwd) {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    clean(&c.name),
                    c.status,
                    clean(&c.detail).replace('|', "\\|").replace('\n', " ")
                ));
            }
        }
        None => out.push_str(&format!(
            "The database could not be opened: {}\n",
            clean(store_error.unwrap_or("unknown error"))
        )),
    }
    out.push_str("\n## Recent crashes\n\n");
    let logs = logs.map(crash_logs_in).unwrap_or_default();
    if logs.is_empty() {
        out.push_str("None recorded.\n");
    }
    for p in logs.iter().rev().take(3) {
        let text = std::fs::read_to_string(p).unwrap_or_default();
        let text: String = text.lines().take(80).collect::<Vec<_>>().join("\n");
        out.push_str(&format!(
            "### {}\n\n```text\n{}\n```\n\n",
            p.file_name().unwrap_or_default().to_string_lossy(),
            clean(&text)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crash_logs_are_redacted_and_pruned() {
        let d = tempfile::tempdir().unwrap();
        let key = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");
        let p = write_crash_log_in(
            d.path(),
            "evyro",
            &format!("token {key} leaked"),
            "src/x.rs:1",
            "frame 0",
        )
        .unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(!text.contains(key), "{text}");
        assert!(text.contains("REDACTED"), "{text}");
        assert!(text.contains(brand::VERSION));
        for i in 0..KEEP_LOGS + 5 {
            std::fs::write(d.path().join(format!("crash-0000{i:03}.txt")), "x").unwrap();
        }
        prune(d.path());
        assert_eq!(crash_logs_in(d.path()).len(), KEEP_LOGS);
    }
    #[test]
    fn report_includes_crashes_without_secrets() {
        let d = tempfile::tempdir().unwrap();
        let key = concat!("sk_", "live_abcdefghijklmnopqrstuvwx");
        write_crash_log_in(d.path(), "evyro", &format!("bad {key}"), "a.rs:2", "bt").unwrap();
        let r = bug_report_with(None, Some("locked"), d.path(), Some(d.path()));
        assert!(r.contains("# Evyro bug report"));
        assert!(r.contains("could not be opened: locked"));
        assert!(r.contains("## Recent crashes"));
        assert!(r.contains("a.rs:2"));
        assert!(!r.contains(key), "{r}");
    }
}
