//! How many saved threads and chat transcripts the desktop keeps. Older ones are deleted.

use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use crate::util;
use serde::{Deserialize, Serialize};

/// The most a person can ask to keep, for each kind.
pub const MAX_KEEP: usize = 500;
const DEFAULT_KEEP: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retention {
    /// Saved terminal threads kept after they end.
    pub threads: usize,
    /// Saved chat transcripts kept after they end.
    pub chats: usize,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            threads: DEFAULT_KEEP,
            chats: DEFAULT_KEEP,
        }
    }
}

fn path(paths: &GlobalPaths) -> std::path::PathBuf {
    paths.config_dir.join("retention.json")
}

impl Retention {
    /// The saved setting, or the default when there is none or it cannot be read.
    pub fn load(paths: &GlobalPaths) -> Self {
        std::fs::read(path(paths))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(|r| Self {
                threads: r.threads.min(MAX_KEEP),
                chats: r.chats.min(MAX_KEEP),
            })
            .unwrap_or_default()
    }

    pub fn validate(&self) -> Result<()> {
        if self.threads > MAX_KEEP || self.chats > MAX_KEEP {
            return Err(Error::Invalid(format!(
                "keep at most {MAX_KEEP} saved threads and {MAX_KEEP} saved chats"
            )));
        }
        Ok(())
    }

    pub fn save(&self, paths: &GlobalPaths) -> Result<()> {
        self.validate()?;
        std::fs::create_dir_all(&paths.config_dir)?;
        util::write_atomic(&path(paths), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> (tempfile::TempDir, GlobalPaths) {
        let dir = tempfile::tempdir().unwrap();
        let p = GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        (dir, p)
    }

    #[test]
    fn defaults_when_unset_and_round_trips_a_saved_choice() {
        let (_d, p) = paths();
        assert_eq!(Retention::load(&p), Retention::default());
        let r = Retention {
            threads: 10,
            chats: 0,
        };
        r.save(&p).unwrap();
        assert_eq!(Retention::load(&p), r);
    }

    #[test]
    fn refuses_values_over_the_limit_and_clamps_a_hand_edited_file() {
        let (_d, p) = paths();
        let too_many = Retention {
            threads: MAX_KEEP + 1,
            chats: 1,
        };
        assert!(matches!(too_many.save(&p), Err(Error::Invalid(_))));
        assert!(!path(&p).exists(), "nothing is written for a refused value");
        std::fs::create_dir_all(&p.config_dir).unwrap();
        std::fs::write(path(&p), r#"{"threads": 99999, "chats": 5}"#).unwrap();
        assert_eq!(
            Retention::load(&p),
            Retention {
                threads: MAX_KEEP,
                chats: 5
            }
        );
        std::fs::write(path(&p), "not json").unwrap();
        assert_eq!(Retention::load(&p), Retention::default());
    }
}
