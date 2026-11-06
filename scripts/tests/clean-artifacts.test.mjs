import assert from 'node:assert/strict';
import { access, mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { cleanArtifacts } from '../clean-artifacts.ts';

async function fixture(t) {
  const root = await realpath(await mkdtemp(path.join(os.tmpdir(), 'jynx-clean-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, 'workspace.config.json'), JSON.stringify({ packages: {
    harness: { path: 'packages/harness' }, cli: { path: 'packages/cli' },
    python: { path: 'packages/sdk-python' }, desktop: { path: 'packages/private/Desktop' },
    extension: { path: 'packages/private/jynx-code' }, missing: { path: 'packages/absent' },
  }}));
  const file = async relative => {
    const target = path.join(root, relative);
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, 'fixture');
    return target;
  };
  return { root, file };
}

test('cleans current native, SDK, packaging and private component outputs while preserving source and state', async t => {
  const { root, file } = await fixture(t);
  const generated = [
    'node_modules/a/index.js', 'dist/Jynx-CLI.zip', '.cache/public-installers/payload.bin',
    'packages/harness/target/release/jynx-harness', 'packages/cli/target/debug/jynx-cli',
    'packages/sdk-python/build/lib/module.py', 'packages/sdk-python/src/jynx.egg-info/PKG-INFO',
    'packages/sdk-python/src/jynx/__pycache__/module.pyc',
    'packages/private/Desktop/build/main.js', 'packages/private/Desktop/.runtime-staging/Harness/bin',
    'packages/private/Desktop/resources/bin/jynx-harness',
    'packages/private/jynx-code/dist/webview.js', 'packages/private/jynx-code/resources/notices/a.txt',
    'packages/private/jynx-code/THIRD_PARTY_NOTICES.md',
    'packages/private/jynx-code/node_modules/react/index.js',
    'packages/private/jynx-code/test.vsix', 'packages/private/Desktop/types.tsbuildinfo',
    'vendor/llama.cpp/binaries/windows/llama-server.exe',
  ];
  const preserved = ['packages/harness/agent/lib.rs', 'packages/cli/Cargo.lock',
    'packages/private/Desktop/.git/config', 'packages/private/jynx-code/src/webview/index.tsx',
    'packages/private/jynx-code/assets/jynx.svg', 'vendor/llama.cpp/source/CMakeLists.txt',
    'packages/private/jynx-code/.development/profile/Cache/state', '.jynx-state/session.json', 'models/model.gguf', 'docs/building.md', 'package-lock.json'];
  for (const name of [...generated, ...preserved]) await file(name);
  const preview = await cleanArtifacts({ root, dryRun: true, log() {} });
  for (const name of generated) await access(path.join(root, name));
  const removed = await cleanArtifacts({ root, log() {} });
  assert.deepEqual(removed, preview);
  for (const name of generated) await assert.rejects(access(path.join(root, name)));
  for (const name of preserved) await access(path.join(root, name));
  assert.deepEqual(await cleanArtifacts({ root, log() {} }), []);
});

test('keeping dependencies still removes other build outputs', async t => {
  const { root, file } = await fixture(t);
  const dependency = await file('packages/private/jynx-code/node_modules/react/index.js');
  const output = await file('packages/private/jynx-code/dist/webview.js');
  await cleanArtifacts({ root, keepDependencies: true, log() {} });
  await access(dependency);
  await assert.rejects(access(output));
});

test('rejects component paths outside the workspace before deleting anything', async t => {
  const { root, file } = await fixture(t);
  const output = await file('dist/keep.txt');
  await writeFile(path.join(root, 'workspace.config.json'), JSON.stringify({ packages: { bad: { path: '../outside' } } }));
  await assert.rejects(cleanArtifacts({ root, log() {} }), /Unsafe component path/);
  await access(output);
});

test('does not traverse directory links into another checkout', async t => {
  const { root, file } = await fixture(t);
  const external = await realpath(await mkdtemp(path.join(os.tmpdir(), 'jynx-clean-outside-')));
  t.after(() => rm(external, { recursive: true, force: true }));
  await mkdir(path.join(external, 'target'));
  await writeFile(path.join(external, 'target', 'keep.txt'), 'keep');
  await file('packages/private/placeholder');
  await symlink(external, path.join(root, 'packages/private/Desktop'), process.platform === 'win32' ? 'junction' : 'dir');
  await cleanArtifacts({ root, log() {} });
  assert.equal(await readFile(path.join(external, 'target', 'keep.txt'), 'utf8'), 'keep');
});
