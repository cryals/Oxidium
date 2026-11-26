# Oxidium Execution State

Last updated: 2026-11-26 (Phase 00)

## Current Phase
- **Active**: Phase 08 (early acyclic build) — MCP Server (`crates/journal-mcp` + `apps/student-journal-mcp`), then Phase 04–07 Slint UI
- **Next**: Phase 04–07 — Slint MD3 Expressive UI (`crates/journal-ui` + `apps/student-journal`)

## Phase Tracker
| Phase | Name | Status | Verification |
| --- | --- | --- | --- |
| 00 | Environment, Assets & Docs | DONE | Toolchain (`rustc 1.100.0`), 8 local Roboto/RobotoFlex/RobotoMono TTFs, 42 Material Symbols SVGs, `oxidium.ico`, root & `docs/` created |
| 01 | Cargo Workspace & `journal-domain` | DONE | `cargo test -p journal-domain` + `proptest` + `clippy -D warnings` PASSED |
| 02 | SQLite Persistence & Migrations (`journal-storage`) | DONE | `cargo test -p journal-storage` (5 integration tests: migrations, FTS5, WAL concurrency, backup/restore) + `clippy -D warnings` PASSED |
| 03 | Shared Application Services (`journal-app`) | DONE | `cargo test -p journal-app` + `clippy -D warnings` PASSED (isolation, stats, forecast, undo, kanban, export/import/backup, secrets, i18n) |
| 04 | Slint MD3 Shell & Electronic Journal | PENDING | — |
| 05 | Profiles, Subjects & Academic Periods UI | PENDING | — |
| 06 | Statistics, Charts & Forecast Calculator UI | PENDING | — |
| 07 | Kanban Board & Drag-and-Drop UI | PENDING | — |
| 08 | MCP Server (`rmcp 3.11`, STDIO + Streamable HTTP, 80+ tools, Resources, Prompts) | PENDING | — |
| 09 | GUI ↔ MCP Cross-Process Synchronization | PENDING | — |
| 10 | Settings Persistence, Localization & Notifications | PENDING | — |
| 11 | Import, Export & Online Backup/Restore | PENDING | — |
| 12 | Security Hardening & Real Performance Benchmarks | PENDING | — |
| 13 | Comprehensive Testing & Visual Screenshot QA | PENDING | — |
| 14 | CI/CD, Release Packaging, Windows Installer & Readiness Report | PENDING | — |

## Resume Notes
- App icons provided by user at `E:\oxidium\assets\icon\` (`web/icon-512.png`, `oxidium.ico`, Android/iOS/Web assets).
- `material-1.1.0/` is unmodified Slint Material 3 library (do NOT edit).
- Verified crate versions locked in `Cargo.toml` workspace dependencies.
