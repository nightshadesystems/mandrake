-- Schema version 3: update state (ADR-0015). One row: the last check's
-- plan and the jobs and boot environments of the last apply.

CREATE TABLE update_state (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    plan        TEXT,
    check_job   TEXT,
    apply_job   TEXT,
    applied_at  TEXT,
    applied_be  TEXT,
    previous_be TEXT
);

INSERT INTO update_state (id) VALUES (1);
