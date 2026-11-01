# Jynx CLI

Run `jynx-cli` without arguments to start an interactive conversation, or use `jynx-cli chat [options]` to choose a route explicitly. A prompt after the options runs one task and exits. Without a prompt, the CLI keeps accepting messages until `/exit` or end of input.

## Project work

`--cwd` selects the reference folder for file tools and commands. Choose the project itself rather than a drive root. Relative paths start there. The workspace is not a virtual machine; permissions are explained separately in the [permissions guide](permissions.md).

The CLI prints tool arguments and results in expanded form. A model saying it will edit a file is not an edit. Look for the execution result, then verify important changes with tests or a version-control diff.

| Command | Purpose |
| --- | --- |
| `/help` | Show conversation commands |
| `/status` | Show route, workspace, permission mode, and message count |
| `/settings` | Open model, runtime, permission, and MCP menus |
| `/clear` | Start fresh without changing the model |
| `/compact` | Shorten model context while keeping the conversation usable |
| `/logs` | Show output from a runtime launched by this CLI session |
| `/exit` | Leave the conversation |

If a tool asks a question, answer in the next message. The answer remains in the current conversation. Answering a question does not approve an unrelated operation.

## Approvals and output

In `:ask` mode, Jynx waits for `y` or a decline. Enter or end of input declines. A pending request does not expire merely because you take time to decide. Stopping the request invalidates its pending approvals.

Text, code blocks, tables, image references, and supported structured graphics are formatted by the terminal renderer. Terminal capabilities vary; plain text is the fallback. Graphics should clarify the answer, not decorate every message.

A slow tool request can reflect model reasoning, network latency, or a process still running. Higher native reasoning effort asks the model to do more work before producing an action; it is not a faster execution mode.

## CLI updates

Interactive startup checks for a stable release and offers Update or Continue before opening chat. Enter keeps the current version. See [releases](../development/releases.md) for installation behavior and offline controls. Run `jynx-cli --version` to inspect the installed version.

Installed setup bundles support `jynx-cli chat --gguf /path/to/model.gguf` without a separate llama-server path. The installed runtime defaults to CPU; `--runtime NAME` selects another bundled backend. Source-only and custom runtime setups can still use `--llama-server`.
