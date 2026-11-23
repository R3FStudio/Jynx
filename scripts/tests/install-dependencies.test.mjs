import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { installDependencies } from '../install-dependencies.mjs';

test('installs the workspace once and each present standalone checkout independently', async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-install-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const packages = {
    sdk: { path: 'sdk', name: 'sdk' },
    desktop: { path: 'desktop', name: 'desktop', standalone: true },
    extension: { path: 'extension', name: 'extension', standalone: true },
  };
  await writeFile(path.join(root, 'package.json'), JSON.stringify({ workspaces: ['sdk'] }));
  await writeFile(
    path.join(root, 'workspace.config.json'),
    JSON.stringify({ buildOrder: Object.keys(packages), packages }),
  );
  for (const id of ['sdk', 'desktop']) {
    await mkdir(path.join(root, id));
    await writeFile(
      path.join(root, id, 'package.json'),
      JSON.stringify({ name: id, scripts: { build: 'node build.mjs' } }),
    );
  }
  const calls = [];
  await installDependencies({ root, install: async (...args) => calls.push(args), log() {} });
  assert.deepEqual(calls, [
    [root, { standalone: false, command: 'ci' }],
    [path.join(root, 'desktop'), { standalone: true, command: 'ci' }],
  ]);
  calls.length = 0;
  await installDependencies({ root, standaloneOnly: true, install: async (...args) => calls.push(args), log() {} });
  assert.deepEqual(calls, [[path.join(root, 'desktop'), { standalone: true, command: 'install' }]]);
  calls.length = 0;
  await installDependencies({ root, standaloneOnly: true, command: 'ci', install: async (...args) => calls.push(args), log() {} });
  assert.deepEqual(calls, [[path.join(root, 'desktop'), { standalone: true, command: 'ci' }]]);
  await assert.rejects(
    installDependencies({
      root,
      install: async () => {
        throw new Error('install failed');
      },
      log() {},
    }),
    /install failed/,
  );
  await writeFile(path.join(root, 'desktop/package.json'), 'invalid');
  await assert.rejects(
    installDependencies({ root, install: async () => assert.fail('must validate before installing'), log() {} }),
    /desktop/,
  );
});


test('root postinstall skips absent private checkouts without reinstalling the public workspace', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-public-install-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, 'package.json'), JSON.stringify({workspaces:[]}));
  await writeFile(path.join(root, 'workspace.config.json'), JSON.stringify({
    buildOrder:['desktop','extension'], packages:{
      desktop:{path:'desktop',name:'desktop',standalone:true},
      extension:{path:'extension',name:'extension',standalone:true},
    },
  }));
  const installed = await installDependencies({root,standaloneOnly:true,install:async()=>assert.fail('Nothing should be installed'),log(){}});
  assert.deepEqual(installed, []);
});
