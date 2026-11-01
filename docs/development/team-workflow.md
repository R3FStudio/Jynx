# Working as a team

## Establish a working baseline

Read the repository guide, install the tool versions required by the package manifests, and run `npm ci`, `npm run doctor`, and `npm run build:all:exclude-desktop`. Keep a known working commit to compare against when diagnosing a regression. Use a disposable project and controlled model endpoint for development; personal conversations and API credentials are not test fixtures.

## Split work by responsibility

The harness owns agent behavior, tools, permissions, model adapters, context, and persistence. The CLI owns terminal interaction. SDKs own client APIs and language-specific behavior. Build scripts own discovery, archives, and release automation. Agree on a shared interface before two contributors change its producer and consumer independently.

For a larger change, write down the observed problem, intended behavior, owning component, acceptance checks, and compatibility constraints in the issue or pull request. Keep unrelated cleanup out of that change. Assign one owner for a cross-package interface and coordinate edits to shared manifests and lockfiles.

## Branch and edit

Create a short-lived branch from the team's current main branch. Prefer small commits that build on their own. If multiple people work on the same machine, use separate clones or Git worktrees rather than switching branches underneath another person's process. Each checkout needs its own dependencies and disposable state directory.

Check status before staging. Never add credentials, model weights, caches, generated binaries, or local conversation data. The [Git workflow](git-workflow.md) covers commits across configured checkouts. A combined command still produces separate commits; agree which public revision an integration needs.

## Review and test

A useful pull request explains the concrete problem, resulting behavior, affected interfaces, and verification. Include a short reproduction and actual test results. For a UI or terminal change, include a representative capture without personal data. For a protocol change, show the old/new request behavior and how existing clients are affected.

Review cancellation, denied approvals, malformed data, missing dependencies, and platform differences when relevant. Prefer focused fixtures over tests that start the whole application. Use [testing](testing.md) for available commands. Re-run only the checks affected by later edits, then run the release checks when preparing a release.

## Resolve conflicts deliberately

Resolve source conflicts in the owning layer first. For a lockfile conflict, settle the desired manifest requirements and regenerate the lockfile with its package manager; do not concatenate competing lockfile sections. After merging, run version checks and the affected builds again. Do not overwrite another contributor's work to remove a conflict.

## Keep documentation useful

Document workflows and public contracts, rather than temporary implementation details. Link to the manifest, command help, configuration, or workflow that owns a value. Avoid copying current version numbers, dependency lists, runner matrices, and exact test counts into prose.

When changing behavior, update its guide in the same pull request. CI checks version alignment and local documentation links. Those checks catch drift in structure; reviewers still need to confirm that the instructions describe the resulting behavior. Archive obsolete instructions instead of leaving two contradictory workflows.
