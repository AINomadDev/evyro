//! Bounded, local desktop transcripts. Each file is replaced atomically.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) const MAX_SAVED: usize = 60;
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Serialize, Deserialize)]
pub(crate) struct SavedChat {
    pub info: Value,
    pub log: Vec<Value>,
}

#[derive(Serialize)]
pub struct SearchHit {
    id: String,
    title: String,
    message_index: usize,
    excerpt: String,
}

impl SavedChat {
    pub fn matches(&self, pattern: &regex::Regex, limit: usize) -> Vec<SearchHit> {
        let id = self.info["id"].as_str().unwrap_or_default();
        let title = self.info["title"].as_str().unwrap_or("Saved chat");
        self.messages()
            .into_iter()
            .filter_map(|(message_index, text)| {
                let found = pattern.find(&text)?;
                let start = text[..found.start()]
                    .char_indices()
                    .rev()
                    .nth(44)
                    .map_or(0, |(i, _)| i);
                let end = text[found.end()..]
                    .char_indices()
                    .nth(70)
                    .map_or(text.len(), |(i, _)| found.end() + i);
                let excerpt = format!(
                    "{}{}{}",
                    if start > 0 { "…" } else { "" },
                    text[start..end].replace(['\r', '\n'], " "),
                    if end < text.len() { "…" } else { "" }
                );
                Some(SearchHit {
                    id: id.to_owned(),
                    title: title.to_owned(),
                    message_index,
                    excerpt,
                })
            })
            .take(limit)
            .collect()
    }

    // Join streamed replies at the same boundaries as the desktop chat renderer.
    fn messages(&self) -> Vec<(usize, Cow<'_, str>)> {
        let mut messages = Vec::new();
        let mut replying = None;
        let mut has_plan = false;
        for (i, item) in self.log.iter().enumerate() {
            let update = &item["update"];
            let kind = item["t"].as_str();
            let update_kind = update["sessionUpdate"].as_str();
            if kind == Some("update") && update_kind == Some("agent_message_chunk") {
                let (_, text) = replying.get_or_insert_with(|| (i, String::new()));
                if update["content"]["type"] == "text" {
                    text.push_str(update["content"]["text"].as_str().unwrap_or_default());
                }
                continue;
            }
            let plan_added = kind == Some("update") && update_kind == Some("plan") && !has_plan;
            let ends_reply = match kind {
                Some("user" | "turn_end" | "error" | "exit" | "permission") => true,
                Some("file") => item["op"] != "read",
                Some("update") => {
                    plan_added || matches!(update_kind, Some("tool_call" | "agent_thought_chunk"))
                }
                _ => false,
            };
            if ends_reply {
                if let Some((start, text)) = replying.take() {
                    messages.push((start, Cow::Owned(text)));
                }
            }
            if kind == Some("user") {
                has_plan = false;
                if let Some(text) = item["text"].as_str() {
                    messages.push((i, Cow::Borrowed(text)));
                }
            } else if plan_added {
                has_plan = true;
            }
        }
        if let Some((start, text)) = replying {
            messages.push((start, Cow::Owned(text)));
        }
        messages
    }

    pub fn capture(mut info: Value, log: Vec<Value>, truncated: bool) -> Self {
        info["archived"] = json!(true);
        info["running"] = json!(false);
        info["busy"] = json!(false);
        info["waiting"] = json!(0);
        info["draft_available"] = json!(false);
        info["switching_model"] = json!(false);
        info["history_truncated"] = json!(truncated);
        if info["state"] != "failed" {
            info["state"] = json!("ended");
        }
        Self { info, log }
    }

    pub fn write(mut self, dir: &Path, id: &str) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        private(dir, 0o700)?;
        let log = std::mem::take(&mut self.log);
        let overhead = serde_json::to_vec(&self)?.len() as u64;
        if overhead > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chat metadata exceeds archive limit",
            ));
        }
        let capacity = MAX_BYTES - overhead;
        let mut remaining = capacity;
        // Saved transcripts never keep secrets the agent or the person pasted.
        let mut found = Vec::new();
        for item in log.into_iter().rev().map(|mut item| {
            evyro_core::redact::redact_json(&mut item, &mut found);
            item
        }) {
            // Reserve a byte for the comma between entries.
            let bytes = serde_json::to_vec(&item)?.len() as u64 + 1;
            if bytes > capacity {
                self.info["history_truncated"] = json!(true);
                continue;
            }
            if bytes > remaining {
                self.info["history_truncated"] = json!(true);
                break;
            }
            remaining -= bytes;
            self.log.push(item);
        }
        self.log.reverse();
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(&mut temp, &self)?;
        temp.flush()?;
        let path = dir.join(format!("{id}.json"));
        temp.persist(&path)?;
        private(&path, 0o600)?;
        Ok(self)
    }
}

