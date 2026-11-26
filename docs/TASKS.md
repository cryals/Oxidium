# Oxidium Master Task List

- [x] Inspect environment, Git, Rust 1.100.0, `material-1.1.0`, user app icons (`assets/icon/`)
- [x] Phase 00: Local fonts (`assets/fonts/`), Material Symbols SVGs (`assets/icons/`), `oxidium.ico`, root & `docs/` documentation
- [x] Phase 01: Cargo Workspace + `crates/journal-domain` (models, exact decimal average/forecast, calendar/leap years, validation, proptest)
- [x] Phase 02: `migrations/` + `crates/journal-storage` (20 tables, WAL, FTS5 Russian/English search, online backup/restore, sync revision, idempotency)
- [x] Phase 03: `crates/journal-app` (all shared services, stats caching, undo/redo, i18n ru/en, notifications, export/import/Telegram preview, backup rotation)
- [x] Phase 04–07: `crates/journal-ui` + `apps/student-journal` (MD3 Expressive UI, pinned journal matrix, quick grades, profiles/subjects/periods, real charts + forecast calculator, Kanban + moves/checklists, settings/MCP panel/notifications, headless screenshot QA)
- [x] Phase 08–09: `crates/journal-mcp` + `apps/student-journal-mcp` (80 tools, 6 Resources, 7 Prompts, STDIO + loopback Streamable HTTP with Bearer auth, permissions, idempotency, activity log, GUI sync, `docs/mcp/*`)
- [x] Phase 10–12: Full settings persistence, security audit (`cargo audit`, `cargo deny`), SQL parameterization audit, real release performance benchmarks (100 subjects, 10k/100k grades, 10k tasks)
- [x] Phase 13: Full test suite (unit, proptest, storage, app, MCP STDIO/HTTP, concurrency, headless Slint UI E2E + 100-subject UI stress test)
- [x] Phase 14: GitHub/Forgejo CI workflows, Windows portable ZIP + verified per-user HKCU Windows installer, Linux packaging scripts, SHA-256 checksums, CycloneDX 1.5 SBOM, `PRODUCTION-READINESS-REPORT.md`
