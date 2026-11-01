#!/usr/bin/env node
import { readdir, rm } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dryRun = process.argv.includes('--dry-run');
const explicitTargets = [
  path.join(projectRoot, 'node_modules'),
  path.join(projectRoot, 'packages', 'private', 'Desktop', 'build'),
  path.join(projectRoot, 'packages', 'private', 'Desktop', 'dist'),
  path.join(projectRoot, 'packages', 'private', 'Desktop', '.runtime-staging'),
  path.join(projectRoot, 'resources', 'bin'),
  path.join(projectRoot, 'dist'),
  path.join(projectRoot, 'packages', 'harness', 'target'),
  path.join(projectRoot, 'packages', 'cli', 'target'),
];

const generatedDirectoryNames = new Set([
  'node_modules',
  'target',
  'dist',
  'build',
  'out',
  'coverage',
  'storybook-static',
  '.cache',
  '.vite',
  '.electron-vite',
  '.webpack',
  '.turbo',
  '.parcel-cache',
  '.next',
  '.nuxt',
  '.svelte-kit',
  '.output',
  '.nyc_output',
  '.jest',
]);

const generatedFilePatterns = [
  /\.log$/i,
  /\.tmp$/i,
  /\.temp$/i,
  /\.pid$/i,
  /\.pid\.lock$/i,
  /\.tsbuildinfo$/i,
  /^\.eslintcache$/i,
  /^\.DS_Store$/,
  /^Thumbs\.db$/i,
];

function assertSafeTarget(target) {
  const resolved = path.resolve(target);
  const relative = path.relative(projectRoot, resolved);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error(`Refusing to clean unsafe path: ${resolved}`);
  }
  return resolved;
}

async function discoverGenerated(root) {
  const found = [];
  const visit = async (directory) => {
    const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
    for (const entry of entries) {
      if (entry.name === '.git') continue;
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        if (generatedDirectoryNames.has(entry.name) || (directory === projectRoot && entry.name.startsWith('bazel-'))) {
          found.push(candidate);
        } else {
          await visit(candidate);
        }
      } else if (entry.isFile() && generatedFilePatterns.some((pattern) => pattern.test(entry.name))) {
        found.push(candidate);
      }
    }
  };
  await visit(root);
  return found;
}

const discoveredTargets = await discoverGenerated(projectRoot);
const targets = [...new Set([...explicitTargets, ...discoveredTargets].map((target) => path.resolve(target)))].sort();

for (const candidate of targets) {
  const target = assertSafeTarget(candidate);
  const label = path.relative(projectRoot, target) || target;
  if (dryRun) console.log(`[dry run] ${label}`);
  else await rm(target, { recursive: true, force: true, maxRetries: 3, retryDelay: 150 });
}
