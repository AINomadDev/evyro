-- Paired devices can expire, and everything done to or by a device is recorded.
ALTER TABLE remote_devices ADD COLUMN expires_at TEXT;   -- NULL: until revoked

-- Pairing, revocation, scope and expiry changes, and the API actions a device took.
-- Never holds codes, tokens, query strings or request bodies.
CREATE TABLE remote_audit (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    at          TEXT NOT NULL,
    device_id   TEXT,
    device_name TEXT,
    actor       TEXT NOT NULL,    -- who did it: local:<user> (cli/desktop) or device:<id>
    action      TEXT NOT NULL,    -- paired | pair_code | revoked | scope | expiry | api | refused
    detail      TEXT NOT NULL DEFAULT '',
    address     TEXT
);
CREATE INDEX remote_audit_device ON remote_audit(device_id, seq);
