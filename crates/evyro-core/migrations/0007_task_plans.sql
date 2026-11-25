-- A plan an AI proposes for a task before it edits anything. A person approves or rejects it;
-- building starts from the approved plan.
CREATE TABLE task_plans (
    task_id     TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    status      TEXT NOT NULL,               -- proposed | approved | rejected
    body        TEXT NOT NULL,               -- the plan, redacted
    note        TEXT,                        -- why it was rejected, or a remark on approval
    proposed_by TEXT NOT NULL,               -- agent:<id> | human
    created_at  TEXT NOT NULL,
    decided_at  TEXT
);
