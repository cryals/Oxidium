# Oxidium Execution State

Last updated: 2026-11-27 (Phase 16 — INDEPENDENT AUDIT REMEDIATION & RELEASE v0.1.1)

## Current Phase
- **Active**: Completed Phase 16 — Independent Review audit remediation, visual polish, updater/MCP security hardening, and version bump to `v0.1.1`
- **Next**: Finalize verification, package Windows `0.1.1`, push `main`, tag `v0.1.1`, and publish GitHub Release `v0.1.1`

## Phase Tracker
| Phase | Name | Status | Verification |
| --- | --- | --- | --- |
| 00 | Environment, Assets & Docs | DONE | Toolchain (`rustc 1.100.0`), local Roboto/RobotoFlex/RobotoMono TTFs, 49 Material Symbols SVGs, `oxidium.ico`, root & `docs/` created |
| 01 | Cargo Workspace & `journal-domain` | DONE | 11 tests (unit + `proptest`) + `clippy -D warnings` PASSED |
| 02 | SQLite Persistence & Migrations (`journal-storage`) | DONE | 5 integration tests (migrations, FTS5, WAL concurrency, online backup/restore) + `clippy -D warnings` PASSED |
| 03 | Shared Application Services (`journal-app`) | DONE | 3 integration tests + 4 updater tests + 2 benchmark suites (10k + 100k grades/10k tasks) + `clippy -D warnings` PASSED |
| 04 | Slint MD3 Shell & Electronic Journal | DONE | Native Slint 1.19.1 + `@material` Mono Lavender MD3 Expressive shell, expandable navigation rail, synchronized pinned header/left/right columns, Cursor/Hand pan mode, inline search, `Week`/`Year`/`Semester` navigation, `Return`/`Delete` shortcuts, quick grade picker & detail editor; visually QA'd in light & dark |
| 05 | Profiles, Subjects & Academic Periods UI | DONE | Quick profile popover + full modals with strict isolation, archive/restore, delete impact confirmation, period-type selector, and placeholders |
| 06 | Statistics, Charts & Forecast Calculator UI | DONE | `1440x320` monotone cubic spline + non-overlapping rounded-cap donut with aligned `%` legend, semantic subject bars with target ticks, `5x7` multi-color split sub-cell heatmap with `week_start` support & hover inspector, honest attendance, attention filter, non-blocking report export & archive-safe forecast; visually QA'd |
| 07 | Kanban Board & Drag-and-Drop UI | DONE | Multi-board, custom columns (`is_completed_column` toggle), WIP limits, Cursor/Hand pan mode, inline search, dismissible subject filter chip, drag/drop & arrow moves, MCP badges, subject/priority/notes/checklist controls, unclipped hints; visually QA'd |
| 08 | MCP Server (`rmcp 3.11`, STDIO + Streamable HTTP, 80 tools, Resources, Prompts) | DONE | 80 tools, 6 resources, 7 prompts; `mcp_tests` (incl. non-loopback `Origin` 403) + `stdio_tests` + synchronous bind `EmbeddedHttpManager` + `clippy -D warnings` PASSED |
| 09 | GUI ↔ MCP Cross-Process Synchronization | DONE | `SyncCoordinator` + 400ms live SQLite revision watcher wired into `UiController`; verified over real STDIO subprocess |
| 10 | Settings Persistence, Localization & Notifications | DONE | 6-tab Settings modal (`week_start` selector, dark-mode accessible mark palette), Russian/English live switch, dynamic MD3 accent palette, OS notifications & reminder ticker verified |
| 11 | Import, Export & Online Backup/Restore | DONE | Non-blocking `spawn_blocking` file dialogs + ZIP/JSON/CSV/XLSX/Markdown I/O, JSON attendance restore, SHA-256 + 512 MiB bounded backup archive restore |
| 12 | Security Hardening & Real Performance Benchmarks | DONE | `cargo audit` (0 vulns), `cargo deny check` (`advisories/bans/licenses/sources ok`), SQL parameterization audited, fail-closed updater SHA-256 + shell-injection-free `rundll32.exe` URL launcher, strict `Origin` header check |
| 13 | Comprehensive Testing & Visual Screenshot QA | DONE | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, all tests & regenerated 1440×900 PNGs visually verified |
| 14 | Dynamic Updater, First-Run Onboarding & Release CI/CD | DONE | Semi-automatic GitHub Releases updater with `--wait-pid`/`--relaunch`/`.old` executable swap, tag/Cargo version guard & single-version release notes in `.github/workflows/release.yml`, unified `0.1.1` versioning |
| 15 | M3 Expressive UI, Smooth Spline/Donut Charts, Multi-Type Heatmap & Motion | DONE | Fritsch-Carlson spline chart, rounded-cap donut, semantic bars, `5x7` mosaic heatmap, Settings layout fixes |
| 16 | Independent Audit Remediation (`v0.1.1`) | DONE | All P0, P1 & polish findings from independent code & UX review resolved, zero Slint easing warnings, dark-mode contrast fixed |

## Resume Notes
- App icons integrated from `assets/icon/` (`web/icon-192.png`, `web/icon-512.png`, `oxidium.ico`).
- `material-1.1.0/` remains 100% unmodified.
- Release artifacts generated locally under `dist/` (gitignored; reproducible via `scripts/package-windows.ps1` and `scripts/package-linux.sh`).
