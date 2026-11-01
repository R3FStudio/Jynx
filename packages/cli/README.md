# Jynx CLI

The Rust terminal interface for Jynx. Connect a local model or compatible API, work with a project, inspect expanded tool results, and approve actions before they run.

```sh
npm run build:cli
packages/cli/target/release/jynx-cli help
```

This Cargo workspace consumes shared crates from `../harness`. It needs that source to compile, but does not need a separately running server process.

See the [CLI guide](../../docs/guides/cli.md), [argument reference](../../docs/reference/cli.md), and [build guide](../../docs/development/building.md).
