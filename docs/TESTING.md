# Oxidium Testing Strategy

Oxidium verifies correctness across every layer rather than treating successful compilation as proof of readiness.

## Test Suites
1. **Domain Unit & Property-Based Tests (`crates/journal-domain`)**:
   - Exact `Decimal` arithmetic mean and weighted mean (`proptest` invariants: bounds, monotonicity, scale invariance, non-counting marks ignored, empty returns `None`).
   - Grade forecasting reachability and ceiling counts.
   - Calendar month lengths and leap years (2000, 2024, 2025, 2100).
   - Validation boundaries.
2. **Storage & Migration Tests (`crates/journal-storage`)**:
   - Clean migration execution, checksum verification, idempotency on restart.
   - Referential integrity (`FOREIGN KEY`, `CHECK`, `UNIQUE`).
   - FTS5 search across Russian and English text with filters.
   - Online SQLite backup, integrity checks, corrupt-file rejection, and rollback on failed restore.
3. **Application Service Tests (`crates/journal-app`)**:
   - Strict profile isolation across subjects, grades, attendance, boards, tasks, and statistics.
   - Multi-grade days, undo/redo stacks, academic periods, custom mark types, honest attendance metrics.
   - Export/import round-trips (JSON, CSV, XLSX, Markdown, Full ZIP backup, Telegram Desktop JSON preview).
   - Localization (`ru`/`en`, Russian pluralization) and persistent settings.
4. **MCP Protocol, Contract & Security Tests (`crates/journal-mcp`, `tests/`)**:
   - Real STDIO child-process JSON-RPC test spawning `student-journal-mcp`.
   - Real Streamable HTTP test (`initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `prompts/list`, `prompts/get`).
   - 401 rejection without valid Bearer token; permission denial (`ReadOnly` rejects writes, `Destructive` blocks deletes unless enabled).
   - Idempotency key replay verification (no duplicate grades/tasks).
   - End-to-end execution of all 9 required Russian natural-language scenarios.
5. **Concurrency & Cross-Process Sync Tests (`tests/`)**:
   - Multi-threaded / multi-connection simultaneous GUI + MCP writes confirming zero lost writes and `sync_state.revision` propagation.
6. **Headless Slint UI Interaction & Visual Tests (`crates/journal-ui`)**:
   - Automated interaction tests via `i-slint-backend-testing` (tab switching, quick grade entry, subject creation, Kanban moves, theme changes).
   - High-resolution software-rendered PNG screenshots (`docs/screenshots/`) visually inspected.
7. **Real Performance Benchmarks (`tests/perf_benchmarks.rs`)**:
   - Measured on 100 subjects, 10,000 grades, 100,000 grades, 10,000 tasks, and multiple profiles.
8. **Security & License Audits**:
   - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo audit`, `cargo deny check`.
