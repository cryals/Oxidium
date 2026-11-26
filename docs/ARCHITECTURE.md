# Oxidium Architecture

## High-Level Design

Oxidium follows a strict layered **Clean / Hexagonal Architecture** so that the **Slint GUI** and **MCP Server** share 100% identical business logic, validation, analytics, permissions, and SQLite persistence:

```text
┌─────────────────────────┐          ┌──────────────────────────────────┐
│    Slint Native GUI     │          │    MCP Server (STDIO / HTTP)     │
│  (crates/journal-ui)    │          │       (crates/journal-mcp)       │
└───────────┬─────────────┘          └────────────────┬─────────────────┘
            │                                         │
            └───────────────────┬─────────────────────┘
                                ▼
               ┌────────────────────────────────┐
               │      Application Services      │
               │      (crates/journal-app)      │
               │  Profiles · Subjects · Grades  │
               │  Attendance · Journal · Stats  │
               │  Kanban · Search · Backup/IO   │
               │  Notifications · i18n · Sync   │
               └────────────────┬───────────────┘
                                │
             ┌──────────────────┴──────────────────┐
             ▼                                     ▼
┌─────────────────────────┐          ┌─────────────────────────┐
│      Domain Layer       │          │      Storage Layer      │
│ (crates/journal-domain) │          │(crates/journal-storage) │
│ Pure entities, Decimal  │          │ SQLite WAL, Migrations, │
│ grade math, forecast,   │          │ FTS5, Online Backup,    │
│ calendar & validation   │          │ sync_state, Idempotency │
└─────────────────────────┘          └─────────────────────────┘
```

## Dependency Direction (Acyclic)

1. `journal-domain` — zero dependency on Slint, SQLite, Axum, or `rmcp`.
2. `journal-storage` — depends on `journal-domain` + `rusqlite` (bundled SQLite + FTS5 + backup API).
3. `journal-app` — depends on `journal-domain` + `journal-storage` + export/notification utilities.
4. `journal-mcp` — depends on `journal-domain` + `journal-app` + `rmcp` + `axum`.
5. `journal-ui` — depends on `journal-domain` + `journal-app` + `journal-mcp` (for embedded HTTP server control) + `slint`.
6. `apps/student-journal` — thin desktop entry point launching Slint UI + background Tokio runtime.
7. `apps/student-journal-mcp` — standalone MCP server entry point (`stdio` default, `--http` optional loopback-only).

## Concurrency & Cross-Process Synchronization

- **Non-blocking UI**: The Slint event loop runs on the main thread; all SQLite I/O, backups, exports, notifications, and the embedded Streamable HTTP MCP server run on a dedicated multi-threaded Tokio background runtime. UI updates marshal safely back via `slint::invoke_from_event_loop`.
- **SQLite WAL & Locking**: Every connection configures `PRAGMA journal_mode = WAL`, `PRAGMA busy_timeout = 5000`, `PRAGMA foreign_keys = ON`, and `PRAGMA synchronous = NORMAL`. Writes run in explicit transactions.
- **GUI ↔ External MCP Sync**: Every mutating transaction increments `sync_state.revision` atomically. In-process writes notify instantly via `tokio::sync::broadcast`; external `student-journal-mcp` STDIO process writes are detected by the GUI's lightweight background `sync_state.revision` watcher (400 ms poll interval), triggering incremental cache invalidation and UI refresh without restarting the app.
- **Incremental Statistics Caching**: `StatisticsService` caches computed reports keyed by `(profile_id, sync_revision, filter_hash)` so repeated UI renders or unrelated edits do not redundantly scan large datasets.
