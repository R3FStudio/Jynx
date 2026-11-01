import { createHash } from 'node:crypto';
import { readdir, readFile, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { run, waitForExit } from './process-runner.ts';
import { harnessRoot, rustTargetRoot, workspaceRoot } from './workspace-paths.ts';

const targetRoot = rustTargetRoot;
const stampPath = path.join(targetRoot, '.jynx-source-fingerprint');
const binaryName = process.platform === 'win32' ? 'jynx-harness.exe' : 'jynx-harness';
const binaryPath = path.join(targetRoot, 'release', binaryName);

async function collectFiles(root: string, relative = ''): Promise<string[]> {
  const directory = path.join(root, relative);
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    if (entry.name === 'target' || entry.name === '.git') continue;
    const next = path.join(relative, entry.name);
    if (entry.isDirectory()) files.push(...(await collectFiles(root, next)));
    else if (/\.(rs|toml|lock)$/i.test(entry.name)) files.push(next);
  }
  return files;
}

async function sourceFingerprint() {
  const hash = createHash('sha256');
  for (const root of ['protocol', 'security', 'sandbox', 'context', 'tools', 'model', 'mcp', 'agent', 'storage', 'server']) {
    const files = (await collectFiles(path.join(harnessRoot, root))).sort((a, b) => a.localeCompare(b));
    for (const relative of files) {
      hash.update(`${root}/${relative.replaceAll('\\', '/')}`);
      hash.update('\0');
      hash.update(await readFile(path.join(harnessRoot, root, relative)));
      hash.update('\0');
    }
  }
  for (const name of ['Cargo.toml', 'Cargo.lock']) {
    hash.update(name);
    hash.update('\0');
    hash.update(await readFile(path.join(harnessRoot, name)));
    hash.update('\0');
  }
  return hash.digest('hex');
}

async function readStamp() {
  try {
    return (await readFile(stampPath, 'utf8')).trim();
  } catch {
    return '';
  }
}

const cargoVersion = run('cargo', ['--version'], { cwd: harnessRoot, env: process.env });
await waitForExit(cargoVersion, 'Rust toolchain check');

const fingerprint = await sourceFingerprint();
const previous = await readStamp();
if (previous !== fingerprint) {
  console.log('[harness] source changed; Cargo will rebuild affected crates incrementally');
}

console.log('[harness] building server');
const build = run('cargo', ['build', '--release', '--manifest-path', path.join(harnessRoot, 'Cargo.toml'), '-p', 'jynx-server', '--bin', 'jynx-harness'], { cwd: workspaceRoot, env: process.env });
await waitForExit(build, 'Jynx Harness build');
await mkdir(targetRoot, { recursive: true });
await writeFile(stampPath, `${fingerprint}\n`, 'utf8');
console.log(`[harness] ready: ${binaryPath}`);
