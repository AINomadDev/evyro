//! Git integration through the official `git` CLI with explicit argv (no shell).
//! Using the CLI avoids a libgit2 C dependency and matches what agents see.

use crate::error::{Error, Result};
use crate::util;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(Error::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn version() -> Result<String> {
    let out = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|e| Error::Git(e.to_string()))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn is_repo(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

pub fn toplevel(dir: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(
        git(dir, &["rev-parse", "--show-toplevel"])?.trim(),
    ))
}

/// The main working tree for a (possibly linked) worktree.
pub fn main_worktree(dir: &Path) -> Result<PathBuf> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = PathBuf::from(common.trim());
    common
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| Error::Git("no main worktree".into()))
}

pub fn remote_url(dir: &Path) -> Result<Option<String>> {
    match git(dir, &["remote", "get-url", "origin"]) {
        Ok(s) => Ok(Some(redact_url(s.trim()))),
        Err(_) => Ok(None),
    }
}

/// Strip credentials from remote URLs (https://user:token@host/...).
pub fn redact_url(u: &str) -> String {
    if let Some(scheme_end) = u.find("://") {
        let rest = &u[scheme_end + 3..];
        if let Some(at) = rest.find('@') {
            if rest[..at].contains(':') || rest[..at].len() > 20 {
                return format!("{}://{}", &u[..scheme_end], &rest[at + 1..]);
            }
        }
    }
    u.to_string()
}

pub fn default_branch(dir: &Path) -> Result<Option<String>> {
    if let Ok(s) = git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        return Ok(s.trim().strip_prefix("origin/").map(|s| s.to_string()));
    }
    for b in ["main", "master", "trunk"] {
        if git(
            dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{b}"),
            ],
        )
        .is_ok()
        {
            return Ok(Some(b.into()));
        }
    }
    Ok(current_branch(dir).ok().flatten())
}

