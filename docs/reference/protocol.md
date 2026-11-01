# Harness protocol

The server exchanges one JSON object per line over standard input and output. Output is the protocol channel. Clients correlate responses by request ID and consume notifications while a request runs.

```json
{"id":1,"method":"server/diagnostics","params":{}}
```

A response repeats `id` and contains `result` or `error`. Errors have a code and message. Read diagnostics before relying on a feature: protocol version and advertised features are the compatibility contract.

| Methods | Purpose |
| --- | --- |
| `server/diagnostics` | Identity, protocol version, features |
| `agent/run` | Execute a turn with supplied messages, route, workspace, settings |
| `agent/control` | Supported request and compaction controls |
| `approval/respond` | Resolve a pending approval ID |
| `context/compact` | Request context compaction |
| `thread/list`, `thread/read`, `thread/resume` | Conversation access |
| `thread/save`, `thread/name/set`, `thread/archive`, `thread/unarchive`, `thread/fork`, `thread/delete` | Conversation management |
| `thread/turns/list`, `thread/feedback` | Turns and explicit answer feedback |
| `runtime/state/get`, `runtime/state/update`, `runtime/state/clear` | Host runtime state |
| `mcp/config`, `mcp/config/add` | MCP configuration |

History methods access user data. Fetch only what the host needs; do not print full conversations into diagnostic logs.

## Events

`agent/event` carries text, tool previews, queued/started/progress/completed actions, approvals, context usage, compaction, questions, and accepted final output. A preview is not execution. A completed event records an outcome.

Use stable IDs and chronological ordering. Concurrent calls can finish out of order but still belong to their original IDs. Retry invalidates an unfinished draft, not completed actions. A stopped turn retains completed work without claiming all remaining actions succeeded.

Types are in `packages/harness/protocol/lib.rs`; dispatch is in `packages/harness/server/lib.rs`. The JavaScript subprocess client wraps common methods and multiplexes responses and events.
