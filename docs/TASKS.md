# Oxidium Master Task List

- [x] Inspect environment, Git, Rust 1.100.0, `material-1.1.0`, user app icons (`assets/icon/`)
- [ ] Phase 00: Local fonts (`assets/fonts/`), Material Symbols SVGs (`assets/icons/`), `oxidium.ico`, root & `docs/` documentation
- [ ] Phase 01: Cargo Workspace + `crates/journal-domain` (models, exact decimal average/forecast, calendar/leap years, validation, proptest)
- [ ] Phase 02: `migrations/` + `crates/journal-storage` (20+ tables, WAL, FTS5 Russian/English search, online backup/restore, sync revision, idempotency)
- [ ] Phase 03: `crates/journal-app` (all shared services, stats caching, undo/redo, i18n ru/en, notifications, export/import/Telegram preview, backup rotation)
- [ ] Phase 04–07: `crates/journal-ui` + `apps/student-journal` (MD3 Expressive UI, pinned virtualized journal, quick grades, profiles/subjects/periods, 8 real charts + forecast calculator, Kanban + drag-and-drop, settings/MCP panel/notifications, headless screenshot verification)
- [ ] Phase 08–09: `crates/journal-mcp` + `apps/student-journal-mcp` (80 tools, Resources, Prompts, STDIO + loopback Streamable HTTP with Bearer auth, permissions, idempotency, activity log, GUI sync, `docs/mcp/*`)
- [ ] Phase 10–12: Full settings persistence, security audit (`cargo audit`, `cargo deny`), real performance benchmarks (100 subjects, 10k/100k grades, 10k tasks)
- [ ] Phase 13: Full test suite (unit, proptest, storage, app, MCP STDIO/HTTP, concurrency, headless Slint UI tests)
- [ ] Phase 14: GitHub/Forgejo CI workflows, Windows portable ZIP + real Windows installer, Linux packaging scripts, SHA-256 checksums, CycloneDX SBOM, `PRODUCTION-READINESS-REPORT.md`
