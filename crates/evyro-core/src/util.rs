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
