//! Optional encryption at rest for saved output: terminal threads and chat
//! transcripts the desktop app keeps on disk.
//!
//! Off by default. When on, new saves are sealed with ChaCha20-Poly1305 under a
//! random 256-bit data key kept in the OS keychain (macOS Keychain). Plaintext
//! files keep loading, so turning it on never hides older output, and
//! [`Sealer::migrate_dir`] converts existing files either way.
//!
//! A sealed file is `MAGIC | key id (8) | nonce (12) | ciphertext+tag`. The key
//! id tells a file sealed with another key apart from a damaged one: a damaged
//! file may be replaced by a new save (its last good copy is kept), but a file
//! whose key is missing or different is never overwritten or deleted.

use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use crate::util;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::ChaCha20Poly1305;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAGIC: &[u8; 8] = b"EVYENC1\n";
const KEY_ID: usize = 8;
const NONCE: usize = 12;
const HEADER: usize = MAGIC.len() + KEY_ID + NONCE;

/// Keychain item holding the data key (service; the account is `evyro`).
pub const KEYCHAIN_SERVICE: &str = "dev.evyro.saved-output";

/// Folders under the data folder that hold saved output.
pub const THREADS_DIR: &str = "threads";
pub const CHATS_DIR: &str = "chat-history";

pub type DataKey = [u8; 32];

/// Where the data key lives. Tests use [`MemoryKeys`]; the app uses [`Keychain`].
pub trait KeyStore: Send + Sync {
    /// The stored key, or `None` when there is none yet.
    fn get(&self) -> Result<Option<DataKey>>;
    fn put(&self, key: &DataKey) -> Result<()>;
}

/// The macOS Keychain, through the `security` tool (as `key_keychain` targets do).
pub struct Keychain;

impl KeyStore for Keychain {
    fn get(&self) -> Result<Option<DataKey>> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported(
                "encrypting saved output needs the macOS Keychain; it is not available on this system yet".into(),
            ));
        }
        let out = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                "evyro",
                "-w",
            ])
            .stderr(std::process::Stdio::null())
            .output()?;
        if !out.status.success() {
            return Ok(None);
        }
        parse_key(String::from_utf8_lossy(&out.stdout).trim()).map(Some)
    }

    fn put(&self, key: &DataKey) -> Result<()> {
        use std::io::Write;
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported(
                "encrypting saved output needs the macOS Keychain; it is not available on this system yet".into(),
            ));
        }
        // Commands go through stdin (`security -i`) so the key never appears
        // in the process list.
        let mut child = std::process::Command::new("security")
            .arg("-i")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        child.stdin.take().unwrap().write_all(
            format!(
                "add-generic-password -U -s {KEYCHAIN_SERVICE} -a evyro -w {}\n",
                util::hex(key)
            )
            .as_bytes(),
        )?;
        child.wait()?;
        // `security -i` exits 0 even when a command fails, so read it back.
        match self.get()? {
            Some(k) if k == *key => Ok(()),
            _ => Err(Error::Other(
                "could not save the saved-output key in the Keychain".into(),
            )),
        }
    }
}

fn parse_key(hex: &str) -> Result<DataKey> {
    let bad = || Error::Invalid("the saved-output key in the keychain is malformed".into());
    if hex.len() != 64 {
        return Err(bad());
    }
    let mut key = [0u8; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2).ok_or_else(bad)?, 16)
            .map_err(|_| bad())?;
    }
    Ok(key)
}

/// A key store in memory, for tests.
#[derive(Default)]
pub struct MemoryKeys(Mutex<Option<DataKey>>);

impl MemoryKeys {
    pub fn with(key: DataKey) -> Self {
        Self(Mutex::new(Some(key)))
    }
}

impl KeyStore for MemoryKeys {
    fn get(&self) -> Result<Option<DataKey>> {
        Ok(*self.0.lock().unwrap())
    }
    fn put(&self, key: &DataKey) -> Result<()> {
        *self.0.lock().unwrap() = Some(*key);
        Ok(())
    }
}

