# Architectural Decision Records (ADRs)

## ADR-001: SQLite via `rusqlite` (bundled) over `sqlx`
- **Decision**: Use `rusqlite = "0.43.0"` with `features = ["bundled", "backup", "chrono", "uuid", "serde_json"]`.
- **Rationale**: Compiles modern SQLite with FTS5 natively on Windows and Linux without system library dependencies; exposes SQLite's official Online Backup API (`rusqlite::backup::Backup`) required for WAL-safe consistent backups; zero compile-time macro DB connection overhead; deterministic synchronous transactions wrapped cleanly on background worker threads when called from async contexts.

## ADR-002: Exact Grade Calculations with `rust_decimal`
- **Decision**: Perform all arithmetic and weighted average calculations using `rust_decimal::Decimal`.
- **Rationale**: Eliminates binary floating-point drift (`4.499999999999999` vs `4.50`), prevents intermediate rounding errors, and rounds strictly at display formatting time (1, 2, or 3 decimal places).

## ADR-003: Official Rust MCP SDK (`rmcp` 3.11) + Loopback Axum Middleware
- **Decision**: Use `rmcp = "3.11.0"` for both STDIO (`transport-io`) and Streamable HTTP (`transport-streamable-http-server`) mounted on `axum = "0.8.9"`.
- **Rationale**: Implements current MCP specification (`2025-03-26` through `2026-07-28`), modular `+`-composable `ToolRouter`, `PromptRouter`, `Resources`, built-in `Host`/`Origin` loopback validation, and request body size limits. Layered with constant-time Bearer token auth (`subtle::ConstantTimeEq`) and server-side Rust permission gates.

## ADR-004: Cross-Process GUI ↔ MCP Synchronization via Atomic SQLite Revision Counter
- **Decision**: Maintain a single-row `sync_state` table whose `revision` increments inside every mutating SQLite transaction, paired with an in-process `tokio::sync::broadcast` channel and a lightweight 400 ms background revision check in the GUI.
- **Rationale**: Works identically across Windows and Linux without fragile OS-specific named-pipe protocols, handles multiple concurrent external STDIO MCP processes reliably, never blocks the UI thread, and avoids redundant UI rebuilds when `revision` is unchanged.

## ADR-005: Synchronized Virtualized Journal Table in Slint
- **Decision**: Bind the pinned date header's `x` directly to the grid `ScrollView.viewport_x`, bind both the pinned subject column's `y` and pinned average column's `y` directly to `ScrollView.viewport_y`, and window-virtualize rendered rows based on scroll offset.
- **Rationale**: Guarantees mathematically zero scroll desynchronization on every frame while keeping UI layout cost bounded even with 100+ subjects.

## ADR-006: Native Slint Bars/Heatmaps + 2× DPI Pure-Rust Anti-Aliased Canvas for Curves & Donuts
- **Decision**: Render bar charts, horizontal subject bars, and activity heatmaps with native interactive Slint M3 components, and render smooth cumulative average line curves and donut charts into crisp 2× DPI `SharedPixelBuffer<Rgba8Pixel>` images overlaid with native interactive hover points and tooltips.
- **Rationale**: Delivers crisp, expressive, theme-aware charts with zero external plotting C-dependencies while remaining 100% native Slint and interactive.

## ADR-007: Secret Storage Separation
- **Decision**: Store the MCP HTTP Bearer token in a dedicated local file (`mcp-token.secret`, chmod `0600` on Unix) next to the database rather than inside SQLite `settings` or `activity_log`.
- **Rationale**: Prevents accidental export of authentication credentials inside database backups, JSON exports, Git commits, or audit logs.
