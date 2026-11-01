# Publish SDK packages

Jynx has two SDK distributions. Publish each to its language's package registry, separately from GitHub release binaries.

| SDK | Registry | Published name | Install command |
| --- | --- | --- | --- |
| JavaScript and TypeScript | npm | `@r3fstudios/jynx-sdk` | `npm install @r3fstudios/jynx-sdk` |
| Python | PyPI | `jynx-python-sdk` | `python -m pip install jynx-python-sdk` |

The npm organization is `r3fstudios`. PyPI does not use npm-style organization scopes: the Python distribution keeps its own project name, and its owners control who can publish it. Python imports stay `from jynx_sdk import JynxClient`.

The Python workspace's `package.json` runs development commands; it is marked private and is not a second npm SDK. Publish the Python distribution described by `pyproject.toml` instead. Do not run a blanket publish across all workspaces.

## Prepare a release

Start from the repository root with dependencies installed. Review and test the SDK changes before publishing. If the intended version already exists on either target registry, choose a new coordinated version using the [versioning guide](versioning.md); published versions cannot be replaced in place.

```sh
npm ci
npm run version:check
npm run sdk:test
npm run docs:check
npm run pack:sdk-js
npm run pack:sdk-python
```

The archives are written directly to root `dist/`. JavaScript uses `Jynx-JavaScript-SDK-vVERSION.tgz`; Python uses a standard wheel filename starting with `jynx_python_sdk-`. These SDK packages contain clients, not model weights, the CLI, or the harness executable.

Check the package manifests and bundled READMEs before uploading. They should link to `https://github.com/R3FStudio/Jynx`, its issues page, and its public SDK and protocol documentation. Package READMEs use full GitHub links so they also work outside a source checkout. Preserve LICENSE and COPYRIGHT in each archive.

## Publish to npm

An npm website session does not automatically log the terminal in. Authenticate the terminal and check organization membership:

```sh
npm login --registry=https://registry.npmjs.org/
npm whoami --registry=https://registry.npmjs.org/
npm org ls r3fstudios --registry=https://registry.npmjs.org/
```

The account must have permission to publish public packages under `r3fstudios`. A missing scope or authorization error must be resolved by an organization owner. Complete any browser or two-factor authentication prompt from npm; do not save credentials in this repository.

Publish the reviewed archive produced by `pack:sdk-js`. These PowerShell commands read the current manifest version, so there is no release number to maintain in this guide:

```powershell
$version = (Get-Content packages/sdk-js/package.json | ConvertFrom-Json).version
npm publish "./dist/Jynx-JavaScript-SDK-v$version.tgz" --access public --registry=https://registry.npmjs.org/ --dry-run
npm publish "./dist/Jynx-JavaScript-SDK-v$version.tgz" --access public --registry=https://registry.npmjs.org/
npm view "@r3fstudios/jynx-sdk@$version" version repository homepage bugs dist.integrity --json --registry=https://registry.npmjs.org/
```

The `--dry-run` command shows the package contents without publishing. Continue with the real publish only after reviewing that list. `publishConfig` also sets the public npm registry and public access in the package manifest. The archive's readable filename does not determine the npm package name; its bundled `package.json` does.

Check the [npm package page](https://www.npmjs.com/package/@r3fstudios/jynx-sdk). In a temporary folder outside the monorepo, install the published version and import both `@r3fstudios/jynx-sdk` and `@r3fstudios/jynx-sdk/harness`. Do not use an existing workspace link as proof that the registry package installs correctly.

## Publish to PyPI

Create a PyPI account and obtain publishing access to `jynx-python-sdk`. If the project has not been published, its first authorized upload creates it if the name is available. If someone else owns the name, resolve ownership before publishing; an npm organization does not grant PyPI access.

Use an isolated environment for packaging tools. The following commands use PowerShell on Windows; on Linux or macOS, create the environment with `python3` and use `.cache/sdk-publish-venv/bin/python` instead of the Windows interpreter path.

```powershell
python -m venv .cache/sdk-publish-venv
& ./.cache/sdk-publish-venv/Scripts/python.exe -m pip install --upgrade build twine
& ./.cache/sdk-publish-venv/Scripts/python.exe -m build packages/sdk-python --outdir dist
$version = (Get-Content packages/sdk-python/package.json | ConvertFrom-Json).version
& ./.cache/sdk-publish-venv/Scripts/python.exe -m twine check "dist/jynx_python_sdk-$version.tar.gz" "dist/jynx_python_sdk-$version-py3-none-any.whl"
```

This produces a source distribution and a wheel in root `dist/` and checks that PyPI can render their package metadata. Keep the file selection specific to this SDK; `dist/*` also includes native executables and npm packages, which do not belong on PyPI.

For a trial upload, create a separate [TestPyPI](https://test.pypi.org/) account and token, then run:

```powershell
& ./.cache/sdk-publish-venv/Scripts/python.exe -m twine upload --repository testpypi "dist/jynx_python_sdk-$version.tar.gz" "dist/jynx_python_sdk-$version-py3-none-any.whl"
```

For the real release, use the production PyPI account and token:

```powershell
& ./.cache/sdk-publish-venv/Scripts/python.exe -m twine upload "dist/jynx_python_sdk-$version.tar.gz" "dist/jynx_python_sdk-$version-py3-none-any.whl"
```

For token authentication, enter `__token__` as the username and the PyPI API token as the password when Twine prompts. TestPyPI and PyPI credentials are separate. Keep tokens out of commands, source files, and Git commits. For automated releases, use [PyPI Trusted Publishing](https://docs.pypi.org/trusted-publishers/) instead of committing a token or relying on a local login.

Check the [PyPI project page](https://pypi.org/project/jynx-python-sdk/) after uploading. Install the published version into a fresh environment, then import `JynxClient` from `jynx_sdk` and exercise it against a compatible test API.

## Finish the release

Confirm the package version, ownership, public visibility, and documentation links on each registry. Commit the manifest, lockfile, SDK code, and documentation together and push the reviewed source to GitHub. GitHub release assets and SDK registry packages are separate uploads; publishing one does not publish the other.

If an upload fails, check the registry before retrying: a successful upload followed by a network error may already have reserved the version. Fix a published package with a new version rather than trying to overwrite its archive.

For current registry requirements, see [npm organization publishing](https://docs.npmjs.com/creating-and-publishing-an-organization-scoped-package/), the [Python packaging tutorial](https://packaging.python.org/en/latest/tutorials/packaging-projects/), and [PyPI publishing guidance](https://docs.pypi.org/publishing/publishing-to-pypi/).
