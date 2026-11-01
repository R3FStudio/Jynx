# Environment reference

| Variable | Purpose |
| --- | --- |
| `JYNX_MODEL_ENDPOINT` | Default compatible model endpoint |
| `JYNX_MODEL_NAME` | Default model ID |
| `JYNX_MODEL_API_KEY` | Optional provider credential |
| `JYNX_HOME` | CLI settings and model-library directory (default: `~/.jynx`) |
| `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` | Provider-specific credential for a matching saved cloud endpoint |
| `JYNX_STATE_DIR` | Harness conversation/runtime-state directory |
| `JYNX_MCP_CONFIG` | MCP configuration path |
| `JYNX_SKILLS_DIR` | Skill directory supplied by a supporting host |
| `JYNX_NODE_EXECUTABLE` | Node executable supplied to applicable tools |
| `JYNX_LLAMA_MODEL` | Runtime start script's model file when `--model` is absent |
| `JYNX_LLAMA_RUNTIME` | Runtime choice for the start script |
| `JYNX_PRIVATE_SOURCE_TOKEN` | Optional credential for linked-repository verification |
| `GH_TOKEN` / `GITHUB_TOKEN` | GitHub authentication where repository tools support it |

Provider keys and repository tokens are separate credentials. Giving a model provider a repository token does not authenticate Git and can disclose the token to that provider.

Flags also configure connection, context, reasoning, permissions, and runtime startup. See [CLI arguments](cli.md). These defaults do not change the inference server's own launch configuration.

For diagnostics, use fresh state and MCP paths. Do not record keys in shell examples, source, issues, or screenshots.

`JYNX_NO_UPDATE_CHECK=1` disables CLI startup checks against GitHub releases. Piped jobs and machine-protocol commands never run these checks.
