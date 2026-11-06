#!/usr/bin/env node
import { lstat, readdir, readFile, realpath, rm } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directoryNames = new Set([
  'node_modules', 'target', 'dist', 'build', 'out', 'coverage', 'storybook-static',
  '.cache', '.vite', '.electron-vite', '.webpack', '.turbo', '.parcel-cache',
  '.next', '.nuxt', '.svelte-kit', '.output', '.nyc_output', '.jest',
  '__pycache__', '.pytest_cache', '.mypy_cache', '.ruff_cache',
]);
const filePatterns = [/\.tsbuildinfo$/i, /^\.eslintcache$/i, /\.py[co]$/i];
const skipDirectories = new Set(['.git', '.development', '.jynx', '.jynx-state', 'models', 'chats', '.venv', 'venv', 'vendor']);

function inside(root, target) {
  const relative = path.relative(root, target);
  return Boolean(relative) && relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative);
}

async function safeTarget(root, candidate) {
  const target = path.resolve(candidate);
  if (!inside(root, target)) throw new Error(`Refusing to clean unsafe path: ${target}`);
  let current = root;
  for (const part of path.relative(root, target).split(path.sep)) {
    current = path.join(current, part);
    const stat = await lstat(current).catch(error => {
      if (error.code === 'ENOENT') return null;
      throw error;
    });
    if (!stat) return null;
    if (stat.isSymbolicLink()) return null;
    if (!inside(root, await realpath(current))) throw new Error(`Refusing to follow external path: ${current}`);
  }
  return target;
}

export async function cleanArtifacts({ root = projectRoot, dryRun = false, keepDependencies = false, log = console.log } = {}) {
  root = await realpath(root);
  const config = JSON.parse(await readFile(path.join(root, 'workspace.config.json'), 'utf8'));
  const components = Object.values(config.packages || {}).map(component => {
    if (!component || typeof component.path !== 'string') throw new Error('Invalid component path in workspace.config.json');
    const directory = path.resolve(root, component.path);
    if (!inside(root, directory)) throw new Error(`Unsafe component path: ${component.path}`);
    return directory;
  });
  const candidates = new Set();
  const add = candidate => candidates.add(path.resolve(candidate));
  for (const base of [root, ...components]) {
    if (!(base === root || await safeTarget(root, base))) continue;
    for (const name of directoryNames) {
      if (keepDependencies && name === 'node_modules') continue;
      add(path.join(base, name));
    }
    add(path.join(base, 'resources', 'bin'));
    add(path.join(base, '.runtime-staging'));
  }
  const extension = config.packages?.extension?.path;
  if (extension) {
    add(path.resolve(root, extension, 'resources'));
    add(path.resolve(root, extension, '.vscode-test'));
    add(path.resolve(root, extension, 'THIRD_PARTY_NOTICES.md'));
  }
  add(path.join(root, 'vendor', 'llama.cpp', 'binaries'));
  const visit = async directory => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (entry.isSymbolicLink() || skipDirectories.has(entry.name)) continue;
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        if (directoryNames.has(entry.name) || entry.name.endsWith('.egg-info') || entry.name.endsWith('.dist-info')) {
          if (!(keepDependencies && entry.name === 'node_modules')) add(candidate);
        } else if (!(directory === root && ['packages', 'tools'].includes(entry.name))) {
          await visit(candidate);
        }
      } else if (entry.isFile() && (filePatterns.some(pattern => pattern.test(entry.name)) ||
        (directory === root || components.includes(directory)) && /\.(vsix|tgz|whl)$/i.test(entry.name))) add(candidate);
    }
  };
  for (const base of [root, ...components]) {
    if (base === root || await safeTarget(root, base)) await visit(base);
  }
  const targets = [];
  for (const candidate of [...candidates].sort((a, b) => a.length - b.length || a.localeCompare(b))) {
    if (targets.some(parent => candidate === parent || inside(parent, candidate))) continue;
    const target = await safeTarget(root, candidate);
    if (target) targets.push(target);
  }
  for (const target of targets) {
    if (!(await safeTarget(root, target))) continue;
    log(`${dryRun ? '[dry run]' : '[clean]'} ${path.relative(root, target)}`);
    if (!dryRun) await rm(target, { recursive: true, force: true, maxRetries: 3, retryDelay: 150 });
  }
  log(`${dryRun ? 'Would remove' : 'Removed'} ${targets.length} artifact paths.`);
  return targets;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  if (args.some(arg => !['--dry-run', '--keep-dependencies'].includes(arg))) throw new Error('Usage: npm run clean -- [--dry-run] [--keep-dependencies]');
  await cleanArtifacts({ dryRun: args.includes('--dry-run'), keepDependencies: args.includes('--keep-dependencies') });
}
