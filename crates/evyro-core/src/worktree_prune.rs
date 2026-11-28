//! Tidying finished worktrees across every project, and the daily automatic run the desktop
//! app and `evyro serve` start. Which worktrees go is decided by [`Store::prune_worktrees`].

use crate::checkout::{PruneOptions, Pruned};
use crate::db::Store;
use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use crate::util;
use serde::{Deserialize, Serialize};

/// The longest idle age a person can set, in days.
pub const MAX_IDLE_DAYS: u32 = 365;
const DEFAULT_IDLE_DAYS: u32 = 7;
/// How often the automatic run happens.
pub const AUTO_EVERY_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneSettings {
    /// Prune when the app or `evyro serve` starts and once a day after that.
    pub auto: bool,
    /// A worktree whose last session ended this many days ago counts as finished.
    pub idle_days: u32,
}

impl Default for PruneSettings {
    fn default() -> Self {
        Self {
            auto: true,
            idle_days: DEFAULT_IDLE_DAYS,
        }
    }
}

fn settings_path(paths: &GlobalPaths) -> std::path::PathBuf {
    paths.config_dir.join("worktree-prune.json")
}

fn last_run_path(paths: &GlobalPaths) -> std::path::PathBuf {
    paths.data_dir.join("worktree-prune-last")
}

impl PruneSettings {
    /// The saved setting, or the default when there is none or it cannot be read.
    pub fn load(paths: &GlobalPaths) -> Self {
        std::fs::read(settings_path(paths))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(|s| Self {
                idle_days: s.idle_days.clamp(1, MAX_IDLE_DAYS),
                ..s
            })
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &GlobalPaths) -> Result<()> {
        if !(1..=MAX_IDLE_DAYS).contains(&self.idle_days) {
            return Err(Error::Invalid(format!(
                "the age must be between 1 and {MAX_IDLE_DAYS} days"
            )));
        }
        std::fs::create_dir_all(&paths.config_dir)?;
        util::write_atomic(&settings_path(paths), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// One project's prune result.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectPrune {
    pub project_id: String,
    pub project: String,
    pub items: Vec<Pruned>,
}

/// The result of pruning every project.
#[derive(Debug, Clone, Serialize)]
pub struct PruneReport {
    pub dry_run: bool,
    pub projects: Vec<ProjectPrune>,
    /// Bytes freed, or that would be freed by a dry run.
    pub bytes: u64,
    /// Projects that could not be checked, with the reason.
    pub errors: Vec<String>,
}

impl Store {
    /// Prune the finished worktrees of every project that is not archived.
    pub fn prune_all_worktrees(&self, opts: PruneOptions) -> Result<PruneReport> {
        let mut report = PruneReport {
            dry_run: opts.dry_run,
            projects: Vec::new(),
            bytes: 0,
            errors: Vec::new(),
        };
        for p in self.list_projects(false, None)? {
            if !p.root().is_dir() || crate::git::head_sha(&p.root()).is_err() {
                continue;
            }
            match self.prune_worktrees(&p, opts) {
                Ok(items) if items.is_empty() => {}
                Ok(items) => {
                    report.bytes += items
                        .iter()
                        .filter(|i| i.removed || (opts.dry_run && i.reason.starts_with("would")))
                        .map(|i| i.bytes)
                        .sum::<u64>();
                    report.projects.push(ProjectPrune {
                        project_id: p.id.clone(),
                        project: p.name.clone(),
                        items,
                    });
                }
                Err(e) => report.errors.push(format!("{}: {e}", p.name)),
            }
        }
        Ok(report)
    }

    /// The automatic run: prune every project when it is turned on and the last automatic run
    /// was a day or more ago. Returns `None` when it did not run.
    pub fn auto_prune_worktrees(&self) -> Result<Option<PruneReport>> {
        let settings = PruneSettings::load(&self.paths);
        if !settings.auto {
            return Ok(None);
        }
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let last = std::fs::read_to_string(last_run_path(&self.paths))
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok());
        if last.is_some_and(|l| now - l < AUTO_EVERY_SECS && l <= now) {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.paths.data_dir)?;
        util::write_atomic(&last_run_path(&self.paths), now.to_string().as_bytes())?;
        self.prune_all_worktrees(PruneOptions {
            dry_run: false,
            idle_days: Some(settings.idle_days),
        })
        .map(Some)
    }
}

/// Run the automatic prune now and then hourly checks for the daily run, on a background
/// thread, until the process ends. Each check opens its own store.
pub fn spawn_auto_prune(open: fn() -> Result<Store>) {
    std::thread::spawn(move || loop {
        if let Ok(store) = open() {
            match store.auto_prune_worktrees() {
                Ok(Some(r)) if r.bytes > 0 => eprintln!(
                    "evyro: removed finished worktrees, freeing {}",
                    util::human_bytes(r.bytes)
                ),
                Err(e) => eprintln!("evyro: automatic worktree prune failed: {e}"),
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(60 * 60));
    });
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
        p.ensure().unwrap();
        (dir, p)
    }

    #[test]
    fn settings_default_on_and_round_trip() {
        let (_d, p) = paths();
        assert_eq!(PruneSettings::load(&p), PruneSettings::default());
        assert!(PruneSettings::default().auto);
        let s = PruneSettings {
            auto: false,
            idle_days: 30,
        };
        s.save(&p).unwrap();
        assert_eq!(PruneSettings::load(&p), s);
        assert!(PruneSettings {
            auto: true,
            idle_days: 0
        }
        .save(&p)
        .is_err());
    }

    #[test]
    fn the_automatic_run_happens_once_a_day_and_not_when_turned_off() {
        let (_d, p) = paths();
        let store = Store::open_at(&p.db_path(), p.clone()).unwrap();
        assert!(store.auto_prune_worktrees().unwrap().is_some());
        assert!(
            store.auto_prune_worktrees().unwrap().is_none(),
            "not again the same day"
        );
        std::fs::remove_file(last_run_path(&p)).unwrap();
        PruneSettings {
            auto: false,
            idle_days: 7,
        }
        .save(&p)
        .unwrap();
        assert!(store.auto_prune_worktrees().unwrap().is_none());
    }
}
