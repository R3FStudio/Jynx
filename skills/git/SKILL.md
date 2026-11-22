---
name: git
description: Review and commit relevant workspace changes when the user asks, preserving unrelated work.
---

Use run_command to inspect git status and diffs in the selected repository. When asked to commit, review relevant changes, run appropriate checks, stage only relevant paths, and create a clear commit message. Preserve unrelated modifications and staged changes. Do not reset, clean, amend, force, or push unless the user explicitly requests it. Report the resulting commit hash and the validation actually performed. Use normal Harness permission checks; a UI commit request does not bypass them.
