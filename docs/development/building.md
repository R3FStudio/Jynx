# Building

```sh
npm ci
npm run doctor
npm run build:all
```

`npm ci` installs public JavaScript tools from the lockfile. `doctor` validates available source. `build:all` builds available configured components.

| Command | Result |
| --- | --- |
| `npm run build:harness` | Release harness server |
| `npm run build:cli` | Release Jynx CLI |
| `npm run build:sdk` | SDK validation |
| `npm run build:all` | All available configured components |
| `npm run build:all:exclude-desktop` | Public components only |
| `npm run cli:check` | CLI Rust check without a release binary |
| `npm run harness:check` | Harness workspace check |

Native outputs are in each package's `target/release`. Windows executables have `.exe`; Linux and macOS executables do not. npm aliases still use Rust's compiler for Rust programs.

## Partial checkouts

Missing component folders are skipped with a reason. Each present package runs independently, so an absent or malformed neighbour cannot block its build. If a present component fails, later independent builds still run. The command exits nonzero after all attempts if a real build or metadata failure occurred.

The summary separates **built**, **skipped**, and **failed**. A skip does not mean a component compiled. The CLI is skipped when harness source is absent; independent SDKs can still build. Source dependencies do not require another component's executable to build first.

## Direct Rust commands

```sh
cargo build --release --manifest-path packages/harness/Cargo.toml -p jynx-server --bin jynx-harness
cargo build --release --manifest-path packages/cli/Cargo.toml --bin jynx-cli
```

Use `cargo check` while iterating. Release builds optimize the result and usually take longer. If a binary is locked on Windows, close the process using it before rebuilding.

## Runtime bundles

`npm run llama:sync` downloads supported runtime bundles. Platform, architecture, and runtime selectors can limit the download. Binaries and weights are not source files to commit. A downloaded runtime and an installed model are separate from a successful source build.

For self-contained installers, follow [releases](releases.md). `npm run package:installers` packages the already built CLI/harness and synchronized llama.cpp runtimes for the host; it does not compile missing native binaries.
