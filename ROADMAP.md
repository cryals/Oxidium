# Oxidium Roadmap

## v0.1.0 — Production Foundation (Current Release)
- [x] Phase 00: Environment audit, local typography/icons/app icon, core documentation
- [ ] Phase 01: Cargo Workspace & pure `journal-domain` (exact decimal grade math, forecasting, calendar/leap-year engine, validation)
- [ ] Phase 02: SQLite persistence (`rusqlite` bundled, WAL, versioned migrations, FTS5 search, online backup/restore)
- [ ] Phase 03: Shared Application Services (`journal-app`), i18n (`ru`/`en`), notifications, import/export, activity log
- [ ] Phase 04–07: Native Slint MD3 Expressive UI — Electronic Journal (pinned headers/columns, virtualized grid, quick grade picker, multi-grade cells, undo/redo), Profiles & Subjects Manager, Statistics & 8 Real-Data Charts + Grade Forecast Calculator, Kanban Board with Drag-and-Drop, Settings & Notifications
- [ ] Phase 08–09: Full `rmcp` MCP Server (STDIO + Loopback Streamable HTTP with Bearer auth, 80+ tools, Resources, Prompts, granular permissions, idempotency, activity log) + Cross-Process GUI/MCP Synchronization
- [ ] Phase 10–12: Settings persistence, automatic rotating backups, security audit (`cargo audit`/`cargo deny`), real performance benchmarks (100 subjects / 10k & 100k grades / 10k tasks)
- [ ] Phase 13–14: Comprehensive tests (unit, property, integration, MCP contract, concurrency, headless Slint UI), CI/CD workflows (GitHub Actions & Forgejo Actions), Windows portable & installer packaging, Linux packaging scripts, SHA-256 checksums, SBOM, Production Readiness Report

## v0.2.0 — Post-Release Enhancements
- [ ] Optional OS background daemon / system tray mode for reminders while main window is closed
- [ ] Schedule / timetable integration (optional class count tracking to unlock exact attendance percentage calculation)
- [ ] Additional localizations (e.g., Ukrainian, Kazakh, German)
- [ ] Signed Windows MSIX / macOS Apple Silicon DMG & Linux Flatpak distribution
- [ ] Optional end-to-end encrypted local sync archive export for multi-device manual transfer
