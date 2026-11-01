import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

export function packageBin(packageName, binName, fromDirectory) {
  const scopedRequire = fromDirectory ? createRequire(path.join(fromDirectory, 'package.json')) : require;
  const packageJsonPath = scopedRequire.resolve(`${packageName}/package.json`);
  return readFile(packageJsonPath, 'utf8').then((text) => {
    const pkg = JSON.parse(text);
    const relative = typeof pkg.bin === 'string' ? pkg.bin : pkg.bin?.[binName];
    if (!relative) throw new Error(`${packageName} does not expose a ${binName} binary.`);
    return path.resolve(path.dirname(packageJsonPath), relative);
  });
}

export function run(command, args = [], options = {}) {
  return spawn(command, args, {
    stdio: 'inherit',
    shell: false,
    ...options,
  });
}

export async function runNodeBin(packageName, binName, args = [], options = {}) {
  const { resolveFrom, ...spawnOptions } = options;
  const cli = await packageBin(packageName, binName, resolveFrom ?? options.cwd);
  return run(process.execPath, [cli, ...args], spawnOptions);
}

export function waitForExit(child, label) {
  return new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${label} failed${code == null ? ` (${signal ?? 'terminated'})` : ` (${code})`}`));
    });
  });
}
