# Security Policy

## Supported Versions
| Version | Supported |
| --- | --- |
| 0.1.x | Yes |

## Security Model of Oxidium
Oxidium is a **local-first, offline-first** desktop application:
- No cloud backend, no telemetry, no analytics, no remote font/icon CDNs.
- All academic records reside in a local SQLite database owned by the user.

### MCP Server Security Controls
1. **Loopback-only HTTP binding**: The optional Streamable HTTP server binds exclusively to `127.0.0.1` (never `0.0.0.0` automatically) and validates `Host` and `Origin` headers to prevent DNS rebinding and cross-origin attacks.
2. **Bearer Token Authentication**: HTTP requests require a cryptographically random Bearer token verified via SHA-256 and constant-time comparison (`subtle::ConstantTimeEq`). Tokens are stored in a dedicated local secret file outside Git and SQLite and are never written to logs or `activity_log`.
3. **Granular Server-Side Permissions**: `ReadOnly`, `ReadWrite`, and `Destructive` gates are enforced inside Rust (`crates/journal-mcp`), never solely in UI.
4. **No Arbitrary Execution**: Oxidium exposes strictly typed academic domain tools — zero `execute_sql`, zero shell command execution, zero arbitrary file reads/writes.
5. **Parameterized SQL & Bounded Inputs**: All database queries use bound parameters; MCP payload sizes and batch counts are strictly bounded.
6. **Prompt Injection Hygiene**: User notes, topics, and task descriptions are treated strictly as inert data fields, never as server instructions.
7. **AI Privacy Disclosure**: Any connected AI client can read or modify journal data strictly within the permissions explicitly granted by the user.

## Reporting a Vulnerability
Please open a private security advisory on GitHub (`https://github.com/cryals/Oxidium/security/advisories/new`) or contact the repository maintainer directly. Include reproduction steps, affected version, and impact. Do not post exploit details in public issues before a fix is available.
