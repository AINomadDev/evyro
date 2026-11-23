//! Review comments on the Changes view.

use crate::{with_store, CmdResult};
use evyro_core::git;
use evyro_core::review::{review_prompt, NewReviewComment};
use serde_json::{json, Value};

#[tauri::command]
pub async fn list_review_comments(project_id: String, all: Option<bool>) -> CmdResult {
    with_store(move |s| {
        let cs = if all.unwrap_or(false) {
            s.list_review_comments(&project_id, &[])?
        } else {
            s.pending_review_comments(&project_id)?
        };
        Ok(json!(cs))
    })
    .await
}

#[tauri::command]
pub async fn add_review_comment(
    project_id: String,
    path: String,
    line: Option<u32>,
    snippet: Option<String>,
    body: String,
    task: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let t = match task.as_deref().filter(|t| !t.is_empty()) {
            Some(r) => Some(s.resolve_task(&p.id, r)?.id),
            None => None,
        };
        let head = git::head_sha(&p.root()).ok().flatten();
        let c = s.add_review_comment(
            &p.id,
            NewReviewComment {
                task_id: t.as_deref(),
                path: &path,
                line,
                snippet: snippet.as_deref().unwrap_or(""),
                body: &body,
                author: "human",
                commit_sha: head.as_deref(),
            },
        )?;
        Ok(json!(c))
    })
    .await
}

#[tauri::command]
pub async fn set_review_status(id: String, status: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.set_review_status(&id, &status, "human")?))).await
}

#[tauri::command]
pub async fn delete_review_comment(id: String) -> CmdResult {
    with_store(move |s| {
        s.delete_review_comment(&id)?;
        Ok(Value::Null)
    })
    .await
}

/// The open comments as one prompt. With `mark_sent`, they become "sent".
#[tauri::command]
pub async fn review_prompt_text(project_id: String, mark_sent: Option<bool>) -> CmdResult {
    with_store(move |s| {
        let cs = s.list_review_comments(&project_id, &["open"])?;
        let text = if cs.is_empty() {
            String::new()
        } else {
            review_prompt(&cs)
        };
        if mark_sent.unwrap_or(false) {
            for c in &cs {
                s.set_review_status(&c.id, "sent", "human")?;
            }
        }
        Ok(json!({"prompt": text, "count": cs.len()}))
    })
    .await
}