/// Transcripts can hold secrets, so only the current user may read them (Unix).
fn private(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

pub(crate) fn load(dir: &Path, keep: usize) -> io::Result<HashMap<String, SavedChat>> {
    fs::create_dir_all(dir)?;
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort_unstable();
    let mut saved = HashMap::new();
    for path in paths.into_iter().rev() {
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|id| valid_id(id))
        else {
            continue;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("json")
            || !path
                .symlink_metadata()
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES)
        {
            continue;
        }
        let Some(record) = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SavedChat>(&bytes).ok())
        else {
            continue;
        };
        if record.info["id"] == id {
            if saved.len() >= keep {
                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("evyro: could not trim old chat history: {e}");
                }
                continue;
            }
            let truncated = record.info["history_truncated"] == true;
            saved.insert(
                id.to_owned(),
                SavedChat::capture(record.info, record.log, truncated),
            );
        }
    }
    Ok(saved)
}

pub(crate) fn remove(dir: &Path, id: &str) -> io::Result<()> {
    if !valid_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid chat ID",
        ));
    }
    match fs::remove_file(dir.join(format!("{id}.json"))) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("chat_")
        .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_joins_reply_chunks_and_excludes_thoughts_and_context_packets() {
        let reply = |text| json!({"t": "update", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}});
        let log = vec![
            json!({"t": "user", "text": "Inspect this"}),
            json!({"t": "user", "text": {"context": "hidden context"}}),
            reply("Looked at the él"),
            json!({"t": "file", "op": "read", "path": "src/lib.rs"}),
            reply("ément in src"),
            json!({"t": "update", "update": {"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "hidden thought"}}}),
            reply("Next step"),
        ];
        let chat = SavedChat::capture(json!({"id": "chat", "title": "Test"}), log, false);
        let pattern = regex::RegexBuilder::new("ÉLÉMENT")
            .case_insensitive(true)
            .build()
            .unwrap();
        let hits = chat.matches(&pattern, 60);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_index, 2);
        assert!(hits[0].excerpt.contains("élément"));
        assert!(chat
            .matches(&regex::Regex::new("hidden|srcNext").unwrap(), 60)
            .is_empty());
        assert_eq!(
            chat.matches(&regex::Regex::new("Inspect this").unwrap(), 60)[0].message_index,
            0
        );
    }

    #[test]
    fn saved_transcripts_do_not_keep_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let id = evyro_core::util::new_id("chat");
        let secret = format!("ghp_{}", "B".repeat(36));
        let log = vec![json!({"t": "user", "text": format!("my token is {secret}")})];
        SavedChat::capture(json!({"id": id}), log, false)
            .write(dir.path(), &id)
            .unwrap();
        let file = fs::read_to_string(dir.path().join(format!("{id}.json"))).unwrap();
        assert!(!file.contains(&secret), "secret reached the saved file");
        let saved = load(dir.path(), MAX_SAVED).unwrap();
        assert!(saved[&id].log[0]["text"]
            .as_str()
            .unwrap()
            .contains("[REDACTED:github_token]"));
    }

    #[test]
    fn oversized_entries_do_not_remove_the_rest_of_the_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let id = evyro_core::util::new_id("chat");
        let log = vec![
            json!({"t": "user", "text": "hello"}),
            json!({"t": "update", "text": "x".repeat(MAX_BYTES as usize)}),
        ];
        SavedChat::capture(json!({"id": id}), log, false)
            .write(dir.path(), &id)
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(dir.path()), 0o700);
            assert_eq!(mode(&dir.path().join(format!("{id}.json"))), 0o600);
        }
        let saved = load(dir.path(), MAX_SAVED).unwrap();
        assert_eq!(saved[&id].log, vec![json!({"t": "user", "text": "hello"})]);
        assert_eq!(saved[&id].info["history_truncated"], true);
    }
}
