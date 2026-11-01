# CLI reference

The installed executable is the source of truth for its command-line options:

```sh
jynx-cli help
jynx-cli --version
```

Use `jynx-cli` for an interactive session, or `jynx-cli chat` with connection options and a prompt for a task. The [CLI guide](../guides/cli.md) covers everyday use and session commands. The [configuration guide](../guides/configuration.md) explains settings and environment variables.

`serve` exposes the harness JSON-line protocol for host applications. Keep its standard output available for protocol messages. See the [protocol reference](protocol.md).

Thread and state commands are listed in help. They operate on the configured state directory; use disposable state for development and integration tests. Exported conversation data can contain project content and should not be committed.

Interactive update checks and installation behavior are described in [releases](../development/releases.md).
