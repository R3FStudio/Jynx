# Jynx CLI

The Rust terminal interface for Jynx. Connect a local model or compatible API, work with a project, inspect tool results, and approve actions before they run.

```sh
npm run build:cli
packages/cli/target/release/jynx-cli help
```

This Cargo workspace consumes shared crates from `../harness`. It needs that source to compile, but does not need a separately running server process.

Press F2 for Actions, F3 for Models, or F4 for Permissions. Menus support arrow keys, Enter, Esc, and filtering. Local models are discovered in `~/.jynx/models`; preferences are saved to `~/.jynx/config.toml`. Cloud endpoints remain available alongside local model loading.

See the [CLI guide](../../docs/guides/cli.md), [argument reference](../../docs/reference/cli.md), and [build guide](../../docs/development/building.md).

Attach images with repeatable `--image` flags, for example `jynx-cli chat --image ./screenshot.png "Describe this image"`. A vision-capable local model needs its matching `mmproj` GGUF beside the main model; the CLI discovers matching companions automatically. An explicit `--llama-arg --mmproj --llama-arg /path/to/projector.gguf` takes precedence. Cloud models receive image inputs through the same shared harness.
