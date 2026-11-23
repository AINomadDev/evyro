-- Devices allowed to use `evyro serve` (a phone, another computer).
-- Only hashes of pairing codes and tokens are stored.
CREATE TABLE remote_devices (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    scope            TEXT NOT NULL,            -- read | approve | control
    pair_code_sha256 TEXT,                     -- until the device pairs
    pair_expires_at  TEXT,
    token_sha256     TEXT UNIQUE,              -- after it pairs
    created_at       TEXT NOT NULL,
    paired_at        TEXT,
    last_seen_at     TEXT,
    last_address     TEXT,
    revoked_at       TEXT
);
