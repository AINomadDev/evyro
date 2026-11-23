-- Acceptance criteria are checked off one by one. `verified` needs every
-- criterion met, in addition to a passing receipt for the current code.
CREATE TABLE acceptance_checks (
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,            -- 1-based position in tasks.acceptance
    criterion   TEXT NOT NULL,               -- text at the time it was checked
    met         INTEGER NOT NULL,            -- 1 met, 0 explicitly unmet
    note        TEXT NOT NULL DEFAULT '',
    receipt_id  TEXT REFERENCES verifications(id),
    actor       TEXT NOT NULL,               -- human | agent:<id>
    provenance  TEXT NOT NULL,               -- human_recorded | evyro_verified | agent_reported
    checked_at  TEXT NOT NULL,
    PRIMARY KEY (task_id, idx)
);
