//! Review comments on working-tree changes.
//!
//! A person reviews what an AI changed (the desktop Changes view or
//! `evyro review`), leaves comments on files and lines, and sends the open ones
//! to an agent as one prompt. The agent addresses them and resolves them. Open
//! comments also appear in the context packet, so the next agent sees them.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::{brand, redact, util};
use serde::{Deserialize, Serialize};

pub const STATUSES: [&str; 3] = ["open", "sent", "resolved"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewComment {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub path: String,
    pub line: Option<u32>,
    pub snippet: String,
    pub body: String,
    pub status: String,
    pub author: String,
    pub commit_sha: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ReviewComment {
    /// The last 8 characters of the id: unique enough to type
    /// (`evyro review resolve 4f9a02c1`). Ids share a time-based prefix.
    pub fn short_id(&self) -> &str {
        &self.id[self.id.len().saturating_sub(8)..]
    }

    /// `src/a.rs:12`, or just the path for a whole-file comment.
    pub fn location(&self) -> String {
        match self.line {
            Some(l) => format!("{}:{}", self.path, l),
            None => self.path.clone(),
        }
    }
}

pub struct NewReviewComment<'a> {
    pub task_id: Option<&'a str>,
    pub path: &'a str,
    pub line: Option<u32>,
    pub snippet: &'a str,
    pub body: &'a str,
    pub author: &'a str,
    pub commit_sha: Option<&'a str>,
}

/// Parse `path:line` (or a bare path) as typed on the command line.
pub fn parse_location(s: &str) -> Result<(String, Option<u32>)> {
    let s = s.trim();
    if let Some((p, l)) = s.rsplit_once(':') {
        if let Ok(n) = l.parse::<u32>() {
            if n == 0 {
                return Err(Error::Invalid("line numbers start at 1".into()));
            }
            return Ok((clean_path(p)?, Some(n)));
        }
    }
    Ok((clean_path(s)?, None))
}

fn clean_path(p: &str) -> Result<String> {
    let p = p.trim().trim_start_matches("./").replace('\\', "/");
    if p.is_empty() || p.starts_with('/') || p.split('/').any(|c| c == "..") {
        return Err(Error::Invalid(format!(
            "review comments need a path inside the repository, got `{p}`"
        )));
    }
    Ok(p)
}

impl Store {
    pub fn add_review_comment(
        &self,
        project_id: &str,
        c: NewReviewComment,
    ) -> Result<ReviewComment> {
        let body = c.body.trim();
        if body.is_empty() {
            return Err(Error::Invalid("a review comment needs some text".into()));
        }
        let path = clean_path(c.path)?;
        if c.line == Some(0) {
            return Err(Error::Invalid("line numbers start at 1".into()));
        }
        let (snippet, _) = redact::redact(c.snippet.trim_end());
        let snippet: String = snippet.chars().take(400).collect();
        let id = util::new_id("rvc");
        let now = util::now();
        self.conn.execute(
            "INSERT INTO review_comments(id, project_id, task_id, path, line, snippet, body, status, author, commit_sha, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'open',?8,?9,?10,?10)",
            rusqlite::params![id, project_id, c.task_id, path, c.line, snippet, body, c.author, c.commit_sha, now],
        )?;
        self.emit(
            Some(project_id),
            c.task_id,
            None,
            "review.commented",
            serde_json::json!({"comment_id": id, "path": path, "line": c.line}),
        )?;
        self.get_review_comment(&id)
    }

    pub fn get_review_comment(&self, id: &str) -> Result<ReviewComment> {
        self.query_review("WHERE id = ?1", rusqlite::params![id])?
            .pop()
            .ok_or_else(|| Error::NotFound(format!("review comment {id}")))
    }

    /// Comments for a project, oldest first. `statuses` empty means all.
    pub fn list_review_comments(
        &self,
        project_id: &str,
        statuses: &[&str],
    ) -> Result<Vec<ReviewComment>> {
        let all = self.query_review(
            "WHERE project_id = ?1 ORDER BY path, COALESCE(line, 0), created_at",
            rusqlite::params![project_id],
        )?;
        Ok(all
            .into_iter()
            .filter(|c| statuses.is_empty() || statuses.contains(&c.status.as_str()))
            .collect())
    }

    /// Comments still waiting for an agent: open or sent, not resolved.
    pub fn pending_review_comments(&self, project_id: &str) -> Result<Vec<ReviewComment>> {
        self.list_review_comments(project_id, &["open", "sent"])
    }

    /// Record delivery of the open comments included in a particular prompt.
    pub fn mark_review_sent(&self, project_id: &str, ids: &[String], actor: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let delivered: std::collections::HashSet<_> = ids.iter().map(String::as_str).collect();
        for comment in self.list_review_comments(project_id, &["open"])? {
            if delivered.contains(comment.id.as_str()) {
                self.set_review_status(&comment.id, "sent", actor)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_review_status(&self, id: &str, status: &str, actor: &str) -> Result<ReviewComment> {
        if !STATUSES.contains(&status) {
            return Err(Error::Invalid(format!(
                "status must be one of {}",
                STATUSES.join(", ")
            )));
        }
        let c = self.get_review_comment(id)?;
        self.conn.execute(
            "UPDATE review_comments SET status = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, status, util::now()],
        )?;
        self.emit(
            Some(&c.project_id),
            c.task_id.as_deref(),
            None,
            &format!("review.{status}"),
            serde_json::json!({"comment_id": id, "actor": actor}),
        )?;
        self.get_review_comment(id)
    }

    /// Accepts a full id, or a unique start or end of one (the short id).
    pub fn resolve_review_ref(&self, project_id: &str, r: &str) -> Result<ReviewComment> {
        let r = r.trim();
        let hits: Vec<ReviewComment> = self
            .list_review_comments(project_id, &[])?
            .into_iter()
            .filter(|c| c.id == r || (r.len() >= 6 && (c.id.starts_with(r) || c.id.ends_with(r))))
            .collect();
        match hits.len() {
            1 => Ok(hits.into_iter().next().unwrap()),
            0 => Err(Error::NotFound(format!("review comment {r}"))),
            _ => Err(Error::Invalid(format!(
                "`{r}` matches more than one comment; use more of the id"
            ))),
        }
    }

    pub fn delete_review_comment(&self, id: &str) -> Result<()> {
        let c = self.get_review_comment(id)?;
        self.conn.execute(
            "DELETE FROM review_comments WHERE id = ?1",
            rusqlite::params![id],
        )?;
        self.emit(
            Some(&c.project_id),
            c.task_id.as_deref(),
            None,
            "review.deleted",
            serde_json::json!({"comment_id": id}),
        )?;
        Ok(())
    }

    fn query_review(
        &self,
        clause: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<ReviewComment>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT id, project_id, task_id, path, line, snippet, body, status, author, commit_sha, created_at, updated_at
             FROM review_comments {clause}"
        ))?;
        let rows = st.query_map(params, |r| {
            Ok(ReviewComment {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task_id: r.get(2)?,
                path: r.get(3)?,
                line: r.get(4)?,
                snippet: r.get(5)?,
                body: r.get(6)?,
                status: r.get(7)?,
                author: r.get(8)?,
                commit_sha: r.get(9)?,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// The prompt that hands review comments to an agent. Plain text, one
/// numbered item per comment, so it can be pasted into any AI or typed into a
/// running agent's terminal.
pub fn review_prompt(comments: &[ReviewComment]) -> String {
    let cli = brand::CLI_NAME;
    let mut s = format!(
        "Review comments on your changes ({} item{}). Address each one. When one is done, run `{cli} review resolve <id>`.\n",
        comments.len(),
        if comments.len() == 1 { "" } else { "s" }
    );
    for (i, c) in comments.iter().enumerate() {
        s.push_str(&format!(
            "\n{}. {} [{}]\n   {}\n",
            i + 1,
            c.location(),
            c.short_id(),
            c.body.replace('\n', "\n   ")
        ));
        if !c.snippet.is_empty() {
            s.push_str(&format!("   on: {}\n", c.snippet.trim()));
        }
    }
    s
}

/// The same comments as a context-packet section.
pub fn context_section(comments: &[ReviewComment]) -> String {
    let mut s = String::from("## Open review comments\n\nFrom the person reviewing the changes. Address them before handing off.\n\n");
    for c in comments {
        s.push_str(&format!(
            "- `{}` ({}): {}\n",
            c.location(),
            c.short_id(),
            c.body.replace('\n', " ")
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(path: &str, line: Option<u32>, body: &str) -> ReviewComment {
        ReviewComment {
            id: "rvc_0000aaaa1111".into(),
            project_id: "p".into(),
            task_id: None,
            path: path.into(),
            line,
            snippet: "+    let limit = 100;".into(),
            body: body.into(),
            status: "open".into(),
            author: "human".into(),
            commit_sha: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn locations_parse_and_reject_paths_outside_the_repo() {
        assert_eq!(
            parse_location("src/a.rs:12").unwrap(),
            ("src/a.rs".into(), Some(12))
        );
        assert_eq!(
            parse_location("./README.md").unwrap(),
            ("README.md".into(), None)
        );
        assert_eq!(parse_location("C:x").unwrap(), ("C:x".into(), None));
        assert!(parse_location("../etc/passwd:1").is_err());
        assert!(parse_location("/etc/passwd").is_err());
        assert!(parse_location("a.rs:0").is_err());
    }

    #[test]
    fn prompt_lists_each_comment_with_location_and_id() {
        let p = review_prompt(&[
            c("src/limit.rs", Some(7), "Make this configurable"),
            c("README.md", None, "Mention the limit"),
        ]);
        assert!(p.starts_with("Review comments on your changes (2 items)"));
        assert!(p.contains(
            "1. src/limit.rs:7 [aaaa1111]\n   Make this configurable\n   on: +    let limit = 100;"
        ));
        assert!(p.contains("2. README.md [aaaa1111]"));
        assert!(p.contains("review resolve <id>"));
        let s = context_section(&[c("a.rs", Some(1), "line\nbreak")]);
        assert!(s.contains("- `a.rs:1` (aaaa1111): line break"));
    }

    #[test]
    fn delivery_marks_only_selected_open_comments_in_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let root = dir.path().join("project");
        let other_root = dir.path().join("other");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&other_root).unwrap();
        let project = store.register_project(&root, None).unwrap().project;
        let other = store.register_project(&other_root, None).unwrap().project;
        let add = |project_id| {
            store
                .add_review_comment(
                    project_id,
                    NewReviewComment {
                        task_id: None,
                        path: "src/lib.rs",
                        line: None,
                        snippet: "",
                        body: "Review note",
                        author: "human",
                        commit_sha: None,
                    },
                )
                .unwrap()
                .id
        };
        let delivered = add(&project.id);
        let resolved = add(&project.id);
        let foreign = add(&other.id);
        let late = add(&project.id);
        store
            .set_review_status(&resolved, "resolved", "human")
            .unwrap();
        let ids = [delivered.clone(), resolved.clone(), foreign.clone()];
        store.mark_review_sent(&project.id, &ids, "human").unwrap();
        for (id, expected) in [
            (delivered, "sent"),
            (resolved, "resolved"),
            (foreign, "open"),
            (late, "open"),
        ] {
            assert_eq!(store.get_review_comment(&id).unwrap().status, expected);
        }
    }
}