pub fn current_branch(dir: &Path) -> Result<Option<String>> {
    let s = git(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default();
    let s = s.trim();
    Ok(if s.is_empty() { None } else { Some(s.into()) })
}

pub fn head_sha(dir: &Path) -> Result<Option<String>> {
    match git(dir, &["rev-parse", "--verify", "-q", "HEAD"]) {
        Ok(s) => Ok(Some(s.trim().into())),
        Err(_) => Ok(None), // unborn branch
    }
}

pub fn commit_exists(dir: &Path, sha: &str) -> bool {
    git(dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_ok()
}

pub fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) fn is_clean(root: &Path) -> Result<bool> {
    let status = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    // Claiming a task can write this local file even before Evyro's ignore file exists.
    Ok(status
        .split_terminator('\0')
        .all(|entry| entry == "?? .evyro/CURRENT_TASK.md"))
}

pub(crate) fn commit_patch(root: &Path, base: &str, head: &str) -> Result<(Vec<String>, File)> {
    let paths = git(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-color",
            "--no-relative",
            "--no-renames",
            "--ignore-submodules=none",
            base,
            head,
            "--",
        ],
    )?;
    let files = paths
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    let patch = tempfile::tempfile()?;
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "diff",
            "--binary",
            "--full-index",
            "--no-color",
            "--no-ext-diff",
            "--no-relative",
            "--no-textconv",
            "--no-renames",
            "--ignore-submodules=none",
            "--submodule=short",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            base,
            head,
            "--",
        ])
        .stdout(patch.try_clone()?)
        .output()?;
    if !out.status.success() {
        return Err(Error::Git(format!(
            "cannot read candidate patch: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok((files, patch))
}

pub(crate) fn apply_to_index(root: &Path, patch: &mut File, check: bool) -> Result<()> {
    patch.seek(SeekFrom::Start(0))?;
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args([
        "-c",
        "apply.ignoreWhitespace=no",
        "apply",
        "--index",
        "--whitespace=nowarn",
    ]);
    if check {
        cmd.arg("--check");
    }
    let out = cmd
        .arg("-")
        .stdin(Stdio::from(patch.try_clone()?))
        .output()?;
    if !out.status.success() {
        return Err(Error::Conflict(format!(
            "Cannot stage the candidate in this checkout. {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileState {
    pub path: String,
    /// Two-letter porcelain status, e.g. " M", "??", "A ", " D".
    pub status: String,
    /// sha256 of working-tree content; None for deleted files/directories.
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GitSnapshot {
    pub root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub dirty: bool,
    pub files: Vec<FileState>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
}

impl GitSnapshot {
    /// Content-addressed fingerprint of the repository state.
    pub fn fingerprint(&self) -> String {
        let mut s = String::new();
        s.push_str(self.head.as_deref().unwrap_or("-"));
        s.push('\n');
        let mut files: Vec<_> = self.files.iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        for f in files {
            s.push_str(&format!(
                "{}\t{}\t{}\n",
                f.status,
                f.path,
                f.sha256.as_deref().unwrap_or("-")
            ));
        }
        util::sha256_hex(s.as_bytes())
    }
}

pub fn status_files(root: &Path) -> Result<Vec<FileState>> {
    let raw = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut out = Vec::new();
    let mut parts = raw.split('\0').filter(|s| !s.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let status = entry[..2].to_string();
        let path = entry[3..].to_string();
        // Renames/copies carry the original path as the next NUL field.
        if status.starts_with('R') || status.starts_with('C') {
            parts.next();
        }
        // Evyro's own bookkeeping never counts as repository drift.
        if path == ".evyro" || path.starts_with(".evyro/") {
            continue;
        }
        let full = root.join(&path);
        let (sha, size) = match std::fs::metadata(&full) {
            Ok(m) if m.is_file() => (util::sha256_file(&full).ok(), Some(m.len())),
            _ => (None, None),
        };
        out.push(FileState {
            path,
            status,
            sha256: sha,
            size,
        });
    }
    Ok(out)
}

/// Files in the working tree that Git tracks or would track: committed,
/// staged and new files, without anything `.gitignore` excludes.
pub fn ls_files(root: &Path) -> Result<Vec<String>> {
    let raw = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut v: Vec<String> = raw
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    v.sort();
    v.dedup();
    Ok(v)
}

pub fn snapshot(dir: &Path) -> Result<GitSnapshot> {
    let root = toplevel(dir)?;
    let files = status_files(&root)?;
    let upstream = git(
        &root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .map(|s| s.trim().to_string());
    let (mut ahead, mut behind) = (None, None);
    if upstream.is_some() {
        if let Ok(s) = git(
            &root,
            &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
        ) {
            let mut it = s.split_whitespace();
            ahead = it.next().and_then(|x| x.parse().ok());
            behind = it.next().and_then(|x| x.parse().ok());
        }
    }
    Ok(GitSnapshot {
        root: root.to_string_lossy().to_string(),
        branch: current_branch(&root)?,
        head: head_sha(&root)?,
        dirty: !files.is_empty(),
        files,
        upstream,
        ahead,
        behind,
    })
}

pub fn diff_stat(root: &Path) -> Result<String> {
    git(root, &["diff", "--stat", "HEAD"]).or_else(|_| Ok(String::new()))
}

pub fn changed_since(root: &Path, sha: &str) -> Result<Vec<String>> {
    let s = git(root, &["diff", "--name-only", sha, "HEAD"])?;
    Ok(s.lines().map(|l| l.to_string()).collect())
}

pub fn log_oneline(root: &Path, range: &str, max: usize) -> Result<Vec<String>> {
    let s = git(root, &["log", "--oneline", &format!("-{max}"), range]).unwrap_or_default();
    Ok(s.lines().map(|l| l.to_string()).collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub locked: bool,
}

pub fn worktrees(root: &Path) -> Result<Vec<Worktree>> {
    let s = git(root, &["worktree", "list", "--porcelain"])?;
    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in s.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            cur = Some(Worktree {
                path: p.into(),
                head: None,
                branch: None,
                locked: false,
            });
        } else if let Some(w) = cur.as_mut() {
            if let Some(h) = line.strip_prefix("HEAD ") {
                w.head = Some(h.into());
            } else if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.trim_start_matches("refs/heads/").into());
            } else if line.starts_with("locked") {
                w.locked = true;
            }
        }
    }
    if let Some(w) = cur {
        out.push(w);
    }
    Ok(out)
}

/// Create a worktree on a new branch. Refuses if the path exists.
pub fn add_worktree(root: &Path, path: &Path, branch: &str) -> Result<()> {
    add_worktree_at(root, path, branch, "HEAD")
}

pub(crate) fn add_worktree_at(root: &Path, path: &Path, branch: &str, base: &str) -> Result<()> {
    if path.exists() {
        return Err(Error::Conflict(format!(
            "{} already exists",
            path.display()
        )));
    }
    validate_ref_name(branch)?;
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            branch,
            &path.to_string_lossy(),
            base,
        ],
    )?;
    Ok(())
}

/// Paths with uncommitted changes (tracked or new), leaving out Evyro's own generated files.
pub(crate) fn dirty_paths(root: &Path) -> Result<Vec<String>> {
    let raw = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(raw
        .split_terminator('\0')
        .filter_map(|e| e.get(3..))
        .filter(|p| !p.starts_with(".evyro/"))
        .map(str::to_owned)
        .collect())
}

/// How many commits `branch` has that `base` does not.
pub(crate) fn commits_ahead(root: &Path, base: &str, branch: &str) -> usize {
    git(root, &["rev-list", "--count", &format!("{base}..{branch}")])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Forget worktree records whose folder is gone.
pub(crate) fn prune_worktree_records(root: &Path) {
    let _ = git(root, &["worktree", "prune"]);
}

/// Delete a branch only if it is fully merged.
pub(crate) fn delete_merged_branch(root: &Path, branch: &str) -> bool {
    git(root, &["branch", "-d", branch]).is_ok()
}

/// Check out an existing branch in a new worktree. Refuses if the path exists.
pub(crate) fn add_worktree_on_branch(root: &Path, path: &Path, branch: &str) -> Result<()> {
    if path.exists() {
        return Err(Error::Conflict(format!(
            "{} already exists",
            path.to_string_lossy()
        )));
    }
    validate_ref_name(branch)?;
    git(root, &["worktree", "add", &path.to_string_lossy(), branch])?;
    Ok(())
}

pub(crate) fn branch_exists(root: &Path, branch: &str) -> bool {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

/// Roll back a branch created for a comparison only while it still points at its original base.
pub(crate) fn drop_unchanged_branch(root: &Path, branch: &str, base: &str) -> Result<()> {
    let full = format!("refs/heads/{branch}");
    if git(root, &["rev-parse", "--verify", &full])?.trim() != base {
        return Err(Error::Conflict(format!("branch {branch} has new work")));
    }
    git(root, &["update-ref", "-d", &full, base])?;
    Ok(())
}

/// Remove a worktree. Refuses when it has uncommitted changes unless `force`.
pub fn remove_worktree(root: &Path, path: &Path, force: bool) -> Result<()> {
    if !force && path.exists() && !status_files(path)?.is_empty() {
        return Err(Error::Conflict(format!(
            "worktree {} has uncommitted changes; commit them or pass --force to discard",
            path.display()
        )));
    }
    let p = path.to_string_lossy().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&p);
    git(root, &args)?;
    Ok(())
}

pub fn validate_ref_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.ends_with('/')
        && !name.ends_with(".lock")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c));
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!("invalid branch name: {name}")))
    }
}

