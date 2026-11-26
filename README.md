# Oxidium — Student Journal

> Русская версия документации: [README.ru.md](README.ru.md)

**Oxidium** is a free, open-source, local-first, offline-first personal **Student Journal** desktop application built with **Rust**, **Slint (Material Design 3 Expressive)**, **SQLite**, and a full built-in **Model Context Protocol (MCP)** server.

Replace clunky institutional portals, scattered spreadsheets, notes, and Telegram Saved Messages with a fast, native desktop workspace that both you and your AI assistants can use together.

## Highlights

- **Native Desktop Performance**: 100% native Rust + Slint UI (no Electron, Tauri, WebView, or browser runtime).
- **Local-First & Offline-First**: Single local SQLite database (WAL + FTS5 + online backup), zero telemetry, zero mandatory registration, zero CDN dependencies.
- **Electronic Journal**: Synchronized pinned headers and columns, virtualized grid for 100+ subjects, quick grade entry (`1–5`, `Н/Б`, `Уваж.`, `Зач.`, `Н/З`, and custom mark types), multi-grade cells, exact `Decimal` arithmetic & weighted averages, leap-year calendar, and undo/redo.
- **Multi-Profile Isolation**: Independent academic profiles (University, College, School, Courses) with isolated subjects, periods, grades, attendance, tasks, and analytics.
- **Statistics & Grade Forecast Calculator**: 8 real-data interactive charts (cumulative average dynamics, grade distribution, subject averages, donut breakdown, attendance, activity heatmap, task completion, weekly/monthly dynamics), two-period comparison, and exact grade forecasting.
- **Kanban Task Management**: Multiple boards per profile, dynamic columns with WIP limits and completion flags, drag-and-drop, checklists, tags, custom badges, Markdown notes, deadlines, reminders, and subject/grade links.
- **Production MCP Server (`rmcp`)**: Both **STDIO** (`student-journal-mcp`) and loopback-only **Streamable HTTP** with Bearer token auth, 80+ typed tools, MCP Resources, MCP Prompts, granular permissions (`ReadOnly` / `ReadWrite` / `Destructive`), idempotency keys, activity audit log with Undo, and live cross-process GUI synchronization.

## Building & Running

```sh
# Build workspace
cargo build --workspace

# Run desktop GUI
cargo run -p student-journal

# Run standalone MCP STDIO server
cargo run -p student-journal-mcp

# Run all checks & tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Database Schema](docs/DATABASE.md)
- [MCP Documentation](docs/mcp/QUICKSTART.md)
- [Testing](docs/TESTING.md)
- [Production Readiness Report](PRODUCTION-READINESS-REPORT.md)
- [Security Policy](SECURITY.md)

## License

Oxidium source code is licensed under the MIT License ([LICENSE.txt](LICENSE.txt)). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled third-party components including Slint, Material Components, Google Fonts (SIL OFL 1.1), and Material Symbols (Apache 2.0).
