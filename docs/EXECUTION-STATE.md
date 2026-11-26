# Oxidium Execution State

Last updated: 2026-11-26 (Phase 15 — COMPLETE & RELEASED v0.1.0)

## Current Phase
- **Active**: Completed all Phases 00–15 (including Stitch-inspired UX overhaul, first-run auto-onboarding, dynamic GitHub Releases semi-automatic updater, and CI/CD release pipeline)
- **Next**: Pushing `main`, tagging `v0.1.0`, and publishing GitHub Release `v0.1.0`

## Phase Tracker
| Phase | Name | Status | Verification |
| --- | --- | --- | --- |
| 00 | Environment, Assets & Docs | DONE | Toolchain (`rustc 1.100.0`), local Roboto/RobotoFlex/RobotoMono TTFs, 49 Material Symbols SVGs, `oxidium.ico`, root & `docs/` created |
| 01 | Cargo Workspace & `journal-domain` | DONE | 11 tests (unit + `proptest`) + `clippy -D warnings` PASSED |
| 02 | SQLite Persistence & Migrations (`journal-storage`) | DONE | 5 integration tests (migrations, FTS5, WAL concurrency, online backup/restore) + `clippy -D warnings` PASSED |
| 03 | Shared Application Services (`journal-app`) | DONE | 3 integration tests + 3 updater tests + 2 benchmark suites (10k + 100k grades/10k tasks) + `clippy -D warnings` PASSED |
| 04 | Slint MD3 Shell & Electronic Journal | DONE | Native Slint 1.19.1 + `@material` Mono Lavender MD3 Expressive shell, expandable navigation rail, synchronized pinned header/left/right columns, Cursor/Hand pan mode, inline search, quick grade picker & detail editor; visually QA'd in light & dark |
| 05 | Profiles, Subjects & Academic Periods UI | DONE | Quick profile popover + full modals with strict isolation, archive/restore, delete impact confirmation, and keyboard shortcuts |
| 06 | Statistics, Charts & Forecast Calculator UI | DONE | Custom 2× DPI AA line & donut charts with smart Y-axis & target line, 1–5 pill distribution, subject bars, heatmap, honest attendance, 1-click report export & non-destructive forecast; visually QA'd |
| 07 | Kanban Board & Drag-and-Drop UI | DONE | Multi-board, custom columns, WIP limits, Cursor/Hand pan mode, inline search, drag/drop & arrow moves, MCP badges, checklist progress, overdue highlight; visually QA'd |
| 08 | MCP Server (`rmcp 3.11`, STDIO + Streamable HTTP, 80 tools, Resources, Prompts) | DONE | 80 tools, 6 resources, 7 prompts; `mcp_tests` + `stdio_tests` + `clippy -D warnings` PASSED; `docs/mcp/*` created |
| 09 | GUI ↔ MCP Cross-Process Synchronization | DONE | `SyncCoordinator` + 400ms live SQLite revision watcher wired into `UiController`; verified over real STDIO subprocess |
| 10 | Settings Persistence, Localization & Notifications | DONE | 6-tab Settings modal, Russian/English live switch, dynamic MD3 accent palette, OS notifications & reminder ticker verified |
| 11 | Import, Export & Online Backup/Restore | DONE | JSON/CSV/XLSX/Markdown export, JSON/CSV/Telegram import preview, rotating online ZIP backup & transactional restore wired in UI |
| 12 | Security Hardening & Real Performance Benchmarks | DONE | `cargo audit` (0 vulns across 700 deps), `cargo deny check` (`advisories/bans/licenses/sources ok`), SQL parameterization audited, release 100k-grade / 10k-task benchmarks PASSED |
| 13 | Comprehensive Testing & Visual Screenshot QA | DONE | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, 26/26 default + 1/1 ignored release scale test PASSED, 9 1440×900 PNGs visually verified |
| 14 | Dynamic Updater, First-Run Onboarding & Release CI/CD | DONE | Semi-automatic GitHub Releases updater (`updater.rs` + `UpdateModal` + top-bar pill + SHA-256 verification), auto first-run 4-step Setup Wizard (`onboarding_completed`), `.github/workflows/release.yml`, Windows portable ZIP + native HKCU installer + CycloneDX 1.5 SBOM + `SHA256SUMS.txt` |
| 15 | M3 Expressive UI, Smooth Spline/Donut Charts, Multi-Type Heatmap & Motion | DONE | Fritsch-Carlson spline chart (`charts.rs`), rounded-cap M3 donut with `%` legend, semantic subject bars with target ticks, stacked attendance bars, 5×7 Mon–Sun multi-color split sub-cell heatmap with hover inspector, fixed Settings `Switch`/button stretching and card overflow, unclipped Kanban hints & M3 animations; `clippy -D warnings`, 26/26 tests & all 15 QA PNGs verified |

## Resume Notes
- App icons integrated from `assets/icon/` (`web/icon-192.png`, `web/icon-512.png`, `oxidium.ico`).
- `material-1.1.0/` remains 100% unmodified.
- Release artifacts generated locally under `dist/` (gitignored; reproducible via `scripts/package-windows.ps1` and `scripts/package-linux.sh`).
