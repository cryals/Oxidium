# Changelog

All notable changes to **Oxidium** are documented in this file.
This project adheres to [Semantic Versioning](https://semver.org/) and [Conventional Commits](https://www.conventionalcommits.org/).

## [0.1.2] - 2026-11-27

### Fixed
- **SQL Migration EOL Checksum Mismatch (CI Windows vs Local LF)**: Added `.gitattributes` enforcing canonical `LF` line endings across SQL migrations, scripts, and source files, normalized SQL text to `LF` before hashing in `journal-storage`, and added automatic migration checksum healing when opening databases previously initialized with `CRLF` migration hashes.
- **Visible Startup Error Reporting on Windows**: Added structured startup error logging to `<db_dir>/startup-error.log` and a native `rfd::MessageDialog` error popup when launching the GUI fails (instead of exiting silently under `windows_subsystem = "windows"`), suppressible via `OXIDIUM_NO_ERROR_DIALOG=1`.
- **Windows In-Place Installer `.old` Parking**: Added PID-suffixed `{exe}.{pid}.old` fallback and `.old` directory sweeping in `windows_installer.rs` so updates succeed cleanly even if an existing `.old` binary is locked by a running background MCP process.

## [0.1.1] - 2026-11-27

### Added
- **Material Design 3 Expressive Motion & Visual Overhaul**: Added smooth M3 emphasized/standard motion tokens (`OxidiumMotion`) respecting `reduced_motion`, animated navigation rail transitions, auto-dismissing snackbars (`4s`), and interactive chart hover scrubbers with live guidelines.
- **Monotone Cubic Spline & Rounded-Cap Donut Charts**: Replaced polyline segments with Fritsch-Carlson monotone cubic spline interpolation (`1440x320`) with single-pass vertical gradient shading and zero seam artifacts; upgraded donut chart to rounded-cap pills with non-overlapping sliver geometry and aligned percentage columns.
- **Multi-Type 5×7 Activity Heatmap**: Upgraded from binary boxes to a calendar heatmap supporting single-type scaling (`5/4` green, `3` amber, `2` warm brown, `Н/Б` red, `Уваж.` blue, tasks purple), 2/3/4-way mosaic sub-cell splits on mixed days, `week_start` alignment, localized headers, and a dedicated Hover Detail Inspector.
- **Interactive Controls & Keyboard Navigation**: Added `Return` (open cell detail) and `Delete` (delete latest entry) shortcuts in Journal, work-type chips and move-entry action in Cell Detail, `week_start` selector in Settings → Journal, dismissible subject filter chip + subject/priority/notes/completion controls in Kanban modals, and localized period/search placeholders.

### Fixed
- **Windows In-App Self-Updater**: Added `--wait-pid`, `--relaunch`, and executable `.old` parking in `windows_installer.rs`, failing closed if `SHA256SUMS.txt` digest is missing, hardening `html_url` & browser opening against `cmd.exe` injection, and gracefully exiting the GUI during unattended upgrades so locked `.exe` replacement succeeds.
- **Version Sync & Release CI Guard**: Unified version propagation across `Cargo.toml`, `package-windows.ps1`, `package-linux.sh`, `windows_installer.rs`, `oxidium.iss`, and `generate-sbom.py`, and enforced tag/workspace version validation plus single-version release notes extraction in `.github/workflows/release.yml`.
- **MCP Streamable HTTP Lifecycle & Security**: Added strict loopback `Origin` header validation (`403 Forbidden` on non-loopback origins), synchronous TCP socket binding in `EmbeddedHttpManager::start`, proper `restart()` shutdown wait, and direct port saving from UI controls.
- **UI/Backend Reliability & Layout**: Fixed `SettingsToggleRow` double-trigger bug, eliminated `OnboardingModal` Step 1 dead space and `UpdateModal` `ScrollView` height bugs, preserved accessible dark-mode grade/mark contrast across views and modals, fixed archived-subject off-by-N indexing in Forecast/Kanban pickers, restored attendance import in `import_json_into_profile`, enforced bounded + SHA-256-verified backup archive restore, offloaded file dialogs and ZIP/XLSX I/O off the UI thread, and eliminated all Slint `easing` compiler warnings.

## [0.1.0] - 2026-11-26

### Added
- Native Rust + Slint (Material Design 3 Expressive) desktop application for Windows and Linux (`student-journal`).
- Standalone and embedded Model Context Protocol (MCP) server (`student-journal-mcp`) over STDIO and loopback Streamable HTTP with Bearer token authentication, granular permissions, idempotency keys, Resources, Prompts, and 80+ typed tools.
- Local-first SQLite database with WAL mode, versioned migrations, FTS5 full-text search (Russian & English), online backup API, safe restore with rollback, and cross-process GUI/MCP synchronization.
- Multi-profile academic isolation, subject management with drag-and-drop reordering, flexible academic periods, customizable mark types, and work types.
- Interactive Electronic Journal with synchronized pinned headers/columns, virtualized rows, quick grade picker, multi-grade cells, keyboard shortcuts, and undo/redo.
- Statistics screen with 8 real-data visualizations, period comparison, and Grade Forecast Calculator.
- Full Kanban board with multiple boards per profile, customizable columns (WIP limits, completion flags), drag-and-drop tasks/columns, checklists, tags, custom badges, Markdown notes, and subject/grade links.
- Local notification center and deadline reminders, JSON/CSV/XLSX/Markdown export, JSON/CSV & Telegram Desktop preview import, and Russian/English localization.
- Dynamic semi-automatic update checker backed by GitHub Releases (`cryals/Oxidium`) with non-intrusive top-bar update pill, release notes modal, automatic `SHA256SUMS.txt` integrity verification, and first-run 4-step expressive Setup Wizard.
- Automated CI/CD workflows (`.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.forgejo/workflows/ci.yml`), Windows portable ZIP & per-user HKCU installer, Linux `.tar.gz`/`.deb` packaging scripts, unified SHA-256 checksums, and CycloneDX 1.5 SBOM.