/// A new random data key.
pub fn new_key() -> DataKey {
    ChaCha20Poly1305::generate_key(&mut OsRng).into()
}

/// The person's choice, kept in `<config>/saved-output.json`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Encrypt saved terminal output and chat transcripts.
    #[serde(default)]
    pub encrypt: bool,
}

fn settings_path(paths: &GlobalPaths) -> PathBuf {
    paths.config_dir.join("saved-output.json")
}

impl Settings {
    /// The saved setting; off when there is none or it cannot be read.
    pub fn load(paths: &GlobalPaths) -> Self {
        std::fs::read(settings_path(paths))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &GlobalPaths) -> Result<()> {
        std::fs::create_dir_all(&paths.config_dir)?;
        util::write_atomic(&settings_path(paths), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// Why a sealed file could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// No key could be read (none stored, or the keychain refused).
    NoKey(String),
    /// Sealed with a different key than the one stored.
    WrongKey,
    /// Sealed with this key but cut short or altered.
    Damaged,
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreadable::NoKey(why) => write!(
                f,
                "this saved output is encrypted and its key is not available ({why}); it was left on disk untouched"
            ),
            Unreadable::WrongKey => f.write_str(
                "this saved output was encrypted with a different key than the one in the keychain; it was left on disk untouched",
            ),
            Unreadable::Damaged => f.write_str("this encrypted saved output is damaged"),
        }
    }
}

impl From<Unreadable> for std::io::Error {
    fn from(e: Unreadable) -> Self {
        let kind = match e {
            Unreadable::Damaged => std::io::ErrorKind::InvalidData,
            _ => std::io::ErrorKind::PermissionDenied,
        };
        std::io::Error::new(kind, e.to_string())
    }
}

/// Seals saved output on write and opens it on read. Cheap to clone.
#[derive(Clone, Default)]
pub struct Sealer {
    encrypt: bool,
    keys: Option<Arc<dyn KeyStore>>,
    /// The key, or why there is none, once asked. The keychain is asked at
    /// most once, and only when a key is needed, so autosaves never prompt
    /// again and again.
    key: Arc<Mutex<Option<std::result::Result<DataKey, Unreadable>>>>,
}

impl Sealer {
    /// Never encrypts; sealed files cannot be opened.
    pub fn plain() -> Self {
        Self::default()
    }

    /// Encrypts new writes when `encrypt`; sealed files are opened with the
    /// key in `keys` either way.
    pub fn new(encrypt: bool, keys: Arc<dyn KeyStore>) -> Self {
        Self {
            encrypt,
            keys: Some(keys),
            key: Default::default(),
        }
    }

    /// The person's setting with the OS keychain.
    pub fn from_settings(paths: &GlobalPaths) -> Self {
        Self::new(Settings::load(paths).encrypt, Arc::new(Keychain))
    }

    pub fn encrypts(&self) -> bool {
        self.encrypt
    }

    fn key(&self) -> std::result::Result<DataKey, Unreadable> {
        self.key
            .lock()
            .unwrap()
            .get_or_insert_with(|| match self.keys.as_ref().map(|k| k.get()) {
                None => Err(Unreadable::NoKey("encryption is not set up".into())),
                Some(Ok(Some(k))) => Ok(k),
                Some(Ok(None)) => Err(Unreadable::NoKey(
                    "no key in the keychain; run `evyro saved-output status`".into(),
                )),
                Some(Err(e)) => Err(Unreadable::NoKey(e.to_string())),
            })
            .clone()
    }

