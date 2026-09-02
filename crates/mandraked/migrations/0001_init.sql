-- Mandrake metadata store, schema version 1 (ADR-0002, ADR-0007, ADR-0009).
-- Holds only what illumos does not: users, sessions, tokens, audit, events,
-- jobs, idempotency records, per-object metadata, and host identity.
-- Timestamps are RFC 3339 UTC text. Ids are UUID text.

CREATE TABLE host (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    display_name    TEXT,
    role            TEXT NOT NULL CHECK (role IN ('admin', 'operator', 'viewer')),
    password_hash   TEXT NOT NULL,
    disabled        INTEGER NOT NULL DEFAULT 0,
    failed_logins   INTEGER NOT NULL DEFAULT 0,
    first_failed_at TEXT,
    locked_until    TEXT,
    last_login_at   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE sessions (
    hash            TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL,
    last_seen_at    TEXT NOT NULL,
    expires_at      TEXT NOT NULL,
    idle_expires_at TEXT NOT NULL,
    source          TEXT
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE tokens (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    id           TEXT NOT NULL UNIQUE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    prefix       TEXT NOT NULL,
    hash         TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    expires_at   TEXT,
    last_used_at TEXT
);
CREATE INDEX tokens_user ON tokens(user_id);

CREATE TABLE audit (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    at             TEXT NOT NULL,
    actor_id       TEXT,
    actor_username TEXT NOT NULL,
    actor_role     TEXT NOT NULL,
    actor_via      TEXT NOT NULL,
    actor_token_id TEXT,
    action         TEXT NOT NULL,
    object_kind    TEXT NOT NULL,
    object_id      TEXT,
    object_name    TEXT,
    before         TEXT,
    after          TEXT,
    result         TEXT NOT NULL CHECK (result IN ('ok', 'denied', 'failed')),
    detail         TEXT,
    request_id     TEXT,
    source         TEXT
);
CREATE INDEX audit_actor  ON audit(actor_id);
CREATE INDEX audit_object ON audit(object_id);
CREATE INDEX audit_action ON audit(action);
CREATE INDEX audit_at     ON audit(at);

CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    at          TEXT NOT NULL,
    kind        TEXT NOT NULL,
    object_kind TEXT,
    object_id   TEXT,
    object_name TEXT,
    actor       TEXT,
    data        TEXT
);

CREATE TABLE jobs (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    state       TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    kind        TEXT NOT NULL,
    target_kind TEXT,
    target_id   TEXT,
    target_name TEXT,
    progress    REAL,
    message     TEXT,
    created_at  TEXT NOT NULL,
    started_at  TEXT,
    finished_at TEXT,
    error       TEXT
);

CREATE TABLE idempotency (
    actor_key    TEXT NOT NULL,
    key          TEXT NOT NULL,
    body_hash    TEXT NOT NULL,
    status       INTEGER NOT NULL,
    content_type TEXT,
    body         BLOB,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (actor_key, key)
);

CREATE TABLE metadata (
    object_id    TEXT PRIMARY KEY,
    display_name TEXT,
    description  TEXT,
    tags         TEXT,
    notes        TEXT,
    updated_at   TEXT NOT NULL
);
