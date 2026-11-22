import { spawn } from 'node:child_process';
import { readFile, access, chmod, cp, mkdir, mkdtemp, rename, rm, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { pathToFileURL } from 'node:url';
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



export async function executablePath(source, bundleDirectory = path.dirname(source)) {
  if (process.platform !== 'linux') return source;
  try { await access(source, constants.X_OK); return source; } catch (error) {
    if (error.code !== 'EACCES') throw error;
  }
  const info = await stat(source);
  const key = createHash('sha256').update(`${source}:${bundleDirectory}:${info.size}:${info.mtimeMs}`).digest('hex').slice(0, 24);
  const cache = path.join(os.homedir(), '.cache', 'jynx', 'executables');
  await mkdir(cache, { recursive: true, mode: 0o700 });
  const destination = path.join(cache, key);
  const relative = bundleDirectory === source ? path.basename(source) : path.relative(bundleDirectory, source);
  const executable = path.join(destination, relative);
  try { await access(executable, constants.X_OK); return executable; } catch {}
  const temporary = await mkdtemp(path.join(cache, '.prepare-'));
  try {
    await cp(bundleDirectory, bundleDirectory === source ? path.join(temporary, relative) : temporary, { recursive: true });
    await chmod(path.join(temporary, relative), 0o755);

    if (path.basename(source) === 'electron') await chmod(path.join(temporary, 'chrome_crashpad_handler'), 0o755);
    try { await rename(temporary, destination); } catch (error) {
      if (!['EEXIST', 'ENOTEMPTY'].includes(error.code)) throw error;
    }
  } finally { await rm(temporary, { recursive: true, force: true }); }
  await access(executable, constants.X_OK);
  return executable;
}

export async function runNodeBin(packageName, binName, args = [], options = {}) {
  const { resolveFrom, ...spawnOptions } = options;
  const cli = await packageBin(packageName, binName, resolveFrom ?? options.cwd);
  if (process.platform === 'linux' && packageName === 'typescript') {
    const scopedRequire = createRequire(path.join(resolveFrom ?? options.cwd ?? process.cwd(), 'package.json'));
    const resolver = path.join(path.dirname(scopedRequire.resolve('typescript/package.json')), 'lib', 'getExePath.js');
    try {
      await access(resolver);
    } catch (error) {
      if (error.code === 'ENOENT') return run(process.execPath, [cli, ...args], spawnOptions);
      throw error;
    }
    const { default: getExePath } = await import(pathToFileURL(resolver).href);
    return run(await executablePath(getExePath()), args, spawnOptions);
  }
  if (process.platform === 'linux' && packageName === 'electron') {
    const scopedRequire = createRequire(path.join(resolveFrom ?? options.cwd ?? process.cwd(), 'package.json'));
    return run(await executablePath(scopedRequire('electron')), args, spawnOptions);
  }
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
