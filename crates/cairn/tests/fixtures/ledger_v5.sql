-- Frozen version 5 addition (cairn 0.7.x).
CREATE TABLE ledger_runs (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    source_id TEXT NOT NULL,
    manual INTEGER NOT NULL,
    started_at TEXT NOT NULL, finished_at TEXT NOT NULL,
    outcome TEXT NOT NULL,
    entries INTEGER NOT NULL DEFAULT 0,
    lessons INTEGER NOT NULL DEFAULT 0,
    ledger_version INTEGER,
    reason TEXT,
    from_bytes INTEGER NOT NULL,
    to_bytes INTEGER,
    extracted_bytes INTEGER,
    omitted_bytes INTEGER,
    tools_cut INTEGER
);
CREATE INDEX ledger_runs_project ON ledger_runs(project_id, id);
UPDATE meta SET value='5' WHERE key='schema_version';
