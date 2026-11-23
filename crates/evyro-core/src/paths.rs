//! Platform-appropriate global locations. `EVYRO_HOME` overrides everything,
//! which keeps tests hermetic and allows portable installs.
//!
//! Defaults (via the `directories` crate):
//! - macOS:   ~/Library/Application Support/dev.Evyro.evyro
//! - Linux:   $XDG_DATA_HOME/evyro (usually ~/.local/share/evyro)
//! - Windows: %APPDATA%\Evyro\evyro\data

use crate::brand;
use crate::error::{Error, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GlobalPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl GlobalPaths {
    pub fn resolve() -> Result<Self> {
        if let Ok(home) = std::env::var(brand::HOME_ENV) {
            if !home.is_empty() {
                let root = PathBuf::from(home);
                return Ok(Self {
                    data_dir: root.join("data"),
                    config_dir: root.join("config"),
                });
            }
        }
        let dirs = directories::ProjectDirs::from("dev", brand::PRODUCT_NAME, brand::CLI_NAME)
            .ok_or_else(|| {
                Error::Other("cannot determine a home directory for application data".into())
            })?;
        Ok(Self {
            data_dir: dirs.data_dir().to_path_buf(),
            config_dir: dirs.config_dir().to_path_buf(),
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("evyro.db")
    }
    pub fn key_path(&self) -> PathBuf {
        self.data_dir.join("contract.key")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }
}
