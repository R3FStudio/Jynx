# Repository guide

Shared behavior belongs in harness crates. Terminal presentation and client APIs build on those interfaces.

| Path | Responsibility |
| --- | --- |
| `packages/harness` | Agent execution, models, tools, context, approvals, MCP, storage, and server |
| `packages/cli` | Commands, interactive conversation, owned runtime processes, terminal formatting |
| `packages/sdk-js` | HTTP client, discovery, and Node harness-subprocess client |
| `packages/sdk-python` | Python HTTP client and discovery |
| `api` | Compatible local-model HTTP API definition |
| `scripts` | Builds, runtime downloads, archives, and repository commands |
| `tools/tooling` | Lint and formatting configuration |
| `assets` | Branding and CLI presentation assets |
| `examples` | Small integration examples |
| `integrations` | Editor and host integration material |
| `docs` | User, contributor, and reference documentation |

## Dependencies

The CLI has a separate Cargo workspace and uses shared Rust crates through path dependencies. It needs harness source, not a running server or a previously built server executable. HTTP SDK clients do not compile Rust.

`workspace.config.json` records component locations, names, build order, and source dependencies. `package.json` records npm commands and public npm workspaces. Available standalone components can use the same build orchestrator without becoming npm workspaces.

## Request flow

The CLI gathers the route and user input, then calls the agent. The model adapter translates messages and tools into the provider's format. The agent checks permission for an action, executes it, and returns its result to the model. A final answer ends the turn. The server exposes this behavior to hosts over JSON lines.

Change the owning layer: stream parsing in the model crate, path policy in the sandbox, terminal formatting in the CLI. Keep presentation details out of shared execution code. The [source map](../reference/source-map.md) identifies the relevant files.
