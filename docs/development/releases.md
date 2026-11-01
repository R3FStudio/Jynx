# Releases

A release ties a source commit, its version, and its downloadable artifacts together. Start with a clean, committed working tree and use the same source revision for every platform.

## Prepare

```sh
npm run version:check
npm run test:scripts
npm run build:all:exclude-desktop
npm run sdk:test
```

Change versions with [the version command](versioning.md), then rebuild. Verify `jynx-cli --version` against the intended release version.

## Build downloadable packages locally

| Command | Output |
| --- | --- |
| `npm run pack:harness` | Harness native archive, SHA-256 file, platform manifest |
| `npm run pack:cli` | CLI native archive, SHA-256 file, platform manifest |
| `npm run pack:sdk-js` | JavaScript package tarball |
| `npm run pack:sdk-python` | Python wheel |
| `npm run pack:llama` | Prepared runtime archive |
| `npm run pack:bundle` | Portable CLI/harness/runtime bundle with source and notices |
| `npm run package:installers` | Host-native setup plus portable bundle |

All public release files are written directly into the repository's root `dist/` directory. Native archives contain the executable, `LICENSE`, and `COPYRIGHT`. They are built for the host OS and architecture; copying a Windows archive to Linux does not make it a Linux binary. Runtime bundles must be downloaded separately before packing. Model weights are not bundled.

Extract a native archive into a temporary folder and run its help/version command. Test the CLI with a controlled endpoint and a disposable project. Inspect SDK archives for expected code, types, and license notices. Do not upload caches, credentials, local state, or unrelated artifacts.

## GitHub release workflow

The workflow in `.github/workflows/release.yml` is the source of truth for runner targets and upload paths. It builds the public components, tests them, and creates native CLI and harness archives for its platform matrix, plus SDK packages. Main-branch pushes and manual workflow runs produce downloadable Actions artifacts without publishing a release. Each native job uses `build:all:exclude-desktop`, downloads installer runtimes with `llama:sync -- --portable`, builds setup packages, and runs installation smoke checks.

For a public release:

1. Commit the version and source changes, then push the public branch.
2. Create a tag named `v` followed by the root package version on that commit. For example, a version of `0.0.2` uses tag `v0.0.2`.
3. Push that tag. The workflow checks the version and builds the artifacts. The publish job rejects a tag that does not match the manifest.
4. After all builds pass, the publish job creates a draft, uploads artifacts, and publishes it. The release includes a link to corresponding source and build instructions; GitHub also exposes the tagged source archives.
5. Check the published downloads and platform manifests. Test installation and the CLI update flow before announcing the release.

The job uses the repository's GitHub Actions token with release-write permission only in the publishing job. Do not commit publishing tokens. If repository policy requires signing or extra approvals, add them before enabling publication. The workflow does not configure signing certificates.

GPL-covered binaries must be distributed with their license notices and corresponding source under the applicable terms. Keep the exact tagged source available. [LICENSE](../../LICENSE) contains the license; [COPYRIGHT](../../COPYRIGHT) identifies R3F Studio and separately licensed material.

## CLI updates

On an interactive chat launch, Jynx CLI checks the latest stable public GitHub release with a short timeout. When a newer matching CLI archive and checksum are available, it shows the installed and available versions and offers **Update** or **Continue**. Enter continues with the installed version.

An update downloads only the matching CLI archive, verifies its SHA-256 checksum, and stages the executable beside the installed one. Windows finishes replacement after the process exits; Linux and macOS replace it atomically. Start the CLI again after installation. Failure before replacement keeps the installed version available. A directory without write permission requires your normal installation method.

Checks do not run for piped input, `serve`, help, or version commands. Set `JYNX_NO_UPDATE_CHECK=1` to disable them completely. Offline or unavailable release checks continue startup. The check contacts GitHub; it does not send prompts or project contents. It does not update model weights or inference runtimes.

## Runtime support

The runtime target definitions and sync/extraction scripts determine which bundles are supported. When upstream layouts change, update extraction fixtures and test the affected platforms. A successful host build is not evidence that another platform's runtime works.

## Complete setup packages

