-- Oxidium Migration 0003: Cross-process sync state, MCP idempotency, and holidays

CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    last_source TEXT NOT NULL DEFAULT 'gui'
);

INSERT OR IGNORE INTO sync_state (id, revision, updated_at, last_source)
VALUES (1, 0, '2026-01-01T00:00:00Z', 'gui');

CREATE TABLE IF NOT EXISTS mcp_idempotency_keys (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    tool_name TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_idempotency_created ON mcp_idempotency_keys(created_at);

CREATE TABLE IF NOT EXISTS holidays (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT REFERENCES profiles(id) ON DELETE CASCADE,
    date TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
);

CREATE INDEX IF NOT EXISTS idx_holidays_date ON holidays(date);
