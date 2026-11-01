# Jynx CLI

Run `jynx-cli` without arguments to start an interactive conversation, or use `jynx-cli chat [options]` to choose a route explicitly. A prompt after the options runs one task and exits. Without a prompt, the CLI keeps accepting messages until `/exit` or end of input.

## Keyboard controls

Type `/` or press **F2** to open Actions. Use **Up/Down** and **Enter** to select an item, type to filter, and press **Esc** to return. **F3** opens Models and **F4** opens Permissions. Pressing Enter on an empty prompt also opens Actions.

The prompt supports Left/Right, Home/End, Delete, Backspace, pasted text, and Up/Down to recall messages typed during the current session. **Alt+Enter** inserts a new line; Enter sends the message. Ctrl+C clears the prompt and Ctrl+D exits when it is empty. Prompt recall stays in memory and is not loaded from saved conversations.

You can also open menus directly:

```sh
jynx-cli settings
jynx-cli models
```

## Models and loading settings

Models scans `~/.jynx/models` and its subfolders for existing GGUF files. It omits common companion files and extra split parts; choose the first part of a split model. You can also enter a GGUF path outside this folder. The picker rescans whenever it opens, so deleted files disappear.

Choose a model, then review its loading settings: context window, installed backend, GPU layers, flash attention, key/value cache types, CPU threads, batch size, micro batch size, and log detail. The context window must fit both the model and available memory. Higher values can require substantially more memory.

Selecting a quantized KV cache enables flash attention. Floating point caches keep their default type until changed. Zero CPU threads lets llama.cpp choose. A micro batch cannot exceed the batch size. Advanced arguments accept one llama-server argument at a time.

Changes apply when the next message starts. Jynx starts the bundled llama-server for the selected model and restarts it when loading settings change. Source builds can also find downloaded backends in the source tree's `vendor/llama.cpp/binaries` directory. A custom server executable remains available for other installations.

Choose **Cloud / custom endpoint and API key** to configure OpenAI, Anthropic's OpenAI-compatible API, or another compatible endpoint. Supply the provider's model ID. API key input is masked and retained only for this process. For later sessions, set `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `JYNX_MODEL_API_KEY` as appropriate. Keys are not written to the preferences file. Anthropic's compatibility API has its own feature limits; see its [compatibility documentation](https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk).

Menu changes are saved to `~/.jynx/config.toml`, or `JYNX_HOME/config.toml` when that variable is set. On Windows, `~` means the user profile directory; on macOS and Linux, it means the home directory. Model and runtime choices survive restarts. Workspace paths, prompts, and API keys are not saved there.

## Project work

`--cwd` selects the reference folder for file tools and commands. Choose the project itself rather than a drive root. Relative paths start there. The workspace is not a virtual machine; permissions are explained separately in the [permissions guide](permissions.md).

Interactive chat shows concise tool activity by default. Open `/tools` from Actions to inspect the last completed turn's arguments, outputs, and diffs. Enable expanded tool output in Settings to show those details as they arrive. Piped output remains expanded. A model saying it will edit a file is not an edit. Look for the execution result, then verify important changes with tests or a version-control diff.

| Command | Purpose |
| --- | --- |
| `/model` | Pick a local GGUF or configure a cloud endpoint |
| `/permissions` | Choose approval rules |
| `/context` | Change the context window |
| `/help` | Show conversation commands |
| `/status` | Show route, workspace, permission mode, and message count |
| `/settings` | Open model, runtime, permission, and MCP menus |
| `/clear` | Start fresh without changing the model |
| `/compact` | Shorten model context while keeping the conversation usable |
| `/tools` | Inspect the last completed turn's tool results |
| `/logs` | Show output from a runtime launched by this CLI session |
| `/exit` | Leave the conversation |

If a tool asks a question, answer in the next message. The answer remains in the current conversation. Answering a question does not approve an unrelated operation.

## Approvals and output

In `:ask` mode, use the arrow keys to choose Decline or Approve, then press Enter. Decline is selected by default; Esc also declines. With piped input, type `y` to approve; Enter or end of input declines. A pending request does not expire merely because you take time to decide. Stopping the request invalidates its pending approvals.

Text, code blocks, tables, image references, and supported structured graphics are formatted by the terminal renderer. Terminal capabilities vary; plain text is the fallback. Graphics should clarify the answer, not decorate every message.

A slow tool request can reflect model reasoning, network latency, or a process still running. Higher native reasoning effort asks the model to do more work before producing an action; it is not a faster execution mode.

## CLI updates

Interactive startup checks for a stable release and offers Update or Continue before opening chat. Enter keeps the current version. See [releases](../development/releases.md) for installation behavior and offline controls. Run `jynx-cli --version` to inspect the installed version.

Installed setup bundles support `jynx-cli chat --gguf /path/to/model.gguf` without a separate llama-server path. The installed runtime defaults to CPU; `--runtime NAME` selects another bundled backend. Source-only and custom runtime setups can still use `--llama-server`.