    /// Whether `bytes` were written sealed.
    pub fn is_sealed(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    /// `plain` as it should be written: sealed when encryption is on.
    pub fn seal(&self, plain: &[u8]) -> std::io::Result<Vec<u8>> {
        if !self.encrypt {
            return Ok(plain.to_vec());
        }
        let key = self.key()?;
        let id = key_id(&key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut aad = MAGIC.to_vec();
        aad.extend_from_slice(&id);
        let sealed = ChaCha20Poly1305::new(&key.into())
            .encrypt(
                &nonce,
                Payload {
                    msg: plain,
                    aad: &aad,
                },
            )
            .map_err(|_| std::io::Error::other("could not encrypt saved output"))?;
        let mut out = Vec::with_capacity(HEADER + sealed.len());
        out.extend_from_slice(&aad);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// The plaintext of a stored file. Plaintext files pass through unchanged.
    pub fn open<'a>(&self, stored: &'a [u8]) -> std::result::Result<Cow<'a, [u8]>, Unreadable> {
        if !Self::is_sealed(stored) {
            return Ok(Cow::Borrowed(stored));
        }
        if stored.len() < HEADER {
            return Err(Unreadable::Damaged);
        }
        let key = self.key()?;
        let (aad, rest) = stored.split_at(MAGIC.len() + KEY_ID);
        if aad[MAGIC.len()..] != key_id(&key) {
            return Err(Unreadable::WrongKey);
        }
        let (nonce, sealed) = rest.split_at(NONCE);
        ChaCha20Poly1305::new(&key.into())
            .decrypt(nonce.into(), Payload { msg: sealed, aad })
            .map(Cow::Owned)
            .map_err(|_| Unreadable::Damaged)
    }

    /// Refuse to replace `path` when it holds output this key cannot open. A
    /// missing, plaintext or damaged file may be replaced.
    pub fn check_overwrite(&self, path: &Path) -> std::io::Result<()> {
        match std::fs::read(path) {
            Ok(bytes) => match self.open(&bytes) {
                Err(e @ (Unreadable::NoKey(_) | Unreadable::WrongKey)) => Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("not replacing {}: {e}", path.display()),
                )),
                _ => Ok(()),
            },
            Err(_) => Ok(()),
        }
    }

    /// Bring every saved-output file in `dir` in line with this sealer:
    /// encrypt plaintext files when encryption is on, decrypt sealed ones when
    /// it is off. Files that cannot be opened are listed and left untouched.
    pub fn migrate_dir(&self, dir: &Path) -> std::io::Result<Migration> {
        let mut report = Migration::default();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(report);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_saved_output(&name) || !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            if Self::is_sealed(&bytes) == self.encrypt {
                continue;
            }
            let data = if self.encrypt {
                self.seal(&bytes)?
            } else {
                match self.open(&bytes) {
                    Ok(plain) => plain.into_owned(),
                    Err(e) => {
                        report.skipped.push((path, e.to_string()));
                        continue;
                    }
                }
            };
            replace_private(&path, &data)?;
            report.changed += 1;
        }
        Ok(report)
    }
}

/// What [`Sealer::migrate_dir`] did.
#[derive(Debug, Default)]
pub struct Migration {
    pub changed: usize,
    /// Files left as they were, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Saved-output files: thread details and output, chat transcripts and their
/// last good copies. Credit counters (`*.usage.json`) are not output.
pub fn is_saved_output(name: &str) -> bool {
    !name.ends_with(".usage.json")
        && [".json", ".json.bak", ".log"]
            .iter()
            .any(|ext| name.ends_with(ext))
}

/// Count sealed and plaintext saved-output files in `dir`.
pub fn count(dir: &Path) -> (usize, usize) {
    let (mut sealed, mut plain) = (0, 0);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if !is_saved_output(&entry.file_name().to_string_lossy()) {
            continue;
        }
        match std::fs::read(entry.path()) {
            Ok(b) if Sealer::is_sealed(&b) => sealed += 1,
            Ok(_) => plain += 1,
            Err(_) => {}
        }
    }
    (sealed, plain)
}

