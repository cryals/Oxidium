# Changelog

All notable changes to **Oxidium** are documented in this file.
This project adheres to [Semantic Versioning](https://semver.org/) and [Conventional Commits](https://www.conventionalcommits.org/).

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
- CI/CD workflows (GitHub Actions & Forgejo Actions), Windows portable ZIP & installer, Linux packaging scripts, SHA-256 checksums, and CycloneDX SBOM.