/// Unified diff of the working tree against HEAD (tracked files), capped.
/// Sensitive paths are left out. The caller must still redact the text.
pub fn diff_text(root: &Path, max_bytes: usize) -> String {
    diff_text_from(root, "HEAD", max_bytes).unwrap_or_default()
}

pub(crate) fn diff_text_from(root: &Path, base: &str, max_bytes: usize) -> Result<String> {
    let mut s = git(
        root,
        &[
            "diff",
            base,
            "--no-color",
            "--",
            ".",
            ":(exclude).evyro",
            ":(exclude)*.env",
            ":(exclude).env*",
            ":(exclude)*.pem",
            ":(exclude)*.key",
        ],
    )?;
    if s.len() > max_bytes {
        let mut cut = max_bytes;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n… diff truncated …\n");
    }
    Ok(s)
}

/// New, untracked text files as "added" diffs, so they can be reviewed like
/// any other change. Sensitive paths, binary files and files over
/// `max_file_bytes` are skipped. The caller must still redact the text.
pub fn untracked_diff(root: &Path, files: &[FileState], max_file_bytes: u64) -> String {
    let mut out = String::new();
    for f in files.iter().filter(|f| f.status == "??") {
        if crate::redact::is_sensitive_path(&f.path) || f.size.map_or(true, |n| n > max_file_bytes)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&f.path)) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        out.push_str(&format!(
            "diff --git a/{p} b/{p}\nnew file mode 100644\n--- /dev/null\n+++ b/{p}\n@@ -0,0 +1,{n} @@\n",
            p = f.path,
            n = lines.len()
        ));
        for l in lines {
            out.push('+');
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_text_files_become_added_diffs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("new.rs"), "fn a() {}\nfn b() {}\n").unwrap();
        std::fs::write(dir.path().join(".env"), "TOKEN=x\n").unwrap();
        std::fs::write(dir.path().join("bin.dat"), [0u8, 1, 2]).unwrap();
        let st = |p: &str, size: u64| FileState {
            path: p.into(),
            status: "??".into(),
            sha256: None,
            size: Some(size),
        };
        let files = [
            st("new.rs", 20),
            st(".env", 8),
            st("bin.dat", 3),
            st("big.txt", 10_000_000),
        ];
        let d = untracked_diff(dir.path(), &files, 1_000_000);
        assert!(
            d.contains("+++ b/new.rs\n@@ -0,0 +1,2 @@\n+fn a() {}\n+fn b() {}\n"),
            "{d}"
        );
        assert!(
            !d.contains(".env") && !d.contains("bin.dat") && !d.contains("big.txt"),
            "{d}"
        );
    }
    #[test]
    fn redacts_credentials_in_urls() {
        assert_eq!(
            redact_url("https://user:ghp_secret@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(
            redact_url("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
    }
    #[test]
    fn ref_names() {
        assert!(validate_ref_name("feature/x-1").is_ok());
        assert!(validate_ref_name("-rf").is_err());
        assert!(validate_ref_name("a..b").is_err());
        assert!(validate_ref_name("a b").is_err());
    }
}
