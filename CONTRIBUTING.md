# Contributing

Start with the [repository guide](docs/development/repository.md) and [build guide](docs/development/building.md). Keep changes focused on the layer that owns the behavior and include a meaningful regression test.

Run the relevant [checks](docs/development/testing.md), review the diff, and describe the user-visible result. Include the command or fixture that verifies it. Leave out unrelated formatting, build output, weights, credentials, and personal state.

Documentation belongs in `docs`. Use direct instructions, realistic examples, and short explanations of unfamiliar terms. Keep the root README descriptive. Avoid diagrams that need updating after ordinary implementation changes.

See [Git workflow](docs/development/git-workflow.md) for ordinary and linked commits, and [releases](docs/development/releases.md) for artifacts.

Read the [team workflow](docs/development/team-workflow.md) for coordinating changes, reviewing interfaces, and resolving conflicts. Run `npm run version:check` before release work.
