import path from 'node:path';
import process from 'node:process';
import {cp, mkdir, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const source = path.join(root, 'packages/sdk-js');
const manifest = JSON.parse(await readFile(path.join(source, 'package.json'), 'utf8'));
const repository = new URL(manifest.repository.url.replace(/^git\+/, '').replace(/\.git$/, ''));
const [owner] = repository.pathname.slice(1).split('/');
if (repository.hostname !== 'github.com' || !manifest.name.startsWith(`@${owner.toLowerCase()}/`)) throw new Error('The SDK scope must match its GitHub repository owner.');
if (!process.env.npm_execpath) throw new Error('Run npm run publish:sdk:github.');
async function verifyPublishedPackage() {
  const headers = {Authorization: `Bearer ${process.env.NODE_AUTH_TOKEN}`, Accept: 'application/vnd.github+json'};
  const repoName = repository.pathname.slice(1);
  const repoResponse = await fetch(`https://api.github.com/repos/${repoName}`, {headers, signal: AbortSignal.timeout(30000)});
  if (!repoResponse.ok) throw new Error(`Repository verification failed: HTTP ${repoResponse.status}`);
  const repo = await repoResponse.json();
  const namespace = repo.owner.type === 'Organization' ? 'orgs' : 'users';
  const packageName = encodeURIComponent(manifest.name.split('/')[1]);
  const response = await fetch(`https://api.github.com/${namespace}/${owner}/packages/npm/${packageName}`, {headers, signal: AbortSignal.timeout(30000)});
  if (!response.ok) throw new Error(`Published package verification failed: HTTP ${response.status}`);
  const published = await response.json();
  if (published.repository?.full_name?.toLowerCase() !== repoName.toLowerCase()) throw new Error('The published SDK is not linked to its source repository.');
  console.log(JSON.stringify({package: published.name, url: published.html_url, repository: published.repository.full_name, visibility: published.visibility}));
}
const dryRun = process.argv.includes('--dry-run');
if (process.argv.slice(2).some(arg => arg !== '--dry-run')) throw new Error('Only --dry-run is supported.');
if (!dryRun) {
  const token = process.env.NODE_AUTH_TOKEN;
  if (!token) throw new Error('GitHub Packages requires NODE_AUTH_TOKEN; use the publishing workflow.');
  const response = await fetch(`https://npm.pkg.github.com/${manifest.name.replace('/', '%2f')}`, {
    headers: {Authorization: `Bearer ${token}`, Accept: 'application/json'},
    signal: AbortSignal.timeout(30000),
  });
  if (response.ok) {
    const metadata = await response.json();
    if (metadata.versions?.[manifest.version]) {
      console.log(`${manifest.name}@${manifest.version} is already on GitHub Packages.`);
      await verifyPublishedPackage();
      process.exit(0);
    }
  } else if (response.status !== 404) {
    throw new Error(`GitHub Packages version check failed: HTTP ${response.status}`);
  }
}
const cache = path.join(root, '.cache');
await mkdir(cache, {recursive: true});
const stage = await mkdtemp(path.join(cache, 'github-sdk-'));
try {
  for (const entry of manifest.files) {
    const absolute = path.resolve(source, entry);
    const relative = path.relative(source, absolute);
    if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('SDK package files must stay inside the SDK.');
    await cp(absolute, path.join(stage, relative), {recursive: true});
  }
  manifest.publishConfig = {...manifest.publishConfig, registry: 'https://npm.pkg.github.com/'};
  await writeFile(path.join(stage, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');
  const args = [process.env.npm_execpath, 'publish', stage, '--access', 'public', '--registry=https://npm.pkg.github.com/', '--ignore-scripts', '--workspaces=false'];
  if (dryRun) args.push('--dry-run');
  const result = spawnSync(process.execPath, args, {cwd: root, env: process.env, stdio: 'inherit'});
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`GitHub Packages publish failed (${result.status}).`);
  if (!dryRun) await verifyPublishedPackage();
} finally {
  const relative = path.relative(cache, stage);
  if (!relative.startsWith('github-sdk-') || relative.includes(path.sep) || path.isAbsolute(relative)) throw new Error('Refusing to clean an unrelated staging folder.');
  await rm(stage, {recursive: true, force: true});
}
