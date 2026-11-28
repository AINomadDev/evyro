//! Recoverable deletes for saved threads and chat transcripts.
//!
//! Instead of deleting files, a delete moves them into a batch folder under
//! `<data dir>/trash/<batch id>/<place>/`, next to a manifest naming the folder each place came
//! from. Until the batch is purged (after [`GRACE`]) it can be put back with [`Trash::restore`].
//! Files are moved as they are, so encrypted files stay encrypted and last-good copies
//! (`.bak`) travel with the file they belong to.

use crate::paths::GlobalPaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TRASH_DIR: &str = "trash";
/// How long the desktop offers an undo after a delete.
pub const UNDO_SECS: u64 = 10;
/// How long deleted files stay restorable before they are purged for good.
pub const GRACE: Duration = Duration::from_secs(60);

const MANIFEST: &str = "manifest.json";

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    /// Seconds since the Unix epoch when the batch was made.
    created: u64,
    /// Each place in the batch and the folder its files came from.
    places: BTreeMap<String, PathBuf>,
}

/// One delete: the files it moved can be restored together.
#[derive(Debug, Clone)]
pub struct Batch {
    pub id: String,
    dir: PathBuf,
}

impl Batch {
    /// The folder that receives the files deleted from `place`.
    pub fn folder(&self, place: &str) -> PathBuf {
        self.dir.join(place)
    }
}

#[derive(Debug, Clone)]
pub struct Trash {
    root: PathBuf,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Move `file` into the folder `into`, keeping its name. A missing file is not an error.
/// Returns whether anything was moved.
pub fn move_into(file: &Path, into: &Path) -> io::Result<bool> {
    let Some(name) = file.file_name() else {
        return Ok(false);
    };
    match std::fs::symlink_metadata(file) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    private_dir(into)?;
    crate::util::rename_replacing(file, &into.join(name))?;
    Ok(true)
}

impl Trash {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The trash under the Evyro data folder.
    pub fn at(paths: &GlobalPaths) -> Self {
        Self::new(paths.data_dir.join(TRASH_DIR))
    }

    /// Start a batch for files deleted from each `(place, folder)`.
    pub fn batch(&self, places: &[(&str, &Path)]) -> io::Result<Batch> {
        let id = crate::util::new_id("del");
        let dir = self.root.join(&id);
        private_dir(&self.root)?;
        private_dir(&dir)?;
        let manifest = Manifest {
            created: now_secs(),
            places: places
                .iter()
                .map(|(p, d)| ((*p).to_owned(), d.to_path_buf()))
                .collect(),
        };
        crate::util::write_atomic(&dir.join(MANIFEST), &serde_json::to_vec_pretty(&manifest)?)?;
        Ok(Batch { id, dir })
    }

    /// Drop a batch that received no files. Returns whether it was dropped.
    pub fn discard_if_empty(&self, batch: &Batch) -> bool {
        let empty = std::fs::read_dir(&batch.dir).is_ok_and(|rd| {
            rd.flatten().all(|e| {
                e.file_name() == MANIFEST
                    || std::fs::read_dir(e.path()).is_ok_and(|mut d| d.next().is_none())
            })
        });
        if empty {
            let _ = std::fs::remove_dir_all(&batch.dir);
        }
        empty
    }

    /// Put every file of batch `id` back where it came from and remove the batch. A file that
    /// exists again at its old place is left as it is there. Returns how many files came back.
    pub fn restore(&self, id: &str) -> io::Result<usize> {
        let gone = || {
            io::Error::new(
                io::ErrorKind::NotFound,
                "the deleted items were already removed for good",
            )
        };
        if !valid_id(id) {
            return Err(gone());
        }
        let dir = self.root.join(id);
        let manifest: Manifest = std::fs::read(dir.join(MANIFEST))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(gone)?;
        let mut restored = 0;
        for (place, to) in &manifest.places {
            let Ok(rd) = std::fs::read_dir(dir.join(place)) else {
                continue;
            };
            private_dir(to)?;
            for entry in rd.flatten() {
                let dest = to.join(entry.file_name());
                if dest.exists() {
                    continue;
                }
                crate::util::rename_replacing(&entry.path(), &dest)?;
                restored += 1;
            }
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(restored)
    }

    /// Remove for good every batch made more than `older_than` ago. Returns how many went.
    pub fn purge(&self, older_than: Duration) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let now = now_secs();
        let mut purged = 0;
        for entry in rd.flatten() {
            let dir = entry.path();
            let created = std::fs::read(dir.join(MANIFEST))
                .ok()
                .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
                .map(|m| m.created)
                // No readable manifest: nothing can be restored, so it goes now.
                .unwrap_or(0);
            if now.saturating_sub(created) >= older_than.as_secs()
                && std::fs::remove_dir_all(&dir).is_ok()
            {
                purged += 1;
            }
        }
        purged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Trash, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let trash = Trash::new(tmp.path().join(TRASH_DIR));
        let threads = tmp.path().join("threads");
        std::fs::create_dir_all(&threads).unwrap();
        (tmp, trash, threads)
    }

    #[test]
    fn deleted_files_come_back_on_restore() {
        let (_tmp, trash, threads) = setup();
        for name in ["a.json", "a.json.bak", "a.log"] {
            std::fs::write(threads.join(name), name).unwrap();
        }
        let batch = trash.batch(&[("threads", &threads)]).unwrap();
        for name in ["a.json", "a.json.bak", "a.log", "missing.log"] {
            move_into(&threads.join(name), &batch.folder("threads")).unwrap();
        }
        assert!(!threads.join("a.json").exists());
        assert_eq!(trash.restore(&batch.id).unwrap(), 3);
        assert_eq!(
            std::fs::read(threads.join("a.json.bak")).unwrap(),
            b"a.json.bak"
        );
        // Restored once; the batch is gone.
        assert_eq!(
            trash.restore(&batch.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn a_newer_file_at_the_old_place_is_kept() {
        let (_tmp, trash, threads) = setup();
        std::fs::write(threads.join("a.json"), "old").unwrap();
        let batch = trash.batch(&[("threads", &threads)]).unwrap();
        move_into(&threads.join("a.json"), &batch.folder("threads")).unwrap();
        std::fs::write(threads.join("a.json"), "new").unwrap();
        assert_eq!(trash.restore(&batch.id).unwrap(), 0);
        assert_eq!(std::fs::read(threads.join("a.json")).unwrap(), b"new");
    }

    #[test]
    fn batches_are_purged_after_the_grace_period() {
        let (_tmp, trash, threads) = setup();
        std::fs::write(threads.join("a.json"), "x").unwrap();
        let batch = trash.batch(&[("threads", &threads)]).unwrap();
        move_into(&threads.join("a.json"), &batch.folder("threads")).unwrap();
        assert_eq!(trash.purge(GRACE), 0, "still within the grace period");
        assert!(trash.restore(&batch.id).is_ok());

        let batch = trash.batch(&[("threads", &threads)]).unwrap();
        move_into(&threads.join("a.json"), &batch.folder("threads")).unwrap();
        assert_eq!(trash.purge(Duration::ZERO), 1);
        assert!(trash.restore(&batch.id).is_err());
        assert!(!threads.join("a.json").exists());
    }

    #[test]
    fn empty_batches_are_dropped_and_bad_ids_refused() {
        let (_tmp, trash, threads) = setup();
        let batch = trash.batch(&[("threads", &threads)]).unwrap();
        assert!(trash.discard_if_empty(&batch));
        assert!(trash.restore(&batch.id).is_err());
        assert!(trash.restore("../threads").is_err());
    }
}
