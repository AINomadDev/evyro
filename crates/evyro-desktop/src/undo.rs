//! Undo for destructive actions. Deleted threads and chat transcripts go to the trash under
//! the data folder (see `evyro_core::trash`) and can be put back for a short while; a freed
//! task lease can be given back to the AI that had it (see `evyro_core::leases`). A stopped
//! program cannot be continued: the window offers to restart its thread instead.

use crate::{chat, with_store, CmdResult};
use evyro_core::at_rest::{CHATS_DIR, THREADS_DIR};
use evyro_core::paths::GlobalPaths;
use evyro_core::trash::{Batch, Trash, GRACE, UNDO_SECS};
use serde_json::{json, Value};
use std::path::PathBuf;

fn bad(e: impl std::fmt::Display) -> Value {
    json!({"code": "undo", "message": e.to_string()})
}

/// One recoverable delete of saved threads and chats.
pub(crate) struct Deleting {
    trash: Trash,
    batch: Batch,
}

impl Deleting {
    pub(crate) fn start() -> Result<Self, Value> {
        let paths = GlobalPaths::resolve().map_err(bad)?;
        let trash = Trash::at(&paths);
        let threads = paths.data_dir.join(THREADS_DIR);
        let chats = paths.data_dir.join(CHATS_DIR);
        let batch = trash
            .batch(&[(THREADS_DIR, &threads), (CHATS_DIR, &chats)])
            .map_err(bad)?;
        Ok(Self { trash, batch })
    }

    pub(crate) fn threads(&self) -> PathBuf {
        self.batch.folder(THREADS_DIR)
    }

    pub(crate) fn chats(&self) -> PathBuf {
        self.batch.folder(CHATS_DIR)
    }

    /// What the window needs to offer an undo (null when nothing was deleted). The files are
    /// removed for good once the grace period is over.
    pub(crate) fn finish(self) -> Value {
        if self.trash.discard_if_empty(&self.batch) {
            return Value::Null;
        }
        let trash = self.trash;
        std::thread::spawn(move || {
            std::thread::sleep(GRACE + std::time::Duration::from_secs(1));
            trash.purge(GRACE);
        });
        json!({"id": self.batch.id, "secs": UNDO_SECS})
    }
}

/// Remove for good what earlier runs left in the trash past the grace period.
pub(crate) fn purge_old() {
    if let Ok(paths) = GlobalPaths::resolve() {
        Trash::at(&paths).purge(GRACE);
    }
}

/// Put back the threads and chat transcripts of delete `id`.
#[tauri::command]
pub async fn trash_restore(chats: tauri::State<'_, chat::Chats>, id: String) -> CmdResult {
    let host = chats.host.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = GlobalPaths::resolve().map_err(bad)?;
        let files = Trash::at(&paths).restore(&id).map_err(bad)?;
        // Threads are listed from disk; chats are read back into the history.
        let chats = host.reload_saved();
        Ok(json!({"files": files, "chats": chats}))
    })
    .await
    .map_err(bad)?
}

/// Give the tasks freed from `session_ids` back to the AIs that had them. Each one is refused
/// when someone has claimed the task since; the reasons come back per session. Sessions with
/// nothing to give back are skipped.
#[tauri::command]
pub async fn release_undo(project_id: String, session_ids: Vec<String>) -> CmdResult {
    with_store(move |s| {
        let mut restored = Vec::new();
        let mut refused = Vec::new();
        for id in &session_ids {
            match s.undo_session_release(&project_id, id) {
                Ok(r) => restored.push(json!(r)),
                // The session held no task, or its task was never freed: nothing to give back.
                Err(evyro_core::Error::NotFound(_)) => {}
                Err(e) => refused.push(json!({"session_id": id, "message": e.to_string()})),
            }
        }
        Ok(json!({"restored": restored, "refused": refused}))
    })
    .await
}