/// Saved output on this machine and whether it is encrypted.
#[derive(Debug, Serialize)]
pub struct Status {
    pub encrypt: bool,
    pub encrypted_files: usize,
    pub plain_files: usize,
    /// Whether the key can be read. Only checked when encryption is on or
    /// encrypted files exist, so the keychain is not asked needlessly.
    pub key_available: Option<bool>,
    /// Files left as they were by the last change, with the reason.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<(PathBuf, String)>,
}

fn output_dirs(paths: &GlobalPaths) -> [PathBuf; 2] {
    [THREADS_DIR, CHATS_DIR].map(|d| paths.data_dir.join(d))
}

pub fn status(paths: &GlobalPaths, keys: &dyn KeyStore) -> Status {
    let (mut sealed, mut plain) = (0, 0);
    for dir in output_dirs(paths) {
        let (s, p) = count(&dir);
        sealed += s;
        plain += p;
    }
    let encrypt = Settings::load(paths).encrypt;
    Status {
        encrypt,
        encrypted_files: sealed,
        plain_files: plain,
        key_available: (encrypt || sealed > 0).then(|| matches!(keys.get(), Ok(Some(_)))),
        skipped: Vec::new(),
    }
}

/// Turn encryption of saved output on or off and convert the files already
/// saved. Turning it on creates the data key in `keys` when there is none.
/// Turning it off keeps the key, so files that could not be converted stay
/// readable. Quit the desktop app first: it reads the setting when it starts.
pub fn set_encryption(
    paths: &GlobalPaths,
    encrypt: bool,
    keys: Arc<dyn KeyStore>,
) -> Result<Status> {
    if encrypt && keys.get()?.is_none() {
        keys.put(&new_key())?;
    }
    Settings { encrypt }.save(paths)?;
    let sealer = Sealer::new(encrypt, keys.clone());
    let mut skipped = Vec::new();
    for dir in output_dirs(paths) {
        skipped.extend(sealer.migrate_dir(&dir)?.skipped);
    }
    Ok(Status {
        skipped,
        ..status(paths, keys.as_ref())
    })
}

fn key_id(key: &DataKey) -> [u8; KEY_ID] {
    let mut h = Sha256::new();
    h.update(b"evyro saved-output key id\0");
    h.update(key);
    h.finalize()[..KEY_ID].try_into().unwrap()
}

