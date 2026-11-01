import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultLlamaRuntime, selectLlamaTargets } from '../llama-runtime-targets.ts';

test('normal sync stages every supported OS and architecture regardless of the host', () => {
  const windows = selectLlamaTargets([], 'win32', 'x64');
  assert.deepEqual(selectLlamaTargets([], 'darwin', 'arm64'), windows);
  assert.equal(windows.length, 16);
  assert.deepEqual([...new Set(windows.map(({ platform, arch }) => `${platform}/${arch}`))].sort(), [
    'linux/arm64', 'linux/x64', 'macos/arm64', 'macos/x64', 'windows/arm64', 'windows/x64',
  ]);
  for (const platform of ['windows', 'linux', 'macos']) {
    for (const arch of ['x64', 'arm64']) {
      const runtimes = windows.filter((target) => target.platform === platform && target.arch === arch).map(({ runtime }) => runtime);
      assert.ok(runtimes.includes('cpu'));
      assert.ok(runtimes.includes(defaultLlamaRuntime(platform, arch)));
    }
  }
});

test('selectors retain single-backend sync and support all backends for a packaging target', () => {
  assert.deepEqual(selectLlamaTargets(['--platform', 'linux', '--arch', 'arm64'], 'win32', 'x64').map(({ runtime }) => runtime), ['vulkan', 'cuda', 'cpu']);
  assert.deepEqual(selectLlamaTargets(['--runtime', 'cpu'], 'win32', 'arm64'), [{ platform: 'windows', arch: 'arm64', runtime: 'cpu' }]);
  assert.deepEqual(selectLlamaTargets(['--host'], 'darwin', 'arm64'), [{ platform: 'macos', arch: 'arm64', runtime: 'metal' }]);
  assert.equal(defaultLlamaRuntime('darwin', 'x64'), 'cpu');
  assert.equal(defaultLlamaRuntime('win32', 'arm64'), 'cpu');
  assert.throws(() => selectLlamaTargets(['--platform', 'macos', '--runtime', 'vulkan'], 'win32', 'x64'), /Unsupported/);
  assert.throws(() => selectLlamaTargets(['--arch', 'ia32'], 'win32', 'x64'), /Unsupported/);
  assert.throws(() => selectLlamaTargets(['--platform'], 'win32', 'x64'), /Missing value/);
  assert.throws(() => selectLlamaTargets(['--all', '--runtime', 'cpu'], 'win32', 'x64'), /not both/);
});

test('portable installer sync includes CPU and the default backend without unrelated GPU downloads',()=>{
 assert.deepEqual(selectLlamaTargets(['--portable'],'win32','x64').map(t=>t.runtime),['vulkan','cpu']);
 assert.deepEqual(selectLlamaTargets(['--portable'],'darwin','arm64').map(t=>t.runtime),['metal','cpu']);
 assert.deepEqual(selectLlamaTargets(['--portable'],'win32','arm64').map(t=>t.runtime),['cpu']);
 assert.throws(()=>selectLlamaTargets(['--portable','--runtime','cpu'],'linux','x64'),/not both/);
});
