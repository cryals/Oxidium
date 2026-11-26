# Oxidium — Agent Instructions

Oxidium is a free, open-source, local-first, offline-first desktop application (**Электронный журнал ученика / Student Journal**) built with **Rust**, **Slint (Material Design 3 Expressive)**, **SQLite**, and an embedded/standalone **Model Context Protocol (MCP)** server.

Repository: `https://github.com/cryals/Oxidium`

## Architecture & Workspace Rules

- **Cargo Workspace layout**:
  - `crates/journal-domain/` — pure domain entities, validation, exact `rust_decimal` grade calculations & forecast math, calendar/leap-year engine, undo/redo commands. **Zero dependency on Slint, SQLite, or MCP.**
  - `crates/journal-storage/` — SQLite persistence (`rusqlite` bundled + WAL + FTS5 + online backup), versioned SQL migrations in `migrations/`, `sync_state` revision counter, MCP idempotency store.
  - `crates/journal-app/` — shared Application Services used identically by both Slint GUI and MCP server, i18n (`ru` primary, `en`), notifications, import/export, backup/restore, activity log, sync coordinator.
  - `crates/journal-mcp/` — `rmcp` MCP server with STDIO & Streamable HTTP transports, granular permissions (`ReadOnly`, `ReadWrite`, `Destructive`), ~80 typed tools, MCP Resources, MCP Prompts, and activity logging.
  - `crates/journal-ui/` — Slint MD3 Expressive UI (`ui/`), custom chart renderer, Rust view-model controller, headless screenshot/test harness.
  - `apps/student-journal/` — main desktop GUI binary (`student-journal`, also supports `--mcp-stdio` and `--render-screenshots`).
  - `apps/student-journal-mcp/` — standalone MCP server binary (`student-journal-mcp`, STDIO default, `--http` optional loopback-only).
- **Single business logic path**: GUI and MCP MUST both call `journal-app` services. Never duplicate domain or persistence logic in UI or MCP layers.
- **No circular dependencies**: `journal-domain` ← `journal-storage` ← `journal-app` ← (`journal-mcp`, `journal-ui`) ← `apps/*`.
- **Do not edit `material-1.1.0/`**: Import Material 3 components via `@material`.

## Security, Reliability & Data Integrity

- Every SQL query with user data MUST be parameterized. Never expose arbitrary SQL, shell, or filesystem MCP tools.
- Never log or commit MCP Bearer tokens; store them only in the dedicated local secret file outside SQLite and Git.
- Never contaminate `stdout` in STDIO MCP mode — all tracing/diagnostics go strictly to `stderr`.
- All Streamable HTTP servers bind strictly to loopback (`127.0.0.1`) with Bearer token auth (`subtle::ConstantTimeEq`) and `Host`/`Origin` validation.
- All assets (fonts in `assets/fonts/`, icons in `assets/icons/`, app icons in `assets/icon/`) MUST remain local — zero CDN fetches at runtime.
- No fake/mock data in production mode. Empty states must render gracefully.
- Never block the Slint UI thread with heavy SQLite, export, backup, or network I/O.

## Build, Test & Verification Commands

```sh
# Format, lint, and test entire workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Live preview during UI iteration
SLINT_LIVE_PREVIEW=1 cargo run -p student-journal --features slint/live-preview

# Render headless screenshots for visual QA
cargo run -p student-journal -- --render-screenshots docs/screenshots

# Security and license audits
cargo audit
cargo deny check

# Release build
cargo build --release --workspace
```

## Working Guidelines for Subsequent AI Agents

1. Read `docs/EXECUTION-STATE.md` and `docs/TASKS.md` first to resume cleanly.
2. Update `docs/EXECUTION-STATE.md` after every substantial milestone.
3. Use atomic Conventional Commits (`feat(scope): ...`, `fix(scope): ...`, `test: ...`, `docs: ...`).
4. Never push to remote repositories or publish user data without explicit user permission.
5. After visible UI changes, regenerate and visually inspect screenshots in `docs/screenshots/`.
