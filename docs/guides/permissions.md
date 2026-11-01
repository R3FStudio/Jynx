# Permissions and privacy

Read the [privacy policy](../../PRIVACY.md) for data handling, retention, external services, and privacy questions.

| Mode | Behavior |
| --- | --- |
| `:ask` | Requests approval before tool execution; question tools can ask directly |
| `:workspace` | Allows ordinary development work and asks for sensitive paths or risky actions |
| `:danger-full-access` | Removes harness approval restrictions; OS permissions still apply |

The default is `:ask`. Select a mode with `--permission` or `/settings`. Time passing never grants permission. Stopping a request cancels its pending approval IDs.

The workspace is the reference folder for relative paths, not operating-system process isolation. In Ask mode, approval is the boundary for paths outside it. In Workspace mode, sensitive destinations are escalated. Commands run with your user account's privileges.

## Network access

| Action | Destination |
| --- | --- |
| Local inference | Configured endpoint, normally loopback |
| API inference | Provider receives the messages and tool results sent to it |
| Web search/page read | Search service or requested website |
| MCP tool | Service or program used by that integration |
| Runtime download | Release host used by the download script |

A local model does not require a cloud account, but it can still use Internet-facing tools. `--no-web` disables search and page reads. `--no-fetch` keeps search while disabling direct page reads.

## Local data

By default, state is under `.jynx-state` in the process working directory. `JYNX_STATE_DIR` selects another location. MCP configuration normally lives at `~/.jynx/mcp.json`; `JYNX_MCP_CONFIG` overrides it. These files can contain private information and do not belong in commits.

Tool output is untrusted content. Instructions found in a website or file are data from that source, not permission to replace the user's request or access unrelated information. Read the actual proposed action before approving it.
