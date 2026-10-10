-- The version 1 schema as shipped (cairn 0.1.0), frozen: what a database made before version 2 holds.
CREATE TABLE meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    adopted INTEGER NOT NULL CHECK (adopted IN (0, 1)),
    adopted_at TEXT, unadopted_at TEXT
);
CREATE TABLE sources (
    id TEXT PRIMARY KEY NOT NULL,
    agent TEXT NOT NULL,
    session_id TEXT,
    association TEXT NOT NULL CHECK (association IN ('hook', 'declared', 'uncertain')),
    first_seen TEXT NOT NULL, last_seen TEXT NOT NULL
);
CREATE TABLE records (
    id TEXT PRIMARY KEY NOT NULL,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    line_path TEXT NOT NULL, branch TEXT,
    source_id TEXT NOT NULL REFERENCES sources(id),
    kind TEXT NOT NULL CHECK (kind IN ('checkpoint', 'correction', 'retraction', 'restore')),
    target_id TEXT REFERENCES records(id),
    body TEXT, facts TEXT,
    created_at TEXT NOT NULL, deleted_at TEXT
);
CREATE TABLE supersessions (
    record_id TEXT NOT NULL REFERENCES records(id),
    target_id TEXT NOT NULL REFERENCES records(id),
    PRIMARY KEY (record_id, target_id)
);
CREATE TABLE injections (
    source_id TEXT NOT NULL, record_id TEXT NOT NULL, injected_at TEXT NOT NULL,
    PRIMARY KEY (source_id, record_id)
);
CREATE TABLE confirmations (
    id INTEGER PRIMARY KEY,
    source_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('saved', 'nothing_new')),
    op_id TEXT UNIQUE,
    record_id TEXT, at TEXT NOT NULL
);
CREATE TABLE turn_decisions (
    source_id TEXT NOT NULL, turn_key TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN (
        'confirmed', 'continue_requested', 'unconfirmed_after_continue',
        'pending_unprocessed', 'skipped'
    )),
    at TEXT NOT NULL,
    PRIMARY KEY (source_id, turn_key, outcome)
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    source_id TEXT NOT NULL, kind TEXT NOT NULL,
    at TEXT NOT NULL, detail TEXT
);
CREATE TABLE spool_ops (
    op_id TEXT PRIMARY KEY NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('ingested', 'rejected')),
    source_id TEXT NOT NULL,
    record_id TEXT, reason TEXT,
    processed_at TEXT NOT NULL
);

CREATE INDEX records_line_source_time ON records (project_id, line_path, source_id, created_at, id);
CREATE INDEX records_target ON records (target_id);
CREATE INDEX supersessions_target ON supersessions (target_id);
CREATE INDEX confirmations_source_time ON confirmations (source_id, at);
CREATE INDEX events_source_time ON events (source_id, at);
INSERT INTO meta (key, value) VALUES ('schema_version', '1');
