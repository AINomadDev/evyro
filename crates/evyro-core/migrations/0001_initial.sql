-- Evyro schema v1

CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    alias           TEXT UNIQUE,
    root_path       TEXT NOT NULL UNIQUE,
    git_remote      TEXT,
    main_branch     TEXT,
    stack           TEXT NOT NULL DEFAULT '[]',      -- JSON array of detected stacks
    commands        TEXT NOT NULL DEFAULT '{}',      -- JSON {build,test,lint: [..]}
    archived        INTEGER NOT NULL DEFAULT 0,
    favorite        INTEGER NOT NULL DEFAULT 0,
    next_task_seq   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    last_activity_at TEXT
);

CREATE TABLE tasks (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    parent_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    kind            TEXT NOT NULL DEFAULT 'task',
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    priority        INTEGER NOT NULL DEFAULT 2,       -- 0 critical .. 4 trivial
    status          TEXT NOT NULL DEFAULT 'backlog',
    acceptance      TEXT NOT NULL DEFAULT '[]',       -- JSON array of strings
    constraints     TEXT NOT NULL DEFAULT '[]',       -- JSON array of strings
    files           TEXT NOT NULL DEFAULT '[]',       -- JSON array of related paths
    assigned_agent  TEXT,
    branch          TEXT,
    worktree        TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(project_id, seq)
);
CREATE INDEX idx_tasks_project_status ON tasks(project_id, status);

CREATE TABLE task_deps (
    task_id         TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_id   TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL DEFAULT 'blocks',   -- blocks | related
    PRIMARY KEY (task_id, depends_on_id)
);

CREATE TABLE task_comments (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    author          TEXT NOT NULL,
    body            TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    harness         TEXT NOT NULL,
    model           TEXT,
    provider        TEXT,
    worktree        TEXT,
    pid             INTEGER,
    status          TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    ended_at        TEXT,
    exit_reason     TEXT,
    last_event_at   TEXT,
    latest_checkpoint_id TEXT,
    handoff_id      TEXT                               -- handoff this session resumed from
);
CREATE INDEX idx_sessions_project ON sessions(project_id, status);

-- Task ownership lease (one active owner per task).
CREATE TABLE leases (
    task_id         TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    harness         TEXT NOT NULL,
    acquired_at     TEXT NOT NULL
);

-- Append-only event log.
CREATE TABLE events (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    id              TEXT NOT NULL UNIQUE,
    project_id      TEXT REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT,
    session_id      TEXT,
    kind            TEXT NOT NULL,
    payload         TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_events_project ON events(project_id, seq);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;

CREATE TABLE checkpoints (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    session_id      TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    parent_id       TEXT REFERENCES checkpoints(id) ON DELETE SET NULL,
    trigger         TEXT NOT NULL,
    summary         TEXT NOT NULL DEFAULT '',
    completed       TEXT NOT NULL DEFAULT '[]',
    remaining       TEXT NOT NULL DEFAULT '[]',
    blockers        TEXT NOT NULL DEFAULT '[]',
    next_action     TEXT NOT NULL DEFAULT '',
    git             TEXT NOT NULL,                     -- JSON GitSnapshot
    fingerprint     TEXT NOT NULL,                     -- hash of git snapshot, for dedupe/drift
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_checkpoints_task ON checkpoints(task_id, created_at);

CREATE TABLE handoffs (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    from_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    from_harness    TEXT,
    to_harness      TEXT NOT NULL,
    checkpoint_id   TEXT REFERENCES checkpoints(id) ON DELETE SET NULL,
    contract_path   TEXT NOT NULL,
    contract_sha256 TEXT NOT NULL,
    status          TEXT NOT NULL,                     -- prepared | accepted | failed | superseded
    quality         TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    accepted_at     TEXT,
    accepted_session_id TEXT
);

-- Verification receipts. Only produced by evyro running a command itself.
CREATE TABLE verifications (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    session_id      TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    command         TEXT NOT NULL,                     -- JSON argv
    exit_code       INTEGER,
    passed          INTEGER NOT NULL,
    started_at      TEXT NOT NULL,
    duration_ms     INTEGER NOT NULL,
    commit_sha      TEXT,
    fingerprint     TEXT,                              -- git fingerprint at run time
    worktree        TEXT NOT NULL,
    environment     TEXT NOT NULL DEFAULT '{}',
    output_path     TEXT,
    output_sha256   TEXT
);
CREATE INDEX idx_verifications_task ON verifications(task_id, started_at);

CREATE TABLE decisions (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id         TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    title           TEXT NOT NULL,
    decision        TEXT NOT NULL,
    rationale       TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'accepted', -- proposed | accepted | superseded
    source          TEXT NOT NULL,                     -- who recorded it
    created_at      TEXT NOT NULL
);

CREATE TABLE memories (
    id              TEXT PRIMARY KEY,
    project_id      TEXT REFERENCES projects(id) ON DELETE CASCADE, -- NULL = global
    category        TEXT NOT NULL,
    content         TEXT NOT NULL,
    source          TEXT NOT NULL,
    task_id         TEXT,
    files           TEXT NOT NULL DEFAULT '[]',
    file_hashes     TEXT NOT NULL DEFAULT '{}',        -- path -> sha256 when recorded (drift detection)
    confidence      TEXT NOT NULL DEFAULT 'medium',    -- low | medium | high
    verification    TEXT NOT NULL DEFAULT 'unverified',-- unverified | verified | stale
    archived        INTEGER NOT NULL DEFAULT 0,
    superseded_by   TEXT,
    created_at      TEXT NOT NULL,
    validated_at    TEXT
);
CREATE VIRTUAL TABLE memories_fts USING fts5(content, category, content='memories', content_rowid='rowid');
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, category) VALUES (new.rowid, new.content, new.category);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, category) VALUES ('delete', old.rowid, old.content, old.category);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, category) VALUES ('delete', old.rowid, old.content, old.category);
  INSERT INTO memories_fts(rowid, content, category) VALUES (new.rowid, new.content, new.category);
END;
