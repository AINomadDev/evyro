//! Remote devices for `evyro serve`: pairing, tokens, scopes and revocation.
//!
//! A person creates a short pairing code on the computer (`evyro remote pair`).
//! The device sends the code once and gets a long random token back. Evyro
//! stores only SHA-256 hashes of both, so a copy of the database cannot be
//! used to sign in. Codes expire after a few minutes and work once. Tokens
//! last until the device is revoked or, when it was given one, its expiry.
//!
//! A device's scope and expiry can be changed without pairing it again. Every
//! pairing, revocation, scope or expiry change and every action a device takes
//! through the API is recorded in `remote_audit`, with who did it. The audit
//! log never holds codes, tokens, query strings or request bodies.
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
    /// When the device's access ends; `None` means until it is revoked.
    pub expires_at: Option<String>,
    /// `waiting` (code not used yet), `expired` (code or access), `active` or `revoked`.
    pub state: String,
}

/// One line of the remote audit log.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub seq: i64,
    pub at: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    /// `cli:<user>`, `desktop:<user>` or `device:<id>`.
    pub actor: String,
    /// `pair_code`, `paired`, `revoked`, `scope`, `expiry`, `api` or `refused`.
    pub action: String,
    pub detail: String,
    pub address: Option<String>,
}

/// How many audit lines to keep; older ones are dropped.
const AUDIT_KEEP: i64 = 20_000;
/// The longest expiry accepted, in seconds (about five years).
const MAX_ACCESS_SECS: i64 = 5 * 366 * 86_400;

/// Who is acting on this computer, for the audit log: `cli:<user>` or `desktop:<user>`.
pub fn local_actor(via: &str) -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    format!("{via}:{user}")
}

/// Parse an expiry like `30d`, `12h`, `90m` or `2w` into seconds; `never` (or
/// `none`) means no expiry.
pub fn parse_expiry(s: &str) -> Result<Option<i64>> {
    let s = s.trim().to_ascii_lowercase();
    if s == "never" || s == "none" {
        return Ok(None);
    }
    let bad = || {
        Error::Invalid(format!(
            "'{s}' is not an expiry: use a number with m, h, d or w (like 30d), or never"
        ))
    };
    let (n, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = n.parse().map_err(|_| bad())?;
    let mul = match unit {
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        _ => return Err(bad()),
    };
    let secs = n.checked_mul(mul).ok_or_else(bad)?;
    check_access_secs(Some(secs))?;
    Ok(Some(secs))
}

fn check_access_secs(secs: Option<i64>) -> Result<()> {
    match secs {
        Some(s) if !(60..=MAX_ACCESS_SECS).contains(&s) => Err(Error::Invalid(
            "an expiry must be between one minute and five years away".into(),
        )),
        _ => Ok(()),
    }
}

fn check_scope(scope: &str) -> Result<()> {
    if SCOPES.contains(&scope) {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "unknown scope '{scope}' (expected one of {})",
            SCOPES.join(", ")
        )))
    }
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

const COLS: &str = "id, name, scope, created_at, paired_at, last_seen_at, last_address, revoked_at, pair_expires_at, token_sha256, expires_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Device> {
    let revoked: Option<String> = r.get(7)?;
    let expires: Option<String> = r.get(8)?;
    let token: Option<String> = r.get(9)?;
    let access_until: Option<String> = r.get(10)?;
    let now = util::now();
    let state = if revoked.is_some() {
        "revoked"
    } else if access_until.as_deref().is_some_and(|e| e < now.as_str()) {
        "expired"
    } else if token.is_some() {
        "active"
    } else if expires.as_deref().is_some_and(|e| e < now.as_str()) {
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
        expires_at: access_until,
        state: state.into(),
    })
}

