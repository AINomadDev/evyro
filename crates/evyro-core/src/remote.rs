//! Remote devices for `evyro serve`: pairing, tokens, scopes and revocation.
//!
//! A person creates a short pairing code on the computer (`evyro remote pair`).
//! The device sends the code once and gets a long random token back. Evyro
//! stores only SHA-256 hashes of both, so a copy of the database cannot be
//! used to sign in. Codes expire after a few minutes and work once. Tokens
//! last until the device is revoked.
//!
//! Scopes, from least to most:
//! - `read`: see projects, tasks, sessions, handoffs, diffs, chats;
//! - `approve`: also answer an agent's permission requests and leave review comments;
//! - `control`: also start, message, stop and close chats, and use MCP with write tools.

use crate::error::{Error, Result};
use crate::{util, Store};
use serde::Serialize;

pub const SCOPES: &[&str] = &["read", "approve", "control"];
/// Pairing codes use letters and digits that are hard to confuse.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const TOKEN_PREFIX: &str = "evr_";

#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub created_at: String,
    pub paired_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_address: Option<String>,
    pub revoked_at: Option<String>,
    /// `waiting` (code not used yet), `expired`, `active` or `revoked`.
    pub state: String,
}

impl Device {
    /// Does this device's scope include `need`?
    pub fn allows(&self, need: &str) -> bool {
        rank(&self.scope) >= rank(need) && self.state == "active"
    }
}

