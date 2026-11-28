//! Bounded, local desktop transcripts. Each file is replaced atomically.

use evyro_core::at_rest::{Sealer, Unreadable};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io;
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

    pub fn write(mut self, dir: &Path, id: &str, sealer: &Sealer) -> io::Result<Self> {
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
        let path = dir.join(format!("{id}.json"));
        // A transcript sealed with a key we do not have is never replaced.
        sealer.check_overwrite(&path)?;
        let data = sealer.seal(&serde_json::to_vec(&self)?)?;
        evyro_core::util::write_keeping_last_good(&path, &data, |b| parse(sealer, b).is_some())?;
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

/// A saved transcript, or `None` when it is too large, cut short, damaged or
/// encrypted with a key we do not have.
fn parse(sealer: &Sealer, bytes: &[u8]) -> Option<SavedChat> {
    let bytes = sealer.open(bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Why an encrypted transcript cannot be opened, when the key is the reason.
fn key_problem(sealer: &Sealer, path: &Path) -> Option<Unreadable> {
    [path.to_path_buf(), evyro_core::util::last_good_path(path)]
        .iter()
        .filter_map(|p| fs::read(p).ok())
        .find_map(|b| match sealer.open(&b) {
            Err(e @ (Unreadable::NoKey(_) | Unreadable::WrongKey)) => Some(e),
            _ => None,
        })
}

/// The chat ID a transcript file (or its last good copy) belongs to.
fn file_id(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let id = name
        .strip_suffix(".json.bak")
        .or(name.strip_suffix(".json"))?;
    (valid_id(id) && path.symlink_metadata().is_ok_and(|m| m.is_file())).then_some(id)
}

pub(crate) fn load(
    dir: &Path,
    keep: usize,
    sealer: &Sealer,
) -> io::Result<HashMap<String, SavedChat>> {
    fs::create_dir_all(dir)?;
    let paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let mut ids: Vec<&str> = paths.iter().filter_map(|p| file_id(p)).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut saved = HashMap::new();
    for id in ids.into_iter().rev() {
        // A transcript cut short by a crash or a full disk falls back to its
        // last good copy; other transcripts are unaffected. One that cannot be
        // decrypted is reported and left on disk, never deleted.
        let path = dir.join(format!("{id}.json"));
        let Some(record) = evyro_core::util::read_or_last_good(&path, |b| parse(sealer, b)) else {
            if let Some(why) = key_problem(sealer, &path) {
                eprintln!("evyro: saved chat {id} not loaded: {why}");
            }
            continue;
        };
        if record.info["id"] == id {
            if saved.len() >= keep {
                if let Err(e) = remove(dir, id, None) {
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

/// Delete a transcript and its last good copy, or with `into` move both into that folder.
pub(crate) fn remove(dir: &Path, id: &str, into: Option<&Path>) -> io::Result<()> {
    if !valid_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid chat ID",
        ));
    }
    let path = dir.join(format!("{id}.json"));
    for p in [evyro_core::util::last_good_path(&path), path] {
        if let Some(to) = into {
            evyro_core::trash::move_into(&p, to)?;
            continue;
        }
        match fs::remove_file(p) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            result => result?,
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("chat_")
        .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evyro_core::at_rest::{new_key, DataKey, MemoryKeys};
    use std::sync::Arc;

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
            .write(dir.path(), &id, &Sealer::plain())
            .unwrap();
        let file = fs::read_to_string(dir.path().join(format!("{id}.json"))).unwrap();
        assert!(!file.contains(&secret), "secret reached the saved file");
        let saved = load(dir.path(), MAX_SAVED, &Sealer::plain()).unwrap();
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
            .write(dir.path(), &id, &Sealer::plain())
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(dir.path()), 0o700);
            assert_eq!(mode(&dir.path().join(format!("{id}.json"))), 0o600);
        }
        let saved = load(dir.path(), MAX_SAVED, &Sealer::plain()).unwrap();
        assert_eq!(saved[&id].log, vec![json!({"t": "user", "text": "hello"})]);
        assert_eq!(saved[&id].info["history_truncated"], true);
    }

    #[test]
    fn a_cut_short_transcript_falls_back_to_its_last_good_copy() {
        last_good_recovery(&Sealer::plain());
    }

    #[test]
    fn last_good_recovery_works_with_encryption_on() {
        last_good_recovery(&encrypted(new_key()));
    }

    fn encrypted(key: DataKey) -> Sealer {
        Sealer::new(true, Arc::new(MemoryKeys::with(key)))
    }

    fn save(dir: &Path, id: &str, texts: &[&str], sealer: &Sealer) -> io::Result<SavedChat> {
        let log = texts
            .iter()
            .map(|t| json!({"t": "user", "text": t}))
            .collect();
        SavedChat::capture(json!({"id": id}), log, false).write(dir, id, sealer)
    }

    fn last_good_recovery(sealer: &Sealer) {
        let dir = tempfile::tempdir().unwrap();
        let save = |id: &str, texts: &[&str]| save(dir.path(), id, texts, sealer).unwrap();
        let load = || load(dir.path(), MAX_SAVED, sealer).unwrap();
        let (damaged, other) = (
            evyro_core::util::new_id("chat"),
            evyro_core::util::new_id("chat"),
        );
        save(&damaged, &["first"]);
        save(&damaged, &["first", "second"]);
        save(&other, &["untouched"]);
        let path = dir.path().join(format!("{damaged}.json"));
        let last_good = evyro_core::util::last_good_path(&path);
        assert!(last_good.exists(), "no last good copy kept");

        // The app stopped mid-write on a disk that does not honour renames: the
        // newest file is cut short.
        let full = fs::read(&path).unwrap();
        fs::write(&path, &full[..full.len() / 2]).unwrap();
        let saved = load();
        assert_eq!(
            saved[&damaged].log,
            vec![json!({"t": "user", "text": "first"})]
        );
        assert_eq!(saved[&other].log[0]["text"], "untouched");

        // Saving again keeps the good copy rather than the damaged file.
        save(&damaged, &["first", "second", "third"]);
        let kept = parse(sealer, &fs::read(&last_good).unwrap()).unwrap();
        assert_eq!(kept.log.len(), 1);
        assert_eq!(load()[&damaged].log.len(), 3);

        // Stopped between keeping the last good copy and the rename: only the
        // copy is left, and it still loads.
        fs::remove_file(&path).unwrap();
        assert_eq!(load()[&damaged].log.len(), 1);

        // Deleting a transcript also deletes its last good copy.
        remove(dir.path(), &damaged, None).unwrap();
        assert!(!last_good.exists());
        assert!(!load().contains_key(&damaged));
    }

    #[test]
    fn encrypted_transcripts_round_trip_and_hide_their_text() {
        let dir = tempfile::tempdir().unwrap();
        let sealer = encrypted(new_key());
        let id = evyro_core::util::new_id("chat");
        save(dir.path(), &id, &["deploy notes"], &sealer).unwrap();
        let file = fs::read(dir.path().join(format!("{id}.json"))).unwrap();
        assert!(Sealer::is_sealed(&file));
        assert!(!String::from_utf8_lossy(&file).contains("deploy notes"));
        let saved = load(dir.path(), MAX_SAVED, &sealer).unwrap();
        assert_eq!(saved[&id].log[0]["text"], "deploy notes");
    }

    #[test]
    fn plaintext_transcripts_still_load_and_are_encrypted_on_the_next_save() {
        let dir = tempfile::tempdir().unwrap();
        let id = evyro_core::util::new_id("chat");
        save(dir.path(), &id, &["old"], &Sealer::plain()).unwrap();
        let sealer = encrypted(new_key());
        let saved = load(dir.path(), MAX_SAVED, &sealer).unwrap();
        assert_eq!(saved[&id].log[0]["text"], "old");
        save(dir.path(), &id, &["old", "new"], &sealer).unwrap();
        let path = dir.path().join(format!("{id}.json"));
        assert!(Sealer::is_sealed(&fs::read(&path).unwrap()));
        // The plaintext copy became the last good copy and still loads.
        let last_good = evyro_core::util::last_good_path(&path);
        assert_eq!(
            parse(&sealer, &fs::read(last_good).unwrap())
                .unwrap()
                .log
                .len(),
            1
        );
    }

    #[test]
    fn a_missing_or_wrong_key_never_loads_replaces_or_deletes_a_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let id = evyro_core::util::new_id("chat");
        save(dir.path(), &id, &["kept"], &encrypted(new_key())).unwrap();
        let path = dir.path().join(format!("{id}.json"));
        let before = fs::read(&path).unwrap();
        for sealer in [
            encrypted(new_key()),
            Sealer::new(true, Arc::new(MemoryKeys::default())),
            Sealer::plain(),
        ] {
            // Not loaded, and so never trimmed by retention (keep 0).
            assert!(load(dir.path(), 0, &sealer).unwrap().is_empty());
            let Err(err) = save(dir.path(), &id, &["overwrite"], &sealer) else {
                panic!("replaced a transcript it cannot open");
            };
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
            assert!(err.to_string().contains("left on disk untouched"), "{err}");
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}
