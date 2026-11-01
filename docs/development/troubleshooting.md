# Troubleshooting

## Missing npm package

Check `node --version`, then run `npm ci` at the repository root. Do not copy dependencies from another operating system. `build:all` skips absent source folders; a present package with missing dependencies is a failure.

## Rust or linker errors

Install rustup and your platform's native build tools. Windows needs MSVC C++ tools and the Windows SDK. Read the first compiler error, not just npm's final lifecycle message. `cargo check` helps narrow the problem.

## Locked executable

Close the process using that binary and retry. Windows can refuse to replace a running program. Deleting conversation state will not fix a compiler or file-lock failure.

## Model connection failure

Confirm endpoint, model ID, and key. Test the server independently. A live runtime process may still be loading weights. Check GPU memory and loaded context if initialization fails.

## Blank response

Verify the model template and supported native effort. Transient empty steps are retried with tools and prior results retained. Persistent empty output is reported, not replaced with a fabricated answer. A short request at the default effort can isolate model configuration from a long context.

## Waiting tool

Check for an approval. Ask mode waits for an explicit answer and does not expire after ten minutes. A declined operation has not run. A command already executing has its own timeout and exit status.

## Full context

The server allocation and harness budget must agree. Reduce large input, choose an appropriate context, or compact the conversation. Compaction changes model input; it does not erase the full audit.

## Git remote unavailable

Check existence and authentication. HTTP 404 can mean either missing access or a missing repository. The guard refuses to guess visibility. Local commits are retained when a network operation fails.
