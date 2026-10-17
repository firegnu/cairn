
CREATE TABLE items (
    id TEXT PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    no INTEGER NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL, body TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL, created_by TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at TEXT, closed_by TEXT, close_reason TEXT,
    deleted_at TEXT,
    UNIQUE (project_id, no)
);
CREATE TABLE item_changes (
    id INTEGER PRIMARY KEY, item_id TEXT NOT NULL REFERENCES items(id),
    action TEXT NOT NULL,
    by TEXT NOT NULL,
    evidence TEXT,
    before_title TEXT, before_body TEXT,
    close_reason TEXT,
    ledger_version INTEGER NOT NULL,
    at TEXT NOT NULL
);
CREATE TABLE ledgers (
    project_id INTEGER PRIMARY KEY REFERENCES projects(id),
    version INTEGER NOT NULL DEFAULT 0
);
ALTER TABLE sessions ADD COLUMN ledger_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN ledger_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN ledger_last_failure_at TEXT;
ALTER TABLE sessions ADD COLUMN ledger_last_failure TEXT;
ALTER TABLE sessions ADD COLUMN ledger_injected_version INTEGER;
UPDATE sessions SET ledger_bytes=summarized_bytes, ledger_injected_version=0;

UPDATE meta SET value='4' WHERE key='schema_version';