impl Store {
    /// Create a device and its one-time pairing code, valid for `ttl_secs`.
    /// With `access_secs`, the device's access ends that long from now.
    /// `actor` is who asked (see [`local_actor`]).
    pub fn pair_device(
        &self,
        name: &str,
        scope: &str,
        ttl_secs: i64,
        access_secs: Option<i64>,
        actor: &str,
    ) -> Result<Pairing> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(Error::Invalid(
                "give the device a name of up to 80 characters".into(),
            ));
        }
        check_scope(scope)?;
        check_access_secs(access_secs)?;
        let ttl = ttl_secs.clamp(60, 3600);
        let code = new_code();
        let id = util::new_id("dev");
        let expires = time_in(ttl);
        self.conn.execute(
            "INSERT INTO remote_devices(id, name, scope, pair_code_sha256, pair_expires_at, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                name,
                scope,
                hash(&normalise_code(&code)),
                expires,
                util::now(),
                access_secs.map(time_in)
            ],
        )?;
        let device = self.device(&id)?;
        let until = device.expires_at.as_deref().unwrap_or("never");
        self.record_remote(
            Some(&device),
            actor,
            "pair_code",
            &format!("scope {scope}, expires {until}"),
            None,
        )?;
        Ok(Pairing {
            device,
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
        if expires < util::now() || self.device(&id)?.state == "expired" {
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
        let d = self.device(&id)?;
        self.record_remote(
            Some(&d),
            &format!("device:{}", d.id),
            "paired",
            "",
            Some(address),
        )?;
        Ok((d, token))
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
            let why = if d.state == "expired" {
                "this device's access has expired; extend it or pair it again on the computer"
            } else {
                "this device was revoked"
            };
            self.record_remote(
                Some(&d),
                &format!("device:{}", d.id),
                "refused",
                &format!("sign-in refused: {}", d.state),
                Some(address),
            )?;
            return Err(Error::Invalid(why.into()));
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

    /// Find a device by id, id prefix (6+ characters) or exact name. Revoked
    /// devices count only with `with_revoked`.
    pub fn find_device(&self, which: &str, with_revoked: bool) -> Result<Device> {
        let all = self.list_devices()?;
        let hits: Vec<&Device> = all
            .iter()
            .filter(|d| with_revoked || d.revoked_at.is_none())
            .filter(|d| {
                d.id == which || d.name == which || (which.len() >= 6 && d.id.starts_with(which))
            })
            .collect();
        match hits.as_slice() {
            [d] => Ok((*d).clone()),
            [] => Err(Error::NotFound(format!("device '{which}'"))),
            _ => Err(Error::Conflict(format!(
                "'{which}' matches more than one device; use its id"
            ))),
        }
    }

    /// Revoke a device by id, id prefix or exact name. Its token stops working at once.
    pub fn revoke_device(&self, which: &str, actor: &str) -> Result<Device> {
        let d = self.find_device(which, false)?;
        self.conn.execute(
            "UPDATE remote_devices SET revoked_at = ?1, pair_code_sha256 = NULL WHERE id = ?2",
            rusqlite::params![util::now(), d.id],
        )?;
        let d = self.device(&d.id)?;
        self.record_remote(Some(&d), actor, "revoked", "", None)?;
        Ok(d)
    }

    /// Change a device's scope without pairing it again. Takes effect on its next request.
    pub fn set_device_scope(&self, which: &str, scope: &str, actor: &str) -> Result<Device> {
        check_scope(scope)?;
        let d = self.find_device(which, false)?;
        if d.scope == scope {
            return Ok(d);
        }
        self.conn.execute(
            "UPDATE remote_devices SET scope = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            rusqlite::params![scope, d.id],
        )?;
        let after = self.device(&d.id)?;
        self.record_remote(
            Some(&after),
            actor,
            "scope",
            &format!("{} -> {scope}", d.scope),
            None,
        )?;
        Ok(after)
    }

    /// Set when a device's access ends, counted from now; `None` keeps it until revoked.
    /// This also extends a device that has already expired.
    pub fn set_device_expiry(
        &self,
        which: &str,
        access_secs: Option<i64>,
        actor: &str,
    ) -> Result<Device> {
        check_access_secs(access_secs)?;
        let d = self.find_device(which, false)?;
        self.conn.execute(
            "UPDATE remote_devices SET expires_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            rusqlite::params![access_secs.map(time_in), d.id],
        )?;
        let after = self.device(&d.id)?;
        self.record_remote(
            Some(&after),
            actor,
            "expiry",
            &format!(
                "{} -> {}",
                d.expires_at.as_deref().unwrap_or("never"),
                after.expires_at.as_deref().unwrap_or("never")
            ),
            None,
        )?;
        Ok(after)
    }

    /// Add a line to the remote audit log. `detail` is redacted before it is stored.
    pub fn record_remote(
        &self,
        device: Option<&Device>,
        actor: &str,
        action: &str,
        detail: &str,
        address: Option<&str>,
    ) -> Result<()> {
        let (detail, _) = crate::redact::redact(detail);
        self.conn.execute(
            "INSERT INTO remote_audit(at, device_id, device_name, actor, action, detail, address)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                util::now(),
                device.map(|d| &d.id),
                device.map(|d| &d.name),
                actor,
                action,
                detail,
                address
            ],
        )?;
        let seq = self.conn.last_insert_rowid();
        if seq % 500 == 0 {
            self.conn.execute(
                "DELETE FROM remote_audit WHERE seq <= ?1",
                [seq - AUDIT_KEEP],
            )?;
        }
        Ok(())
    }

    /// The newest audit lines first, for one device (id, prefix or name) or all.
    pub fn remote_audit(&self, which: Option<&str>, limit: usize) -> Result<Vec<AuditEntry>> {
        let device = match which {
            Some(w) => Some(self.find_device(w, true)?.id),
            None => None,
        };
        let mut st = self.conn.prepare(
            "SELECT seq, at, device_id, device_name, actor, action, detail, address
             FROM remote_audit WHERE ?1 IS NULL OR device_id = ?1
             ORDER BY seq DESC LIMIT ?2",
        )?;
        let v = st
            .query_map(rusqlite::params![device, limit.min(10_000) as i64], |r| {
                Ok(AuditEntry {
                    seq: r.get(0)?,
                    at: r.get(1)?,
                    device_id: r.get(2)?,
                    device_name: r.get(3)?,
                    actor: r.get(4)?,
                    action: r.get(5)?,
                    detail: r.get(6)?,
                    address: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(v)
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
        let p = s
            .pair_device("Phone", "approve", 600, None, "cli:me")
            .unwrap();
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
        s.revoke_device(&dev.id[..10], "cli:me").unwrap();
        assert!(
            s.authenticate_device(&token, "x").is_err(),
            "revoked tokens stop working"
        );
    }

    #[test]
    fn expired_codes_and_bad_input_are_refused() {
        let (_d, s) = store();
        let p = s
            .pair_device("Laptop", "read", 600, None, "cli:me")
            .unwrap();
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
        assert!(s.pair_device("", "read", 60, None, "cli:me").is_err());
        assert!(s.pair_device("x", "admin", 60, None, "cli:me").is_err());
        let codes: std::collections::HashSet<String> = (0..200).map(|_| new_code()).collect();
        assert_eq!(codes.len(), 200);
        assert!(codes.iter().all(|c| c
            .chars()
            .all(|ch| ch == '-' || CODE_ALPHABET.contains(&(ch as u8)))));
    }

    #[test]
    fn expired_devices_are_refused_and_can_be_extended() {
        let (_d, s) = store();
        assert!(s.pair_device("P", "read", 600, Some(10), "cli:me").is_err());
        let p = s
            .pair_device("Phone", "read", 600, Some(86_400), "cli:me")
            .unwrap();
        assert!(p.device.expires_at.is_some());
        let (dev, token) = s.redeem_pairing(&p.code, "1.2.3.4").unwrap();
        assert_eq!(dev.state, "active");
        s.conn
            .execute(
                "UPDATE remote_devices SET expires_at = '2000-01-01T00:00:00.000Z'",
                [],
            )
            .unwrap();
        let d = s.device(&dev.id).unwrap();
        assert_eq!(d.state, "expired");
        assert!(!d.allows("read"), "deny by default once expired");
        let e = s.authenticate_device(&token, "1.2.3.4").unwrap_err();
        assert!(e.to_string().contains("expired"), "{e}");
        let d = s.set_device_expiry("Phone", None, "desktop:me").unwrap();
        assert_eq!((d.state.as_str(), d.expires_at), ("active", None));
        assert!(s.authenticate_device(&token, "1.2.3.4").is_ok());
        // A code whose device expired before it was used does not pair.
        let p2 = s
            .pair_device("Tablet", "read", 600, Some(3600), "cli:me")
            .unwrap();
        s.conn
            .execute(
                "UPDATE remote_devices SET expires_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1",
                [&p2.device.id],
            )
            .unwrap();
        assert!(s.redeem_pairing(&p2.code, "x").is_err());
        assert_eq!(parse_expiry("30d").unwrap(), Some(30 * 86_400));
        assert_eq!(parse_expiry("Never").unwrap(), None);
        assert!(parse_expiry("30").is_err() && parse_expiry("xd").is_err());
        assert!(parse_expiry("0d").is_err() && parse_expiry("99w").is_ok());
        assert!(parse_expiry("400w").is_err());
    }

    #[test]
    fn scope_changes_apply_at_once_and_everything_is_audited() {
        let (_d, s) = store();
        let p = s
            .pair_device("Phone", "control", 600, None, "cli:me")
            .unwrap();
        let (dev, token) = s.redeem_pairing(&p.code, "1.2.3.4").unwrap();
        assert!(s.set_device_scope("Phone", "admin", "cli:me").is_err());
        let d = s.set_device_scope(&dev.id, "read", "desktop:me").unwrap();
        assert_eq!(d.scope, "read");
        let d = s.authenticate_device(&token, "1.2.3.4").unwrap();
        assert!(d.allows("read") && !d.allows("approve"));
        s.record_remote(
            Some(&d),
            "device:x",
            "api",
            "POST /api/v1/chats -> 403",
            None,
        )
        .unwrap();
        s.revoke_device("Phone", "cli:me").unwrap();
        assert!(s.set_device_scope("Phone", "control", "cli:me").is_err());
        assert!(s.authenticate_device(&token, "1.2.3.4").is_err());

        let log = s.remote_audit(Some("Phone"), 50).unwrap();
        let actions: Vec<&str> = log.iter().map(|e| e.action.as_str()).collect();
        assert_eq!(
            actions,
            ["refused", "revoked", "api", "scope", "paired", "pair_code"]
        );
        let scope = log.iter().find(|e| e.action == "scope").unwrap();
        assert_eq!(scope.detail, "control -> read");
        assert_eq!(scope.actor, "desktop:me");
        assert_eq!(log.last().unwrap().actor, "cli:me");
        let all = serde_json::to_string(&s.remote_audit(None, 100).unwrap()).unwrap();
        assert!(
            !all.contains(&token) && !all.contains(&p.code),
            "no secrets"
        );
        assert!(!all.contains(&hash(&token)));
    }
}