/// Replace `path` through a synced, owner-only temporary file.
fn replace_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealer(encrypt: bool, key: DataKey) -> Sealer {
        Sealer::new(encrypt, Arc::new(MemoryKeys::with(key)))
    }

    #[test]
    fn round_trips_and_the_file_does_not_hold_the_plaintext() {
        let s = sealer(true, new_key());
        let sealed = s.seal(b"secret output").unwrap();
        assert!(Sealer::is_sealed(&sealed));
        assert!(!sealed.windows(6).any(|w| w == b"secret"));
        assert_eq!(&*s.open(&sealed).unwrap(), b"secret output");
        // Each write uses a fresh nonce.
        assert_ne!(sealed, s.seal(b"secret output").unwrap());
    }

    #[test]
    fn wrong_missing_and_damaged_are_told_apart() {
        let sealed = sealer(true, new_key()).seal(b"data").unwrap();
        assert_eq!(
            sealer(true, new_key()).open(&sealed),
            Err(Unreadable::WrongKey)
        );
        let none = Sealer::new(true, Arc::new(MemoryKeys::default()));
        assert!(matches!(none.open(&sealed), Err(Unreadable::NoKey(_))));
        assert!(matches!(
            Sealer::plain().open(&sealed),
            Err(Unreadable::NoKey(_))
        ));
        let key = new_key();
        let mut cut = sealer(true, key).seal(b"data").unwrap();
        cut.truncate(cut.len() - 3);
        assert_eq!(sealer(true, key).open(&cut), Err(Unreadable::Damaged));
        assert!(
            none.seal(b"x").is_err(),
            "never writes plaintext when the key is missing"
        );
    }

    #[test]
    fn plaintext_still_loads_and_off_writes_plaintext() {
        let key = new_key();
        assert_eq!(
            &*sealer(true, key).open(b"{\"a\":1}").unwrap(),
            b"{\"a\":1}"
        );
        let off = sealer(false, key);
        assert_eq!(off.seal(b"x").unwrap(), b"x");
        // Turning encryption off still opens files sealed earlier.
        let sealed = sealer(true, key).seal(b"old").unwrap();
        assert_eq!(&*off.open(&sealed).unwrap(), b"old");
    }

    #[test]
    fn never_overwrites_output_it_cannot_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.json");
        std::fs::write(&path, sealer(true, new_key()).seal(b"x").unwrap()).unwrap();
        let other = sealer(true, new_key());
        assert!(other.check_overwrite(&path).is_err());
        std::fs::write(&path, b"plain").unwrap();
        assert!(other.check_overwrite(&path).is_ok());
    }

    #[test]
    fn migrates_both_ways_and_skips_what_it_cannot_open() {
        let dir = tempfile::tempdir().unwrap();
        let key = new_key();
        for name in ["a.json", "a.json.bak", "a.log", "a.usage.json", "notes.txt"] {
            std::fs::write(dir.path().join(name), b"plain").unwrap();
        }
        let on = sealer(true, key);
        assert_eq!(on.migrate_dir(dir.path()).unwrap().changed, 3);
        assert_eq!(count(dir.path()), (3, 0));
        let read = |n: &str| std::fs::read(dir.path().join(n)).unwrap();
        assert_eq!(read("a.usage.json"), b"plain");
        assert_eq!(&*on.open(&read("a.log")).unwrap(), b"plain");

        std::fs::write(
            dir.path().join("b.json"),
            sealer(true, new_key()).seal(b"other").unwrap(),
        )
        .unwrap();
        let report = sealer(false, key).migrate_dir(dir.path()).unwrap();
        assert_eq!(report.changed, 3);
        assert_eq!(report.skipped.len(), 1);
        assert!(Sealer::is_sealed(&read("b.json")), "left untouched");
        assert_eq!(read("a.json"), b"plain");
    }

    #[test]
    fn settings_default_off_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        assert!(!Settings::load(&p).encrypt);
        Settings { encrypt: true }.save(&p).unwrap();
        assert!(Settings::load(&p).encrypt);
        assert!(!Sealer::plain().encrypts());
    }

    #[test]
    fn turning_encryption_on_and_off_converts_saved_output() {
        let dir = tempfile::tempdir().unwrap();
        let p = GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        let threads = p.data_dir.join(THREADS_DIR);
        std::fs::create_dir_all(&threads).unwrap();
        std::fs::write(threads.join("t.log"), b"output").unwrap();
        let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeys::default());
        let off = status(&p, keys.as_ref());
        assert!(!off.encrypt && off.key_available.is_none() && off.plain_files == 1);

        let on = set_encryption(&p, true, keys.clone()).unwrap();
        assert!(on.encrypt && on.key_available == Some(true));
        assert_eq!((on.encrypted_files, on.plain_files), (1, 0));
        let key = keys.get().unwrap().unwrap();
        // Turning it on again reuses the key.
        set_encryption(&p, true, keys.clone()).unwrap();
        assert_eq!(keys.get().unwrap(), Some(key));
        assert!(Sealer::from_settings(&p).encrypts());

        let back = set_encryption(&p, false, keys.clone()).unwrap();
        assert_eq!((back.encrypted_files, back.plain_files), (0, 1));
        assert_eq!(std::fs::read(threads.join("t.log")).unwrap(), b"output");
        assert_eq!(keys.get().unwrap(), Some(key), "the key is kept");
    }

    #[test]
    fn keychain_key_text_is_checked() {
        let k = new_key();
        assert_eq!(parse_key(&util::hex(&k)).unwrap(), k);
        assert!(parse_key("abc").is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
    }
}
