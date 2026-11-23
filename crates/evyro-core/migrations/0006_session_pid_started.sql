-- When the session's process started (Unix seconds), recorded with its pid.
-- A pid can be reused by an unrelated process after the agent exits; the
-- start time tells the two apart.
ALTER TABLE sessions ADD COLUMN pid_started INTEGER;
