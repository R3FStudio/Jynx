# Harness protocol

The server exchanges one JSON object per line over standard input and output. Output is the protocol channel. Clients correlate responses by request ID and consume notifications while a request runs.

```json
{ "id": 1, "method": "server/diagnostics", "params": {} }
```

A response repeats `id` and contains `result` or `error`. Errors have a code and message. Read diagnostics before relying on a feature: protocol version and advertised features are the compatibility contract.

| Methods                                                                                                | Purpose                                                           |
| ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `server/diagnostics`                                                                                   | Identity, protocol version, features                              |
| `agent/run`                                                                                            | Execute a turn with supplied messages, route, workspace, settings |
| `agent/control`                                                                                        | Supported request and compaction controls                         |
| `approval/respond`                                                                                     | Resolve a pending approval ID                                     |
| `context/compact`                                                                                      | Request context compaction                                        |
| `thread/list`, `thread/read`, `thread/resume`                                                          | Conversation access                                               |
| `thread/save`, `thread/name/set`, `thread/archive`, `thread/unarchive`, `thread/fork`, `thread/delete` | Conversation management                                           |
| `thread/turns/list`, `thread/feedback`                                                                 | Turns and explicit answer feedback                                |
| `runtime/state/get`, `runtime/state/update`, `runtime/state/clear`                                     | Host runtime state                                                |
| `mcp/config`, `mcp/config/add`                                                                         | MCP configuration                                                 |

History methods access user data. Fetch only what the host needs; do not print full conversations into diagnostic logs.

## Events

`agent/event` carries text, tool previews, queued/started/progress/completed actions, approvals, context usage, compaction, questions, and accepted final output. A preview is not execution. A completed event records an outcome.

Use stable IDs and chronological ordering. Concurrent calls can finish out of order but still belong to their original IDs. Retry invalidates an unfinished draft, not completed actions. A stopped turn retains completed work without claiming all remaining actions succeeded.

Types are in `packages/harness/protocol/lib.rs`; dispatch is in `packages/harness/server/lib.rs`. The JavaScript subprocess client wraps common methods and multiplexes responses and events.

## Turn completion

A model response ending is separate from the user turn ending. Tool calls run before the next model step. Native assistant `phase` metadata distinguishes `commentary` and `partial_answer` from `final_answer`; preserve it in assistant context when saving and restoring a conversation. User and tool messages do not carry assistant phases.

When tools are enabled, providers without native phases finish through the harness-owned `finish_turn` tool. It accepts a nonempty `answer` and a `status` of `completed` or `blocked`, and must be the only call in its response. It does not execute a command, change files, require tool approval, or add a tool row to the interface. Use `ask_user` for clarification. Tools-disabled chat can use ordinary provider completions.

Unphased text is retained as progress rather than guessed to be a final answer. The harness gives a short protocol reminder when needed. Repeated missing completion signals produce an explicit incomplete-turn error rather than a false success. Output-token limits, cancellation, questions, and tool failures retain their separate handling. A model can still make an incorrect completion claim; explicit signaling prevents premature completion caused by interpreting prose, but does not independently prove task correctness.

## Context budget

The harness checks context pressure before every model request, including continuation and output-limit recovery steps. The budget includes the tool catalog and reserves room for the response. Provider prompt-token usage calibrates subsequent growth estimates; supported local token-count routes can verify pressure directly. Missing local tokenizer routes do not disable automatic compaction during an active turn. Unverified historical estimates alone do not trigger compaction merely because a saved conversation was reopened.

`auto_compact_enabled` controls automatic compaction and `compact_at_ratio` selects the trigger. The default ratio is 0.72. Response headroom may require an earlier trigger. Compaction preserves completed activity in the UI while reducing model context; manual compaction remains available when automatic compaction is off. If an active request still cannot fit after compaction, the harness reports the budget problem before sending another oversized model request.

Context updates include `estimated` to distinguish conservative estimates from provider-calibrated counts, and `context_reset` when a real compaction changes the retained messages. Clients can retain a saved usage count when an uncalibrated estimate is lower. Agent results expose `context_estimated` for the same distinction. Measured usage and compaction may legitimately reduce the displayed count.
