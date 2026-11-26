# Oxidium Execution State

Last updated: 2026-11-26 (Phase 11 + Visual QA)

## Current Phase
- **Active**: Phase 12–13 — Security Hardening (`cargo audit`, `cargo deny`), Real Performance Benchmarks, and Full Workspace Test Suite
- **Next**: Phase 14 — CI/CD Workflows, Release Build, Windows Portable ZIP + Installer, SBOM & Production Readiness Report

## Phase Tracker
| Phase | Name | Status | Verification |
| --- | --- | --- | --- |
| 00 | Environment, Assets & Docs | DONE | Toolchain (`rustc 1.100.0`), 8 local Roboto/RobotoFlex/RobotoMono TTFs, 42 Material Symbols SVGs, `oxidium.ico`, root & `docs/` created |
| 01 | Cargo Workspace & `journal-domain` | DONE | `cargo test -p journal-domain` + `proptest` + `clippy -D warnings` PASSED |
| 02 | SQLite Persistence & Migrations (`journal-storage`) | DONE | `cargo test -p journal-storage` (5 integration tests: migrations, FTS5, WAL concurrency, backup/restore) + `clippy -D warnings` PASSED |
| 03 | Shared Application Services (`journal-app`) | DONE | `cargo test -p journal-app` + `clippy -D warnings` PASSED (isolation, stats, forecast, undo, kanban, export/import/backup, secrets, i18n) |
| 04 | Slint MD3 Shell & Electronic Journal | DONE | Native Slint 1.19.1 + `@material` Mono Lavender MD3 Expressive shell, synchronized pinned header/left/right columns, quick grade picker & detail editor; visually QA'd in light & dark |
| 05 | Profiles, Subjects & Academic Periods UI | DONE | Full modals with isolation, archive/restore, impact confirmation, and keyboard shortcuts |
| 06 | Statistics, Charts & Forecast Calculator UI | DONE | Custom AA line & donut charts, 1–5 bar chart, subject bars, heatmap, honest attendance & non-destructive forecast; visually QA'd |
| 07 | Kanban Board & Drag-and-Drop UI | DONE | Multi-board, custom columns, WIP limits, drag/drop & arrow moves, checklist progress, overdue highlight; visually QA'd |
| 08 | MCP Server (`rmcp 3.11`, STDIO + Streamable HTTP, 80+ tools, Resources, Prompts) | DONE | 80 tools, 6 resources, 7 prompts; `cargo test -p journal-mcp` + `stdio_tests` + `clippy -D warnings` PASSED; `docs/mcp/*` created |
| 09 | GUI ↔ MCP Cross-Process Synchronization | DONE | `SyncCoordinator` + 400ms live SQLite revision watcher wired into `UiController`; verified over real STDIO subprocess |
| 10 | Settings Persistence, Localization & Notifications | DONE | 6-tab Settings modal, Russian/English live switch, dynamic MD3 accent palette, OS notifications & reminder ticker verified |
| 11 | Import, Export & Online Backup/Restore | DONE | JSON/CSV/XLSX/Markdown export, JSON/CSV/Telegram import preview, rotating online ZIP backup & transactional restore wired in UI |
| 12 | Security Hardening & Real Performance Benchmarks | IN_PROGRESS | Running real benchmarks, `cargo audit`, `cargo deny`, and full workspace checks |
| 13 | Comprehensive Testing & Visual Screenshot QA | DONE | Headless UI E2E test (`ui_tests.rs`) PASSED + all 5 1440×900 PNGs visually inspected & layout defects fixed |
| 14 | CI/CD, Release Packaging, Windows Installer & Readiness Report | PENDING | — |

## Resume Notes
- App icons provided by user at `E:\oxidium\assets\icon\` (`web/icon-512.png`, `oxidium.ico`, Android/iOS/Web assets).
- `material-1.1.0/` is unmodified Slint Material 3 library (do NOT edit).
- Verified crate versions locked in `Cargo.toml` workspace dependencies.
