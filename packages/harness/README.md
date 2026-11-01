# Jynx Harness

The shared Rust engine for Jynx. It handles agent execution, model connections, tools, permissions, context, MCP integrations, and persistence. The server exposes its capabilities over a JSON-line protocol; other Rust clients can use the crates directly.

See the root [documentation](../../docs/README.md) for usage, building, development, and releases, and the [protocol reference](../../docs/reference/protocol.md) for host integrations.

Licensed under GPL-3.0-only. Copyright (C) 2026 R3F Studio. See the root LICENSE and COPYRIGHT files.
