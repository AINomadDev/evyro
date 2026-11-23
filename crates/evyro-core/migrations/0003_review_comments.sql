-- Review comments on working-tree changes. A person (or an agent) leaves a
-- comment on a file and line of the diff; open comments are sent to the next
-- agent and resolved when addressed.
CREATE TABLE review_comments (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id     TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    path        TEXT NOT NULL,               -- repository-relative path
    line        INTEGER,                     -- line in the new file; NULL for the whole file
    snippet     TEXT NOT NULL DEFAULT '',    -- the commented line, redacted
    body        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open', -- open | sent | resolved
    author      TEXT NOT NULL,               -- human | agent:<id>
    commit_sha  TEXT,                        -- HEAD when the comment was made
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX review_comments_project ON review_comments(project_id, status);
