import path from 'node:path';
import { createWriteStream } from 'node:fs';
import { access, copyFile, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { workspaceRoot } from './workspace-paths.ts';
import { extractRuntimeTar } from './extract-runtime-tar.ts';

import { selectLlamaTargets, type LlamaPlatform as Platform, type LlamaTarget as Target } from './llama-runtime-targets.ts';
type Asset = { name: string; browser_download_url: string; size?: number };
type Release = { tag_name?: string; name?: string; draft?: boolean; prerelease?: boolean; assets?: Asset[] };

const projectRoot = workspaceRoot;
const root = path.resolve(projectRoot, 'vendor', 'llama.cpp');
const sourceDir = path.join(root, 'source');
const binariesDir = path.join(root, 'binaries');
const args = process.argv.slice(2);
const arg = (name: string) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const matrix = selectLlamaTargets(args, process.platform, process.arch);

function run(command: string, commandArgs: string[], cwd?: string) {
  return new Promise<void>((resolve, reject) => {
    const child = spawn(command, commandArgs, { cwd, stdio: 'inherit', shell: false, windowsHide: true });
    child.once('error', reject);
    child.once('exit', (code) => (code === 0 ? resolve() : reject(new Error(`${command} exited with ${code}`))));
  });
}

async function githubJson(url: string) {
  const response = await fetch(url, {
    headers: {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'Jynx',
      'X-GitHub-Api-Version': '2022-11-28',
      ...(process.env.GH_TOKEN || process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GH_TOKEN || process.env.GITHUB_TOKEN}` } : {}),
    },
  });
  if (!response.ok) throw new Error(`GitHub request failed with ${response.status}: ${url}`);
  return response.json();
}

async function releaseCandidates(): Promise<Release[]> {
  const explicit = arg('--release');
  if (explicit) {
    const release = (await githubJson(`https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/${encodeURIComponent(explicit)}`)) as Release;
    return [release];
  }
  const releases = (await githubJson('https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=100')) as Release[];
  return releases.filter((release) => !release.draft && Array.isArray(release.assets) && release.assets.length > 0);
}

function archiveAsset(asset: Asset) {
  const lower = asset.name.toLowerCase();
  return lower.startsWith('llama-') && /\.(zip|tar\.gz|tgz)$/.test(lower) && !lower.includes('-ui.') && !lower.includes('xcframework');
}

function runtimeVersion(name: string, marker: 'cuda' | 'rocm') {
  const match = name.toLowerCase().match(new RegExp(`${marker}-([0-9]+(?:\\.[0-9]+)*)-`));
  return match?.[1] || '';
}

function versionScore(version: string) {
  const parts = version.split('.').map((part) => Number(part) || 0);
  return (parts[0] || 0) * 1_000_000 + (parts[1] || 0) * 1_000 + (parts[2] || 0);
}

function assetScore(asset: Asset, target: Target) {
  if (!archiveAsset(asset)) return -1;
  const lower = asset.name.toLowerCase();
  let score = 0;
  if (target.platform === 'windows') {
    if (!/-bin-win-/.test(lower) || !lower.endsWith('.zip')) return -1;
    score += 100;
  } else if (target.platform === 'linux') {
    if (!/-bin-ubuntu-/.test(lower) || !/\.tar\.gz$|\.tgz$/.test(lower)) return -1;
    score += 100;
  } else {
    if (!/-bin-macos-/.test(lower) || !/\.tar\.gz$|\.tgz$/.test(lower)) return -1;
    score += 100;
  }
  if (target.arch === 'arm64') {
    if (!/(?:-|_)arm64(?:\.|-|_)/.test(lower)) return -1;
    score += 50;
  } else {
    if (!/(?:-|_)x64(?:\.|-|_)/.test(lower)) return -1;
    score += 50;
  }
  if (target.platform === 'macos') {
    if (target.runtime !== 'metal' && target.runtime !== 'cpu') return -1;
    return score + 50;
  }
  if (target.runtime === 'vulkan') {
    if (!/-vulkan-/.test(lower)) return -1;
    return score + 50;
  }
  if (target.runtime === 'cuda') {
    if (!/-cuda-/.test(lower)) return -1;
    return score + 50 + versionScore(runtimeVersion(lower, 'cuda'));
  }
  if (target.runtime === 'rocm') {
    if (!/-rocm-/.test(lower)) return -1;
    return score + 50 + versionScore(runtimeVersion(lower, 'rocm'));
  }
  if (/-vulkan-|-cuda-|-rocm-|-sycl-|-openvino-|-opencl-/.test(lower)) return -1;
  if (target.platform === 'windows' && !/-cpu-/.test(lower)) return -1;
  return score + 50;
}

function selectPrimaryAsset(release: Release, target: Target) {
  const ranked = (release.assets || [])
    .map((asset) => ({ asset, score: assetScore(asset, target) }))
    .filter((entry) => entry.score >= 0)
    .sort((a, b) => b.score - a.score || a.asset.name.localeCompare(b.asset.name));
  return ranked[0]?.asset;
}

function companionAssets(release: Release, target: Target, primary: Asset) {
  if (target.runtime !== 'cuda' || target.platform === 'macos') return [] as Asset[];
  const version = runtimeVersion(primary.name, 'cuda');
  if (!version) return [] as Asset[];
  const arch = target.arch;
  return (release.assets || []).filter((asset) => {
    const lower = asset.name.toLowerCase();
    if (!lower.startsWith('cudart-llama')) return false;
    if (!lower.includes(`cuda-${version}-`)) return false;
    if (target.platform === 'windows') return lower.includes('-bin-win-cuda-') && lower.endsWith(`-${arch}.zip`);
    return lower.includes('-bin-ubuntu-cuda-') && (lower.endsWith(`-${arch}.tar.gz`) || lower.endsWith(`-${arch}.tgz`));
  });
}

function targetAvailable(release: Release, target: Target) {
  const primary = selectPrimaryAsset(release, target);
  if (!primary) return false;
  if (target.runtime === 'cuda' && target.platform !== 'macos') return companionAssets(release, target, primary).length > 0;
  return true;
}

function coverage(release: Release, targets: Target[]) {
  return targets.reduce((count, target) => count + (targetAvailable(release, target) ? 1 : 0), 0);
}

async function chooseRelease(targets: Target[]) {
  const releases = await releaseCandidates();
  if (!releases.length) throw new Error('No llama.cpp releases were returned by GitHub.');
  const complete = releases.find((release) => coverage(release, targets) === targets.length);
  if (complete) return complete;
  if (targets.length === 1) {
    const supported = releases.find((release) => coverage(release, targets) === 1);
    if (supported) return supported;
  }
  const ranked = releases
    .map((release) => ({ release, count: coverage(release, targets) }))
    .sort((a, b) => b.count - a.count);
  const best = ranked[0];
  const tag = String(best?.release.tag_name || best?.release.name || 'unknown');
  throw new Error(`No recent llama.cpp release contains the complete requested runtime matrix. Best candidate ${tag} contains ${best?.count || 0}/${targets.length} targets.`);
}

async function download(url: string, target: string) {
  const response = await fetch(url, { headers: { 'User-Agent': 'Jynx' } });
  if (!response.ok || !response.body) throw new Error(`Download failed with ${response.status}: ${url}`);
  const total = Number(response.headers.get('content-length') || 0);
  let transferred = 0;
  let lastPercent = -1;
  const hash = createHash('sha256');
  const reader = response.body.getReader();
  const stream = new Readable({
    async read() {
      const result = await reader.read();
      if (result.done) return this.push(null);
      transferred += result.value.byteLength;
      hash.update(result.value);
      const percent = total ? Math.max(1, Math.min(100, Math.round((transferred / total) * 100))) : 0;
      if (percent && percent !== lastPercent) {
        process.stdout.write(`\r${percent}%`);
        lastPercent = percent;
      }
      this.push(Buffer.from(result.value));
    },
  });
  await pipeline(stream, createWriteStream(target));
  process.stdout.write('\n');
  return hash.digest('hex');
}

async function extract(archive: string, target: string) {
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  const lower = archive.toLowerCase();
  if (lower.endsWith('.zip')) {
    if (process.platform === 'win32') {
      const escapedArchive = archive.replaceAll("'", "''");
      const escapedTarget = target.replaceAll("'", "''");
      await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `Expand-Archive -LiteralPath '${escapedArchive}' -DestinationPath '${escapedTarget}' -Force`]);
    } else {
      await run('unzip', ['-o', archive, '-d', target]);
    }
    return;
  }
  await extractRuntimeTar(archive, target);
}

async function fileExists(target: string) {
  try {
    await access(target);
    return true;
  } catch {
    return false;
  }
}

async function findServer(rootDir: string, platform: Platform) {
  const wanted = platform === 'windows' ? 'llama-server.exe' : 'llama-server';
  if (!(await fileExists(rootDir))) return null;
  const queue = [rootDir];
  while (queue.length) {
    const current = queue.shift()!;
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) queue.push(full);
      else if (entry.name === wanted) return full;
    }
  }
  return null;
}

async function mergeFilesNextToServer(sourceRoot: string, serverDir: string) {
  let copied = 0;
  const queue = [sourceRoot];
  while (queue.length) {
    const current = queue.shift()!;
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        queue.push(full);
      } else if (entry.isFile() || entry.isSymbolicLink()) {
        await copyFile(full, path.join(serverDir, entry.name));
        copied += 1;
      }
    }
  }
  if (!copied) throw new Error(`CUDA companion archive extracted no runtime files from ${sourceRoot}.`);
}

async function syncSource(tag: string) {
  const reference = (await githubJson(`https://api.github.com/repos/ggml-org/llama.cpp/git/ref/tags/${encodeURIComponent(tag)}`)) as { object?: { type?: string; sha?: string } };
  let object = reference.object;
  for (let depth = 0; object?.type === 'tag' && depth < 4; depth += 1) {
    const annotation = (await githubJson(`https://api.github.com/repos/ggml-org/llama.cpp/git/tags/${object.sha}`)) as { object?: { type?: string; sha?: string } };
    object = annotation.object;
  }
  if (object?.type !== 'commit' || !/^[0-9a-f]{40}$/i.test(object.sha || '')) throw new Error(`Could not resolve ${tag} to a llama.cpp commit.`);
  const commit = object.sha!;
  if (await fileExists(path.join(sourceDir, 'CMakeLists.txt'))) {
    const installedCommit = await readFile(path.join(sourceDir, 'JYNX_LLAMA_COMMIT'), 'utf8').catch(() => '');
    if (installedCommit.trim() === commit) return commit;
  }
  const staging = await mkdtemp(path.join(root, '.source-sync-'));
  const archive = path.join(staging, 'source.tar.gz');
  const extracted = path.join(staging, 'extracted');
  const backup = path.join(staging, 'previous');
  let movedPrevious = false;
  try {
    await mkdir(extracted);
    await download(`https://api.github.com/repos/ggml-org/llama.cpp/tarball/${encodeURIComponent(tag)}`, archive);
    await run('tar', ['-xzf', archive, '-C', extracted]);
    const directories = (await readdir(extracted, { withFileTypes: true })).filter((entry) => entry.isDirectory());
    if (directories.length !== 1) throw new Error('The llama.cpp source archive did not contain one source directory.');
    const staged = path.join(extracted, directories[0].name);
    if (!(await fileExists(path.join(staged, 'CMakeLists.txt')))) throw new Error('The llama.cpp source archive is missing CMakeLists.txt.');
    if (await fileExists(sourceDir)) {
      await rename(sourceDir, backup);
      movedPrevious = true;
    }
    try {
      await rename(staged, sourceDir);
    } catch (error) {
      if (movedPrevious) await rename(backup, sourceDir);
      movedPrevious = false;
      throw error;
    }
    return commit;
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

await mkdir(binariesDir, { recursive: true });
const release = await chooseRelease(matrix);
const tag = String(release.tag_name || release.name || '');
if (!tag) throw new Error('The selected llama.cpp release did not have a tag.');
console.log(`Selected llama.cpp release ${tag} with ${release.assets?.length || 0} assets.`);
const commit = await syncSource(tag);
const tempRoot = path.join(binariesDir, `.sync-${Date.now()}`);
const archiveRoot = path.join(tempRoot, 'archives');
await mkdir(archiveRoot, { recursive: true });

const manifestPath = path.join(binariesDir, 'manifest.json');
let manifest: any = { runtimes: {} };
try {
  manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
} catch {}
manifest.schemaVersion = 2;
manifest.release = tag;
manifest.commit = commit;
manifest.updatedAt = new Date().toISOString();
manifest.runtimes ||= {};

const archiveCache = new Map<string, { path: string; checksum: string }>();

async function acquireAsset(asset: Asset) {
  const cached = archiveCache.get(asset.name);
  if (cached) return cached;
  const target = path.join(archiveRoot, asset.name);
  console.log(`Downloading ${asset.name}`);
  const checksum = await download(asset.browser_download_url, target);
  const value = { path: target, checksum };
  archiveCache.set(asset.name, value);
  return value;
}

try {
  for (const targetSpec of matrix) {
    const primary = selectPrimaryAsset(release, targetSpec);
    if (!primary) throw new Error(`Release ${tag} is missing ${targetSpec.platform}/${targetSpec.arch}/${targetSpec.runtime}.`);
    const target = path.join(binariesDir, targetSpec.platform, targetSpec.arch, targetSpec.runtime);
    const key = `${targetSpec.platform}/${targetSpec.arch}/${targetSpec.runtime}`;
    const installed = manifest.runtimes[key];
    if (installed?.commit === commit && installed?.asset === primary.name && await findServer(target, targetSpec.platform)) {
      console.log(`Already synchronized ${key} (${tag}).`);
      continue;
    }
    const primaryFile = await acquireAsset(primary);
    console.log(`Extracting ${primary.name} -> ${path.relative(projectRoot, target)}`);
    const staged = path.join(tempRoot, 'staged', targetSpec.platform, targetSpec.arch, targetSpec.runtime);
    await extract(primaryFile.path, staged);
    const server = await findServer(staged, targetSpec.platform);
    if (!server) throw new Error(`${primary.name} did not produce llama-server for ${targetSpec.platform}/${targetSpec.arch}/${targetSpec.runtime}.`);
    const companions = companionAssets(release, targetSpec, primary);
    const companionRecords: Array<{ name: string; checksum: string }> = [];
    let companionIndex = 0;
    for (const companion of companions) {
      companionIndex += 1;
      const companionFile = await acquireAsset(companion);
      const companionExtract = path.join(tempRoot, 'companions', `${targetSpec.platform}-${targetSpec.arch}-${targetSpec.runtime}-${companionIndex}`);
      console.log(`Merging ${companion.name} next to ${path.relative(projectRoot, target)}`);
      await extract(companionFile.path, companionExtract);
      await mergeFilesNextToServer(companionExtract, path.dirname(server));
      companionRecords.push({ name: companion.name, checksum: companionFile.checksum });
    }
    await mkdir(path.dirname(target), { recursive: true });
    const previous = path.join(tempRoot, 'previous', targetSpec.platform, targetSpec.arch, targetSpec.runtime);
    let movedPrevious = false;
    if (await fileExists(target)) {
      await mkdir(path.dirname(previous), { recursive: true });
      await rename(target, previous);
      movedPrevious = true;
    }
    try {
      await rename(staged, target);
    } catch (error) {
      if (movedPrevious) await rename(previous, target);
      throw error;
    }
    manifest.runtimes[key] = {
      release: tag,
      commit,
      asset: primary.name,
      assets: [{ name: primary.name, checksum: primaryFile.checksum }, ...companionRecords],
      server: path.relative(staged, server).replaceAll('\\', '/'),
    };
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  }
} finally {
  await rm(tempRoot, { recursive: true, force: true });
}

await writeFile(path.join(sourceDir, 'JYNX_LLAMA_COMMIT'), `${commit}\n`);
await writeFile(path.join(sourceDir, 'JYNX_LLAMA_RELEASE'), `${tag}\n`);
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`llama.cpp ${tag} (${commit}) synchronized with ${matrix.length} runtime target${matrix.length === 1 ? '' : 's'}.`);
