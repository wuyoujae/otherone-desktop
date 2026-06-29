# MCP Import

## Scope

- The desktop app supports importing MCP server configuration from the MCP tab.
- Import accepts either pasted JSON or a remote JSON URL.
- The supported JSON shape is the common client config wrapper:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\path\\to\\folder"]
    }
  }
}
```

- A single server object is also accepted when it includes `name`:

```json
{
  "name": "github",
  "type": "http",
  "url": "https://example.com/mcp"
}
```

## Persistence

- Imported server definitions are stored in SQLite table `mcp_servers`.
- Enabled state is stored in the existing `plugin_installs` table with `kind = 'mcp'`.
- Import enables the server by default.
- Uninstall removes only the enabled state; the imported server config remains available in the MCP market list.

## Validation

- `name` must be 1-64 ASCII characters and may contain letters, numbers, dots, underscores, and hyphens.
- `stdio` servers require `command`; optional `args` must be a string array and optional `env` must be a string map.
- `http` and `sse` servers require an `http://` or `https://` `url`; optional `headers` must be a string map.
- URL import is limited to 1MB UTF-8 JSON. GitHub `blob/...` URLs are converted to raw GitHub URLs.

## Current Limit

- This feature imports and manages MCP server configuration only.
- It does not yet connect servers through `otherone-mcp`, list remote tools, or adapt async MCP calls into the Agent tool loop.
- MCP configs can include secrets in `env` or `headers`; the current app stores them in local SQLite as plain JSON, matching the current local settings storage model.

## References

- Official MCP transport specification: https://modelcontextprotocol.io/specification/2025-06-18/basic/transports
- Claude Code MCP JSON examples and `mcpServers` usage: https://docs.anthropic.com/en/docs/claude-code/mcp
