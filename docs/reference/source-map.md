# Source map

Paths below are relative to `packages/`.

| Source | Responsibility |
| --- | --- |
| `harness/protocol/lib.rs` | Shared requests, responses, settings, events |
| `harness/agent/lib.rs` | Execution, batches, approvals, continuation, final acceptance |
| `harness/agent/control.rs` | Pause/resume and compaction coordination |
| `harness/agent/presentation.rs` | Visible phases and text pacing |
| `harness/agent/tool_output.rs` | Bounded model-facing results |
| `harness/model/lib.rs` | Requests, streaming, tool parsing, retries |
| `harness/model/reasoning.rs` | Native reasoning values |
| `harness/model/private_text.rs` | Separation of reasoning from visible output |
| `harness/model/token_count.rs` | Supported counting routes |
| `harness/tools/lib.rs` | Built-in schemas and execution |
| `harness/sandbox/lib.rs` | Path normalization and approval policy |
| `harness/security/lib.rs` | Untrusted-content checks |
| `harness/context` | Budgets and compaction |
| `harness/mcp/lib.rs` | Configuration and MCP stdio client |
| `harness/storage/lib.rs` | Conversation/runtime persistence |
| `harness/server/lib.rs` | Dispatch and agent hosting |
| `cli/app.rs` | Arguments, menus, approvals, runtime startup, event printing |
| `cli/terminal_graphics.rs` | Rich terminal formatting |

These boundaries identify ownership rather than a fixed call graph.

`build-all.ts` and `workspace-components.mjs` discover available packages. `process-runner.ts` launches tools without shell-string composition. Runtime targets, downloads, and extraction are in `llama-runtime-targets.ts`, `sync-llama.ts`, and `extract-runtime-tar.ts`. `pack-*` scripts create artifacts; `repositories.mjs` coordinates Git checkouts.

SDK implementations, declarations, and tests live within their packages. Change the public API and its examples together.
