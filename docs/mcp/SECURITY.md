# Oxidium MCP Security Model

1. **Server-Side Permission Enforcement**: Permissions are evaluated inside Rust (`crates/journal-mcp/src/permissions.rs`) on every tool call using live SQLite settings:
   - `ReadOnly`: blocks all write and destructive tools.
   - `ReadWrite`: permits read, create, update, archive, and batch creation tools; blocks `delete_*` tools unless `mcp_allow_destructive = true`.
   - `Destructive`: separate opt-in toggle required for `delete_grade`, `delete_attendance`, `delete_task`, and `delete_checklist_item`.
2. **Loopback-Only HTTP Binding**: Streamable HTTP binds strictly to `127.0.0.1:<port>` with `Host` allowlist (`localhost`, `127.0.0.1`, `::1`) and 2 MiB max request body limit. Oxidium never opens firewall ports or binds public interfaces automatically.
3. **Bearer Authentication**: Streamable HTTP validates `Authorization: Bearer <token>` via SHA-256 and constant-time comparison (`subtle::ConstantTimeEq`). Missing or invalid tokens return HTTP `401 Unauthorized`. Tokens reside exclusively in `mcp-token.secret` (mode `0600` on Unix) and are redacted from logs and `activity_log`.
4. **No Arbitrary Execution Surface**: There is no `execute_sql`, `run_shell`, or arbitrary file read/write tool. Every tool maps to a validated typed application service.
5. **Profile Isolation**: All mutating tools require explicit `profile_id` and reject cross-profile subject/board/grade links.
6. **Idempotency**: Write tools accept `idempotency_key` stored in `mcp_idempotency_keys` to prevent duplicate entries after retries.
7. **Prompt Injection Defense**: All student comments, topics, notes, and task descriptions are serialized purely as inert JSON data values and never interpreted as server commands.
8. **AI Data Exposure Notice**: Any AI client connected by the user receives only the data returned by invoked tools/resources within the user's configured permission level.
