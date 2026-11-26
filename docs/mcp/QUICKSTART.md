# Oxidium MCP Quickstart

Oxidium provides a local Model Context Protocol (MCP) server over **STDIO** (recommended for local desktop/CLI AI clients) and **Streamable HTTP** (loopback-only `127.0.0.1` with Bearer token authentication).

## 1. STDIO Transport (Recommended)

Run the standalone binary `student-journal-mcp` (or `student-journal --mcp-stdio`):
- Protocol JSON-RPC messages go strictly to `stdout`.
- Diagnostic logs go strictly to `stderr`.
- Uses the same local SQLite database as the desktop GUI (`OXIDIUM_DB_PATH` or standard OS local app data directory).

### Generic MCP JSON Configuration (Claude Desktop, Claude Code, Gemini CLI, Antigravity, Codex)

```json
{
  "mcpServers": {
    "oxidium": {
      "command": "C:/Path/To/student-journal-mcp.exe",
      "args": []
    }
  }
}
```

On Linux:
```json
{
  "mcpServers": {
    "oxidium": {
      "command": "/usr/local/bin/student-journal-mcp",
      "args": []
    }
  }
}
```

Optional custom database path:
```json
{
  "mcpServers": {
    "oxidium": {
      "command": "student-journal-mcp",
      "args": ["--db", "/path/to/oxidium.db"]
    }
  }
}
```

## 2. Streamable HTTP Transport (Loopback Only)

Start either from **Oxidium GUI → Settings → MCP → Start Embedded HTTP Server** or via CLI:

```sh
student-journal-mcp --http --port 38421
```

Endpoint: `http://127.0.0.1:38421/mcp`

Every HTTP request MUST include the Bearer token generated in `mcp-token.secret` (viewable/rotatable in **Settings → MCP**):

```sh
curl -X POST http://127.0.0.1:38421/mcp \
  -H "Authorization: Bearer <YOUR_OXIDIUM_TOKEN>" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"1.0"}}}'
```

## 3. Permissions & Live Sync

- Default mode: **Read & Write** (destructive deletions disabled until explicitly enabled in Settings → MCP).
- Every AI write is recorded in the **MCP Activity Log** (with one-click Undo for supported operations).
- Changes written by an external `student-journal-mcp` STDIO process automatically appear in the open Oxidium GUI within ~0.5 seconds without restarting.
