-- Schema version 2: the image catalogue (ADR-0012). Sources and their
-- cached indexes, and the images imported from them.

CREATE TABLE image_sources (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL UNIQUE,
    url               TEXT NOT NULL,
    public_key        TEXT,
    enabled           INTEGER NOT NULL DEFAULT 1,
    builtin           INTEGER NOT NULL DEFAULT 0,
    verified          INTEGER NOT NULL DEFAULT 0,
    last_refreshed_at TEXT,
    last_error        TEXT,
    created_at        TEXT NOT NULL
);

CREATE TABLE image_catalogue (
    source_id    TEXT NOT NULL REFERENCES image_sources(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    type         TEXT NOT NULL,
    url          TEXT NOT NULL,
    sha256       TEXT NOT NULL,
    size         INTEGER NOT NULL,
    description  TEXT,
    os           TEXT,
    published_at TEXT,
    PRIMARY KEY (source_id, name, version)
);

CREATE TABLE images (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    id          TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    version     TEXT NOT NULL,
    type        TEXT NOT NULL,
    state       TEXT NOT NULL CHECK (state IN ('pending', 'downloading', 'verifying', 'importing', 'ready', 'failed')),
    sha256      TEXT NOT NULL,
    size        INTEGER NOT NULL,
    pool        TEXT NOT NULL,
    dataset     TEXT,
    path        TEXT,
    source_id   TEXT,
    source_name TEXT,
    url         TEXT NOT NULL,
    description TEXT,
    os          TEXT,
    progress    REAL,
    error       TEXT,
    job_id      TEXT,
    created_at  TEXT NOT NULL,
    imported_at TEXT
);
CREATE INDEX images_sha256 ON images(sha256);

-- The two built-in sources (ADR-0012). Keys are set by the publisher.
INSERT INTO image_sources (id, name, url, enabled, builtin, verified, created_at) VALUES
    ('4d0b7f7e-0f5b-4c2e-9a1e-000000000001', 'omnios',
     'https://images.nightshade.systems/omnios/index.json', 1, 1, 0, '2026-09-02T00:00:00Z'),
    ('4d0b7f7e-0f5b-4c2e-9a1e-000000000002', 'nightshade.systems',
     'https://images.nightshade.systems/mandrake/index.json', 1, 1, 0, '2026-09-02T00:00:00Z');
