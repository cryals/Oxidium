# Oxidium MCP Integration Architecture

Oxidium ships with a production Model Context Protocol (MCP) server built on the official Rust SDK (`rmcp` 3.11), enabling AI assistants to inspect and manage the student journal safely via natural language.

See `docs/mcp/` for complete client-facing documentation:
- [QUICKSTART.md](mcp/QUICKSTART.md)
- [TOOLS.md](mcp/TOOLS.md)
- [RESOURCES.md](mcp/RESOURCES.md)
- [SECURITY.md](mcp/SECURITY.md)
- [EXAMPLES.md](mcp/EXAMPLES.md)
- [CLIENT-COMPATIBILITY.md](mcp/CLIENT-COMPATIBILITY.md)

## Transports
1. **STDIO (`student-journal-mcp` or `student-journal --mcp-stdio`)**:
   - Standard JSON-RPC over `stdin`/`stdout`.
   - Diagnostics logged strictly to `stderr`; `stdout` is never polluted.
2. **Streamable HTTP (`student-journal-mcp --http` or embedded in GUI Settings → MCP)**:
   - Loopback-only (`127.0.0.1:<port>/mcp`), never auto-exposed on public interfaces.
   - Enforces `Authorization: Bearer <token>` (constant-time SHA-256 check) and loopback `Host`/`Origin` validation.
   - Exposes live status, request/error counters, and last-request timestamps in the GUI.

## Capabilities
- **Tools**: 80 typed tools across Profiles, Subjects, Grades, Attendance, Journal, Kanban Boards/Columns, Tasks, Tags/Badges/Checklists, and Statistics.
- **Resources**: Read-only structured URIs (`oxidium://profiles`, subjects, mark types, academic periods, active tasks, schema reference).
- **Prompts**: 7 ready-to-use academic prompts (daily/weekly/monthly summaries, upcoming assignments, grade improvement plan, workload planning, importing academic notes).
- **Safety**: Server-side permission enforcement (`ReadOnly`, `ReadWrite`, `Destructive`), explicit `profile_id` on writes, idempotency keys, payload/batch limits, full `activity_log` auditing with Undo support, and zero arbitrary SQL/shell/file tools.