fn rank(scope: &str) -> usize {
    SCOPES
        .iter()
        .position(|s| *s == scope)
        .map(|i| i + 1)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct Pairing {
    pub device: Device,
    /// Shown once; only its hash is kept.
    pub code: String,
    pub expires_at: String,
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    out.truncate(n);
    out
}

fn new_code() -> String {
    // Rejection sampling keeps every character equally likely.
    let mut chars = Vec::with_capacity(8);
    while chars.len() < 8 {
        for b in random_bytes(16) {
            if (b as usize) < 256 - 256 % CODE_ALPHABET.len() && chars.len() < 8 {
                chars.push(CODE_ALPHABET[b as usize % CODE_ALPHABET.len()] as char);
            }
        }
    }
    let s: String = chars.into_iter().collect();
    format!("{}-{}", &s[..4], &s[4..])
}

/// Codes are typed by hand: ignore case, spaces and dashes.
fn normalise_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn hash(s: &str) -> String {
    util::sha256_hex(s.as_bytes())
}

fn time_in(secs: i64) -> String {
    let t = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
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

const COLS: &str = "id, name, scope, created_at, paired_at, last_seen_at, last_address, revoked_at, pair_expires_at, token_sha256";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Device> {
    let revoked: Option<String> = r.get(7)?;
    let expires: Option<String> = r.get(8)?;
    let token: Option<String> = r.get(9)?;
    let state = if revoked.is_some() {
        "revoked"
    } else if token.is_some() {
        "active"
    } else if expires.as_deref().is_some_and(|e| e < util::now().as_str()) {
        "expired"
    } else {
        "waiting"
    };
    Ok(Device {
        id: r.get(0)?,
        name: r.get(1)?,
        scope: r.get(2)?,
        created_at: r.get(3)?,
        paired_at: r.get(4)?,
        last_seen_at: r.get(5)?,
        last_address: r.get(6)?,
        revoked_at: revoked,
        state: state.into(),
    })
}

impl Store {
    /// Create a device and its one-time pairing code, valid for `ttl_secs`.
    pub fn pair_device(&self, name: &str, scope: &str, ttl_secs: i64) -> Result<Pairing> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(Error::Invalid(
                "give the device a name of up to 80 characters".into(),
            ));
        }
        if !SCOPES.contains(&scope) {
            return Err(Error::Invalid(format!(
                "unknown scope '{scope}' (expected one of {})",
                SCOPES.join(", ")
            )));
        }
        let ttl = ttl_secs.clamp(60, 3600);
        let code = new_code();
        let id = util::new_id("dev");
        let expires = time_in(ttl);
        self.conn.execute(
            "INSERT INTO remote_devices(id, name, scope, pair_code_sha256, pair_expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, name, scope, hash(&normalise_code(&code)), expires, util::now()],
        )?;
        Ok(Pairing {
            device: self.device(&id)?,
            code,
            expires_at: expires,
        })
    }

    /// Exchange a pairing code for a token. The code works once.
    pub fn redeem_pairing(&self, code: &str, address: &str) -> Result<(Device, String)> {
        let h = hash(&normalise_code(code));
        let found: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, pair_expires_at FROM remote_devices
                 WHERE pair_code_sha256 = ?1 AND token_sha256 IS NULL AND revoked_at IS NULL",
                [&h],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let Some((id, expires)) = found else {
            return Err(Error::Invalid("that pairing code is not valid".into()));
        };
        if expires < util::now() {
            return Err(Error::Invalid(
                "that pairing code has expired; make a new one".into(),
            ));
        }
        let token = format!("{TOKEN_PREFIX}{}", util::hex(&random_bytes(32)));
        let now = util::now();
        self.conn.execute(
            "UPDATE remote_devices SET token_sha256 = ?1, pair_code_sha256 = NULL, paired_at = ?2,
             last_seen_at = ?2, last_address = ?3 WHERE id = ?4",
            rusqlite::params![hash(&token), now, address, id],
        )?;
        Ok((self.device(&id)?, token))
    }

    /// The active device a token belongs to. Records when and from where it was used.
    pub fn authenticate_device(&self, token: &str, address: &str) -> Result<Device> {
        if !token.starts_with(TOKEN_PREFIX) {
            return Err(Error::Invalid("not a device token".into()));
        }
        let d = self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM remote_devices WHERE token_sha256 = ?1"),
                [hash(token)],
                row,
            )
            .map_err(|_| Error::Invalid("unknown device token".into()))?;
        if d.state != "active" {
            return Err(Error::Invalid("this device was revoked".into()));
        }
        self.conn.execute(
            "UPDATE remote_devices SET last_seen_at = ?1, last_address = ?2 WHERE id = ?3",
            rusqlite::params![util::now(), address, d.id],
        )?;
        Ok(d)
    }

    pub fn device(&self, id: &str) -> Result<Device> {
        self.conn
            .query_row(
                &format!("SELECT {COLS} FROM remote_devices WHERE id = ?1"),
                [id],
                row,
            )
            .map_err(|_| Error::NotFound(format!("device {id}")))
    }

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM remote_devices ORDER BY created_at"
        ))?;
        let v = st
            .query_map([], row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Revoke a device by id, id prefix or exact name. Its token stops working at once.
    pub fn revoke_device(&self, which: &str) -> Result<Device> {
        let all = self.list_devices()?;
        let hits: Vec<&Device> = all
            .iter()
            .filter(|d| d.revoked_at.is_none())
            .filter(|d| {
                d.id == which || d.name == which || (which.len() >= 6 && d.id.starts_with(which))
            })
            .collect();
        let d = match hits.as_slice() {
            [d] => (*d).clone(),
            [] => return Err(Error::NotFound(format!("device '{which}'"))),
            _ => {
                return Err(Error::Conflict(format!(
                    "'{which}' matches more than one device; use its id"
                )))
            }
        };
        self.conn.execute(
            "UPDATE remote_devices SET revoked_at = ?1, pair_code_sha256 = NULL WHERE id = ?2",
            rusqlite::params![util::now(), d.id],
        )?;
        self.device(&d.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::GlobalPaths;

    fn store() -> (tempfile::TempDir, Store) {
        let d = tempfile::tempdir().unwrap();
        let paths = GlobalPaths {
            data_dir: d.path().join("data"),
            config_dir: d.path().join("config"),
        };
        paths.ensure().unwrap();
        let s = Store::open_at(&paths.db_path(), paths.clone()).unwrap();
        (d, s)
    }

    #[test]
    fn pairing_is_one_time_and_tokens_are_hashed() {
        let (_d, s) = store();
        let p = s.pair_device("Phone", "approve", 600).unwrap();
        assert_eq!(p.code.len(), 9);
        assert_eq!(p.device.state, "waiting");
        assert!(s.redeem_pairing("WRONG-CODE", "1.2.3.4").is_err());
        let (dev, token) = s
            .redeem_pairing(&p.code.to_lowercase().replace('-', " "), "1.2.3.4")
            .unwrap();
        assert_eq!(dev.state, "active");
        assert!(token.starts_with(TOKEN_PREFIX) && token.len() == 4 + 64);
        assert!(
            s.redeem_pairing(&p.code, "1.2.3.4").is_err(),
            "a code works once"
        );
        let stored: String = s
            .conn
            .query_row("SELECT token_sha256 FROM remote_devices", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, token);
        let d = s.authenticate_device(&token, "5.6.7.8").unwrap();
        assert!(d.allows("read") && d.allows("approve") && !d.allows("control"));
        assert!(s.authenticate_device("evr_nope", "x").is_err());
        s.revoke_device(&dev.id[..10]).unwrap();
        assert!(
            s.authenticate_device(&token, "x").is_err(),
            "revoked tokens stop working"
        );
    }

    #[test]
    fn expired_codes_and_bad_input_are_refused() {
        let (_d, s) = store();
        let p = s.pair_device("Laptop", "read", 600).unwrap();
        s.conn
            .execute(
                "UPDATE remote_devices SET pair_expires_at = '2000-01-01T00:00:00.000Z'",
                [],
            )
            .unwrap();
        assert_eq!(s.device(&p.device.id).unwrap().state, "expired");
        assert!(s
            .redeem_pairing(&p.code, "x")
            .unwrap_err()
            .to_string()
            .contains("expired"));
        assert!(s.pair_device("", "read", 60).is_err());
        assert!(s.pair_device("x", "admin", 60).is_err());
        let codes: std::collections::HashSet<String> = (0..200).map(|_| new_code()).collect();
        assert_eq!(codes.len(), 200);
        assert!(codes.iter().all(|c| c
            .chars()
            .all(|ch| ch == '-' || CODE_ALPHABET.contains(&(ch as u8)))));
    }
}
