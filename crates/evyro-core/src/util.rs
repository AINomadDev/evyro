use sha2::{Digest, Sha256};
use std::path::Path;
use time::OffsetDateTime;

/// RFC 3339 UTC timestamp with a fixed three-digit millisecond field, e.g.
/// `2026-11-22T21:56:20.050Z`. The fixed width keeps string order equal to
/// time order; the trimmed form (`20.05Z`, `20Z`) does not sort correctly.
pub fn now() -> String {
    let t = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// Stable, time-sortable id with a type prefix, e.g. `tsk_0193...`.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// Rough token estimate (chars / 4). Always labelled as an estimate in output.
pub fn approx_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

/// Write a file atomically: write to a sibling temp file then rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id()
    ));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// The last good copy kept beside `path` (`<name>.bak`).
pub fn last_good_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Replace `path` so that a crash or power loss never costs more than this
/// write: the data goes to a temporary file in the same directory, is synced to
/// disk and renamed over `path`. The file being replaced is copied to the last
/// good copy ([`last_good_path`]) when `is_good` accepts it, so a damaged file
/// never overwrites a good copy. `path` itself exists at every moment, so a
/// reader running at the same time never finds it missing. Both temporary
/// files are readable only by their owner.
pub fn write_keeping_last_good(
    path: &Path,
    data: &[u8],
    is_good: impl Fn(&[u8]) -> bool,
) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    if let Ok(current) = std::fs::read(path) {
        if is_good(&current) {
            // Copy rather than move, so `path` is never briefly absent.
            let mut keep = tempfile::NamedTempFile::new_in(dir)?;
            keep.write_all(&current)?;
            keep.as_file().sync_all()?;
            let keep = keep.into_temp_path();
            rename_replacing(&keep, &last_good_path(path))?;
            keep.keep().map_err(|e| e.error)?;
        }
    }
    let temp = temp.into_temp_path();
    rename_replacing(&temp, path)?;
    temp.keep().map_err(|e| e.error)?;
    #[cfg(unix)]
    std::fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// Rename `from` over `to`, replacing it. Windows answers "access denied" while
/// another program (a reader, a virus scanner, the search indexer) briefly has
/// `to` open, so there it tries again for up to about a second.
pub fn rename_replacing(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut tries: u64 = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e)
                if cfg!(windows)
                    && e.kind() == std::io::ErrorKind::PermissionDenied
                    && tries < 20 =>
            {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(5 * tries));
            }
            done => return done,
        }
    }
}

/// Read `path` with `parse`, falling back to its last good copy when the file
/// is missing, unreadable or rejected by `parse` (for example cut short).
pub fn read_or_last_good<T>(path: &Path, parse: impl Fn(&[u8]) -> Option<T>) -> Option<T> {
    [path.to_path_buf(), last_good_path(path)]
        .iter()
        .find_map(|p| std::fs::read(p).ok().and_then(|bytes| parse(&bytes)))
}

/// Normalise a path for storage: canonicalised when it exists, with forward
/// slashes kept as-is on Unix and the Windows verbatim prefix stripped.
pub fn canonical(path: &Path) -> std::io::Result<std::path::PathBuf> {
    let p = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            if !rest.starts_with("UNC\\") {
                return Ok(std::path::PathBuf::from(rest));
            }
        }
    }
    Ok(p)
}

#[cfg(test)]
mod time_tests {
    #[test]
    fn timestamps_are_fixed_width_and_sortable() {
        let a = super::now();
        assert_eq!(a.len(), "2026-11-22T21:56:20.050Z".len(), "{a}");
        assert!(a.ends_with('Z') && a.as_bytes()[19] == b'.');
        // Parses as RFC 3339.
        time::OffsetDateTime::parse(&a, &time::format_description::well_known::Rfc3339).unwrap();
    }
}

#[cfg(test)]
mod last_good_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// While the file is rewritten over and over, a reader at the same time
    /// always finds it, and the last good copy holds a complete earlier write.
    #[test]
    fn a_file_being_replaced_is_never_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thread.json");
        write_keeping_last_good(&path, b"v0", |_| true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut missing = 0;
                while !stop.load(Ordering::SeqCst) {
                    if std::fs::metadata(&path).is_err() {
                        missing += 1;
                    }
                }
                missing
            })
        };
        for i in 1..=300 {
            write_keeping_last_good(&path, format!("v{i}").as_bytes(), |_| true).unwrap();
        }
        stop.store(true, Ordering::SeqCst);
        assert_eq!(watcher.join().unwrap(), 0, "the file was briefly missing");
        assert_eq!(std::fs::read(&path).unwrap(), b"v300");
        assert_eq!(std::fs::read(last_good_path(&path)).unwrap(), b"v299");
    }
}
