# Requirements

| Tool | Purpose | When needed |
| --- | --- | --- |
| Git | Downloads source and records changes | Source development |
| Node.js 24 or later | Runs repository scripts and JavaScript clients | npm commands |
| npm | Installs JavaScript tools and runs package commands | Included with Node.js |
| Rust through rustup | Compiles the harness and CLI | Native builds |
| Python 3 | Runs the Python SDK and its tests | Python development |
| Compatible model server | Generates responses | Conversations |

The Rust workspaces select their compiler channel through `rust-toolchain.toml`. Install rustup rather than copying a compiler executable.

On Windows, Rust needs the MSVC C++ build tools and Windows SDK. Select **Desktop development with C++** in Visual Studio Installer. On Linux, install your distribution's C/C++ compiler and linker packages. On macOS, install the Xcode command-line tools.

## Model resources

Model weights dominate memory use. RAM and GPU requirements depend on the model, quantization, context size, and offload settings. A successful program build does not mean a particular model fits in memory. Confirm that the chosen runtime can answer a small request before adding long context and tools.

## Check the tools

```sh
node --version
npm --version
rustup show
cargo --version
python --version
```

The Python package launcher also checks common alternatives to `python`. Python is not required to build only the CLI. See the [model guide](../guides/models.md) for connection and runtime choices.
