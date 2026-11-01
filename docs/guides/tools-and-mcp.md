# Tools and MCP

The catalogue can include listing and searching files, reads, creation and edits, commands, web searches, page reads, image viewing, and questions. Availability depends on settings and supported host capabilities. An adapter is advertised only when its host can handle it.

Each call has arguments, a running state, and an outcome. The model sees a bounded result suitable for its context; the host can retain a fuller audit. Large text is shortened with an omission notice instead of silently becoming blank.

Independent searches can be requested together. Up to four consecutive page reads can run concurrently. Independent writes to different files can also overlap in bounded groups. Repeated writes to one path stay ordered, and commands form execution boundaries. Results return to the model in request order even when execution finishes in another order.

New files are created; existing files are edited. Use the final result and a file diff to verify what happened.

## MCP configuration

MCP adds tools through external programs. Configure stdio servers in `/settings` or in `~/.jynx/mcp.json`:

```json
{
  "mcpServers": {
    "example": {
      "command": "/path/to/server",
      "args": ["--stdio"],
      "env": {},
      "enabled": true
    }
  }
}
```

This is a format example, not a bundled integration. Install the chosen program separately and ensure its executable resolves in Jynx's environment. Names use letters, numbers, hyphens, and underscores.

Disabled servers are not offered to the model. Environment values can contain credentials, so keep the configuration outside source control. Each server has its own resource use and network behavior; installing it is not blanket approval of its actions.

The shared MCP client handles initialization, discovery, request IDs, and process I/O. Hosts can reuse this implementation instead of creating a separate integration protocol.
