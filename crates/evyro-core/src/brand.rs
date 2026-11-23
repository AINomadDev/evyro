//! Central product identity. Never hard-code the product name elsewhere.

pub const PRODUCT_NAME: &str = "Evyro";
pub const CLI_NAME: &str = "evyro";
pub const TAGLINE: &str = "Switch agents. Keep building.";
/// Directory created inside each registered repository.
pub const PROJECT_DIR: &str = ".evyro";
/// Environment variable that overrides the global data directory (tests, portable installs).
pub const HOME_ENV: &str = "EVYRO_HOME";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