Run `npm run build:all:exclude-desktop`, `npm run llama:sync -- --portable`, then `npm run package:installers` on the target host. The packager uses the synced runtime manifest rather than downloading a different source of runtime binaries. It includes the full selected runtime directories so shared libraries stay with llama-server. Existing model weights and local state are not bundled.

Output is directly in the root `dist/`: Windows `.exe`, Linux `.run`, or macOS `.pkg`, plus portable archives and SHA-256 files. Windows packaging provisions a pinned, checksum-verified portable NSIS compiler when `MAKENSIS` or a standard compiler installation is unavailable. macOS packaging uses the system `pkgbuild`; `JYNX_MAC_INSTALLER_IDENTITY` can select an installed signing identity. Notarization and signing credentials are not automatically provisioned.

The native setup contains a snapshot of the current public source files. Private checkouts and ignored build/state directories are excluded. Commit or review source changes before release so that the snapshot corresponds to the binaries. Do not archive an unrelated dirty checkout as a release.

Run `node scripts/verify-installer.mjs` after packaging. Windows tests use an isolated setup test mode without registering the application or modifying user PATH. Unix checks use a temporary installation prefix. macOS additionally validates the generated package structure; the portable installation path is exercised without modifying the runner's system installation.

Keep the workflow's target matrix and runtime target definitions as the source of truth for supported packaging combinations. A bundle produced on one operating system is not a cross-compiled binary for another one.

Root `npm run installer` and `npm run dist` build the public setup package; `npm run pack` builds the portable bundle. `npm run pack:all` packages public deliverables.

## Packaging selections

Root aggregate packaging commands accept `--exclude-desktop` explicitly. For example, `npm run pack:all -- --exclude-desktop` and `npm run package:installers -- --exclude-desktop` select public deliverables only. The named aliases `pack:all:exclude-desktop`, `package:installers:exclude-desktop`, and `pack:bundle:exclude-desktop` provide the same selection.

Use `--dry-run` to print the selected packaging steps without building or downloading anything. Contradictory inclusion and exclusion options fail before any packaging starts. Public-only packaging remains the default.

## One release folder

All public installers, portable bundles, CLI updater archives, harness archives, SDK packages, checksums, and manifests are written directly into `dist/`. Packaging does not split them across package-specific folders. Intermediate compiler output still lives in its normal build directories.

Download the Actions artifact named `jynx-release-files` for the complete cross-platform release. Its ZIP contains the files directly, without `dist/`, `public/`, or `packages/` wrappers. Extract it into the root `dist/` folder and upload those files as GitHub Release assets. Keep CLI archive filenames and their matching `.sha256` files together so automatic updates work. Platform-specific artifacts are also available with the same flat layout.

## Choosing release files

| Filename starts with | Contents and purpose |
| --- | --- |
| `Jynx-Complete-Setup-` | Install Jynx CLI, harness, and bundled llama.cpp runtimes. Choose your operating system and architecture. |
| `Jynx-Complete-Portable-` | The complete bundle for manual installation without the setup wizard. |
| `Jynx-CLI-Only-` | Just the CLI executable. The CLI automatic updater downloads this archive and its matching checksum. |
| `Jynx-Harness-Server-` | Just the harness server executable for separate deployments. |
| `Jynx-LlamaCpp-Runtime-` | A llama.cpp runtime and its libraries for the named backend. |
| `Jynx-JavaScript-SDK-` | The JavaScript client package for developers. |
| `jynx_python_sdk-` | The Python client wheel for developers; its name follows Python wheel packaging rules. |

`.sha256` files verify the archive or installer with the same name. `.manifest.json` files describe a standalone executable archive; they are not installers. `x64` means Intel/AMD 64-bit; `arm64` means ARM 64-bit.

The CLI updater accepts only `Jynx-CLI-Only-vVERSION-OS-ARCH.tar.gz` and its matching `.sha256` asset. Older archive names are not supported. Upload the complete new file set when publishing a release.

Install the new CLI manually if your current CLI predates this naming scheme. Republishing a version uploads its new file set and removes superseded asset names from that release.

For npm and PyPI uploads, follow [publishing SDK packages](publishing-sdks.md). Uploading SDK archives to a GitHub Release does not publish them to either registry.
