# Versions and component updates

The root `package.json` is the release-version source of truth. Owned npm packages, Rust crates, SDK distribution metadata, the API document, and installed optional components use that version. Third-party dependencies and downloaded runtime bundles keep their upstream versions.

## Change the release version

From the repository root:

```sh
npm run version:check
npm run version:set -- 0.0.2
npm run version:check
```

`0.0.2` is an example next release, not a value to paste for every release. Choose the intended version first. The setter updates related manifests and owned lockfile entries without upgrading dependencies, committing, tagging, or publishing. Missing optional checkouts are skipped. Install a linked checkout before setting a coordinated release version if it must participate.

Review the diff, run the relevant tests, and commit before creating a tag. CI rejects version drift. A native executable gets its version at compile time; editing a manifest does not change a binary already built. Rebuild it and check `jynx-cli --version`.

## Update a dependency

1. Identify the package that owns the dependency. npm dependencies are declared in that package's `package.json`; Rust dependencies in its Cargo manifest; Python packaging requirements in `pyproject.toml`.
2. Read the upstream release notes and change the intended requirement. Avoid upgrading unrelated dependencies in the same change.
3. Use the owning package manager to refresh its lockfile. For a public npm workspace, run `npm install` from the root. For Rust, use `cargo update --manifest-path packages/cli/Cargo.toml -p DEPENDENCY` or the corresponding harness manifest. Replace `DEPENDENCY` with the actual crate name.
4. Test the integration that uses it, not only installation. Review any platform, API, or license changes.
5. Commit the manifest and lockfile together. Keep dependency updates separate from behavior changes when that makes review easier.

The CLI and harness have separate Cargo lockfiles. A shared Rust dependency change can require refreshing both. Do not hand-edit third-party checksums or change upstream dependency versions with the Jynx version command.

## Update a runtime

Runtime selectors and download logic are defined in the build scripts. Follow [releases](releases.md) for runtime packaging. Runtime versions are independent of Jynx versions; model files and context settings are not application versions.

For an SDK or protocol change, update implementation, type declarations, fixtures, and examples together. Record compatibility changes in the release notes. Version alignment does not make an incompatible interface safe for old clients.
