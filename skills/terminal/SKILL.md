---
name: terminal
description: Choose between normal Harness shell work and the explicitly requested side-panel terminal.
---

# Terminal

Use `run_command` for normal builds, tests, package managers, version-control commands, and other shell-native work. Normal command activity stays in the conversation and must not open or take over Jynx's side-panel terminal.

Use `run_in_terminal_panel` only when the user explicitly asks you to use, show, or run a command in the side-panel Terminal. It executes under the same Harness permission rules while allowing the proprietary desktop host to open that panel and show the command and result.

Use the typed file tools for reading, listing, searching, and editing files. Respect the active permission mode, explain consequential commands before running them, and report non-zero exit output accurately.
