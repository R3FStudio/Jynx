# Configuration

The CLI takes route defaults from environment variables and explicit values from flags. `/settings` changes the current interactive session. Use `/status` to confirm the connection, workspace, and permission mode.

```sh
jynx-cli chat --endpoint http://127.0.0.1:8080 --model your-model --cwd /path/to/project --permission :ask --reasoning default
```

The endpoint selects the server. The model ID selects a model at that server. The workspace selects the reference folder for tools. A model file path is not an API model ID.

## Environment

`JYNX_MODEL_ENDPOINT`, `JYNX_MODEL_NAME`, and `JYNX_MODEL_API_KEY` supply route defaults. `JYNX_STATE_DIR` selects the state location; `JYNX_MCP_CONFIG` selects an integration configuration. See the [environment reference](../reference/environment.md).

For a temporary PowerShell session:

```powershell
$env:JYNX_MODEL_ENDPOINT = 'http://127.0.0.1:8080'
$env:JYNX_MODEL_NAME = 'your-model'
$env:JYNX_STATE_DIR = 'D:\JynxData'
```

These values affect that shell and its child processes. They do not edit repository configuration.

## Files

`packages/harness/config/default.toml` is a settings example. Do not assume editing it changes an existing CLI session: use supported flags, environment variables, and menus.

MCP configuration is JSON. Conversation and runtime state are maintained by the storage crate. A compatible host can write the optional HTTP discovery file, `~/.jynx/server.json`; it can include an API key. The SDK validates its loopback endpoint before using it.

Keep keys, `.env` files, state, dependencies, and model weights out of commits. Ignore rules prevent common accidental additions but do not erase secrets already present in Git history.
