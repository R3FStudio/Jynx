# Testing

```sh
npm run docs:check
npm run version:check
npm run lint
npm run test:scripts
npm run sdk:test
cargo test --manifest-path packages/harness/Cargo.toml --workspace
cargo test --manifest-path packages/cli/Cargo.toml
```

Run the checks relevant to the change. Script tests cover component discovery, partial builds, archive extraction, and linked Git operations. Git tests use disposable repositories and local bare remotes; they do not publish to GitHub. SDK tests use fixtures and local servers. Rust tests cover stream parsing, execution, approvals, context, and persistence.

## Test data

Do not use personal conversation stores, provider keys, or integration configurations. Set `JYNX_STATE_DIR` and `JYNX_MCP_CONFIG` to a disposable location for diagnostic sessions that could access state. Use a fixture project for file writes and commands.

Model tests should supply controlled local responses rather than rely on a live model behaving identically every run. Include empty, interrupted, and malformed responses when testing recovery. Confirm that accepted actions are not repeated.

## Verification

For build changes, test complete and partial checkouts. For tools, test success, failure, denial, and cancellation. For protocol changes, check both the Rust implementation and its SDK consumer.

A successful compile is not proof of a successful model task. Execute the relevant fixture command or inspect the resulting fixture file. Keep routine tests offline unless a test explicitly verifies a network integration.
