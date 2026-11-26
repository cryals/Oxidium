# Contributing to Oxidium

Thank you for your interest in contributing to **Oxidium**!

## Development Prerequisites
- Rust toolchain (`rustc` >= 1.95, Rust edition 2024)
- Git
- Optional tools: `cargo-audit`, `cargo-deny`, `slint-lsp`, `slint-viewer`
- On Linux: standard Slint desktop development libraries (`libfontconfig1-dev`, `libxkbcommon-dev`, `libwayland-dev`, `libgl1-mesa-dev`, `pkg-config`).

## Project Structure
Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [AGENTS.md](AGENTS.md) before opening a pull request.
- Keep `crates/journal-domain` completely independent of Slint, SQLite, and MCP.
- Route all mutations from both UI and MCP through `crates/journal-app`.
- Do not modify files inside `material-1.1.0/`.

## Workflow
1. Fork or branch from `main`.
2. Write clear, small commits following Conventional Commits (`feat(journal): ...`, `fix(mcp): ...`, `test(storage): ...`).
3. Ensure all checks pass locally:
   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo audit
   cargo deny check
   ```
4. For UI changes, verify visual output with `cargo run -p student-journal -- --render-screenshots docs/screenshots`.
5. Submit a Pull Request describing what changed and how it was verified.
