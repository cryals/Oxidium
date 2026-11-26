# MCP Client Compatibility Matrix

Compatibility depends on each client's supported MCP transport, protocol revision, and authentication options. Entries below distinguish **locally verified protocol compliance** from **external client configurations**.

| Client | Recommended Transport | Auth Method | Status | Notes |
| --- | --- | --- | --- | --- |
| Automated JSON-RPC STDIO Harness (`student-journal-mcp`) | STDIO (`stdin`/`stdout`) | Local OS process isolation | **Verified** | Verified in automated integration tests (`initialize`, `tools/list`, `tools/call`, `resources/*`, `prompts/*`). |
| Automated Streamable HTTP Harness (`127.0.0.1:<port>/mcp`) | Streamable HTTP (`rmcp 3.11`) | `Authorization: Bearer <token>` | **Verified** | Verified with 401 unauthorized rejection, session header, JSON responses, and all 9 user scenarios. |
| Claude Desktop | STDIO (`mcpServers`) | Local process | Standard MCP STDIO config (not interactively verified with closed-source Claude Desktop binary on this build host) | Configure `student-journal-mcp` in `claude_desktop_config.json`. |
| Claude Code | STDIO or Streamable HTTP | Local process / Bearer header | Standard MCP config (unverified with external CLI binary here) | `claude mcp add oxidium -- student-journal-mcp`. |
| Gemini CLI | STDIO (`mcpServers`) | Local process | Standard MCP config (unverified with external CLI binary here) | Add `student-journal-mcp` under `mcpServers` in `~/.gemini/settings.json`. |
| Google Antigravity | STDIO or Streamable HTTP | Local process / Bearer header | Standard MCP config | Register `student-journal-mcp` in Antigravity MCP settings. |
| Codex CLI | STDIO | Local process | Standard MCP config (unverified with external CLI binary here) | Register `student-journal-mcp` in Codex MCP server config. |
| ChatGPT MCP Connectors | Streamable HTTP (HTTPS remote connectors typically require public TLS tunnel; Oxidium intentionally binds loopback-only by default) | Bearer token | **Local-only by design** | Oxidium never exposes `127.0.0.1` publicly or alters firewall rules; remote cloud connectors require user-managed secure tunneling if desired. |
