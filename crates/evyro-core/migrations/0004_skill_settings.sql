-- Which skills are turned on. scope is '*' for every project, or a project id.
-- A project's own row wins over the '*' row; with no row a skill is on.
CREATE TABLE skill_settings (
    scope       TEXT NOT NULL,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (scope, name)
);
