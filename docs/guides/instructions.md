# Jynx instructions

Create `jynx.md` in the project folder passed to Jynx. Write ordinary Markdown describing the conventions Jynx should follow: how to build and test, which files are generated, coding style, and any limits on autonomous work.

The shared harness loads this file for local models and API providers, including autonomous tool loops. It is not an extension-only setting. No restart is needed after saving: new requests load the file again. The selected project root matters; Jynx does not scan every subdirectory for additional instruction files.

For defaults across projects, create `jynx.md` in your Jynx home directory. This is `~/.jynx/jynx.md`, or `<JYNX_HOME>/jynx.md` when `JYNX_HOME` is set. Project guidance takes precedence over those personal defaults. Keep each file short; loading is bounded to 32 KiB total, and oversized guidance is marked as truncated.

An example project file:

```markdown
# Project conventions

Use Rust for the application and keep platform-specific code in its own modules.
Run cargo test before reporting a code change as complete.
Ask before introducing a new runtime dependency.
Do not edit generated files in dist.

## Autonomous work

Complete related implementation and validation within the current task.
Use small, concrete progress updates and report any checks that could not run.
```

These are working instructions, not permission grants. They cannot bypass approval settings or make an unavailable tool usable. The current user's request still takes precedence. Jynx does not automatically import `AGENTS.md`, `CLAUDE.md`, or another product's instruction files; move any guidance you want Jynx to follow into `jynx.md`.

Instruction files become model context. Avoid putting passwords, access tokens, or other secrets in them, especially when using an external API provider.
