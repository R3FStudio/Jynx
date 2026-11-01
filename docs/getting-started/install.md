# Install Jynx

Use an installer from the repository's GitHub Releases page, or download the `jynx-release-files` Actions artifact from a main-branch/manual build. Extract that ZIP to see all release files in one folder. Choose the package matching your operating system and CPU architecture. `VERSION` and `ARCH` below are placeholders, not literal filenames.

The setup bundle contains Jynx CLI, Jynx Harness, and llama.cpp runtimes with their required libraries. It includes CPU support and the platform's default accelerated backend when available. You do not need Node.js or Rust to use these packages. Model weights are separate.

## Windows

Open `Jynx-Complete-Setup-vVERSION-windows-ARCH.exe`. Setup installs for your Windows user, adds the command directory to your user PATH, and creates a Start-menu shortcut. Open a new terminal after installation.

Use Installed Apps or the included uninstaller to remove Jynx. Existing projects, models, and conversation data outside the application files are retained.

## Linux

Run the self-contained installer:

```sh
sh Jynx-Complete-Setup-vVERSION-linux-ARCH.run
```

The default location is `~/.local/share/jynx`, with command links in `~/.local/bin`. Make sure the command directory is on your PATH. No administrator account is needed for this default.

Choose other absolute locations when required:

```sh
sh Jynx-Complete-Setup-vVERSION-linux-ARCH.run --prefix /absolute/install/path --bin-dir /absolute/command/path
```

To remove a default installation, run `sh ~/.local/share/jynx/uninstall.sh`. The uninstaller removes owned files and matching command links; it preserves unrelated files.

## macOS

Open `Jynx-Complete-Setup-vVERSION-macos-ARCH.pkg` in Installer. The package installs under `/usr/local/lib/jynx` and provides commands in `/usr/local/bin`. It may ask for administrator approval.

For a user-only installation, extract the portable `.tar.gz` bundle and run its `install.sh` as described for Linux. Run the installed `uninstall.sh` to remove its owned files. Removing files from a `.pkg` install does not remove the system's package receipt automatically.

Signing and notarization depend on the release environment. Building a package does not by itself give it an Apple Developer signature or notarization approval. Use your organization's signing process for a signed public release.

## First commands

```sh
jynx-cli --version
jynx-harness --version
jynx-cli help
```

Connect to an existing endpoint using the [model guide](../guides/models.md), or start a bundled runtime with a model you downloaded:

```sh
jynx-cli chat --gguf "/path/to/model.gguf"
```

The CLI discovers its installed runtime manifest and uses CPU by default. Add `--runtime NAME` to select another bundled backend. Available names and upstream versions are recorded in `runtime-manifest.json` in the installation directory. GPU backends still require compatible hardware and drivers.

`jynx-llama` starts the bundled CPU llama-server directly when a separate local API is useful. Pass the model and server flags supported by that upstream runtime.

`jynx-harness` is a JSON-line server for host programs. Setup does not start it as a background system service. Use Jynx CLI for interactive conversations.

## Verify downloads and update

Each setup file and portable bundle has a companion `.sha256` file. Compare its digest with your downloaded file before installation. The source snapshot and license notices are included in the bundle.

The CLI's startup updater updates the CLI executable. To update the harness or bundled runtime libraries as well, install a newer complete setup package. See [releases](../development/releases.md) for packaging and update behavior.

Linux compatibility depends on the upstream llama.cpp binary requirements and installed system libraries. Start with CPU when GPU drivers are unavailable. Packages do not include a GPU driver or a model.
