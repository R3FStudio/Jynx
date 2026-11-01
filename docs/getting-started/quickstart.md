# Quick start

## Get the source

```sh
git clone https://github.com/R3FStudio/Jynx.git
cd Jynx
npm ci
```

`npm ci` installs the JavaScript dependencies recorded in the lockfile. It does not download model weights.

## Build Jynx CLI

```sh
npm run build:cli
packages/cli/target/release/jynx-cli help
```

The result is `packages/cli/target/release/jynx-cli`. Windows adds `.exe`:

```powershell
.\packages\cli\target\release\jynx-cli.exe help
```

## Connect a model

Start an OpenAI-compatible model server, then supply its endpoint, model ID, and project folder:

```sh
packages/cli/target/release/jynx-cli chat --endpoint http://127.0.0.1:8080 --model your-model --cwd /path/to/project
```

The default permission mode is **Ask for approval**. Read a request before approving it. Tools run under your operating-system account.

For a compatible API provider, use its base URL and `--cloud`. Keep the key in the environment rather than a command saved in shell history:

```powershell
$env:JYNX_MODEL_API_KEY = 'your-key'
.\packages\cli\target\release\jynx-cli.exe chat --cloud --endpoint https://provider.example/v1 --model your-model
```

That provider URL is a placeholder, not a Jynx service.

## Try a small task

Ask Jynx to list the project files and explain the README. Check the tool results, then try a small edit in a disposable project. Use `/help` for conversation commands and `/settings` for menus.

To build every available component, run `npm run build:all`. Missing component folders are skipped. The [build guide](../development/building.md) explains full and partial builds.
