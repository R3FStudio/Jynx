# Developing Jynx in VS Code

Open the repository root and accept the recommended extensions. Install Node.js
22.12 or newer, the stable Rust toolchain, and your platform's native build tools.
Install npm dependencies before using JavaScript tasks.

- **Ctrl/Cmd+Shift+B** builds the harness with debug symbols.
- **Tasks: Run Test Task** runs the harness workspace tests by default.
- **Tasks: Run Task** offers Cargo checks, Clippy, formatting checks, CLI builds,
  release builds, SDK tests, JavaScript linting, and repository diagnostics.
- **Run and Debug** offers the harness stdio server, the interactive CLI, and the
  Node build orchestrator. CodeLLDB builds the selected Rust binary before launch.

The harness speaks newline-delimited JSON on stdin/stdout. Use the integrated
terminal for protocol input; for interactive conversations, select the CLI launcher.
Rust debug sessions set `RUST_BACKTRACE=1` and keep state under `.cache/debug/`,
which Git ignores. These sessions can still execute tools and access providers;
use fixture projects when investigating tool execution.

Rust Analyzer loads both Cargo workspaces and runs Clippy on save. Rustfmt formats
Rust; Prettier uses the repository's installed tooling package for web code and
JSON. Generated output is excluded from search and file watching.

These files are shared configuration. Keep personal paths, credentials, and
machine-specific overrides in VS Code's user settings.
