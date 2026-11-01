import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(await readFile(path.join(root, 'packages/sdk-js/package.json'), 'utf8'));
const target = process.env.SDK_REMOVE_VERSION;
const stable = value => typeof value === 'string' && /^\d+\.\d+\.\d+$/.test(value);
if (!stable(target) || !stable(manifest.version)) throw new Error('An exact stable version is required.');
const a = target.split('.').map(Number);
const b = manifest.version.split('.').map(Number);
const difference = a.map((value, index) => value - b[index]).find(value => value !== 0) ?? 0;
if (difference >= 0) throw new Error('Only a version older than the current SDK can be removed.');
const token = process.env.NODE_AUTH_TOKEN;
if (!token) throw new Error('Use the GitHub publishing workflow with package permissions.');
const headers = {Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json'};
const source = new URL(manifest.repository.url.replace(/^git\+/, '').replace(/\.git$/, ''));
if (source.hostname !== 'github.com') throw new Error('Expected a GitHub source repository.');
const repoName = source.pathname.slice(1);
const ownerResponse = await fetch(`https://api.github.com/repos/${repoName}`, {headers});
if (!ownerResponse.ok) throw new Error(`Repository lookup failed: HTTP ${ownerResponse.status}`);
const repo = await ownerResponse.json();
const namespace = repo.owner.type === 'Organization' ? 'orgs' : 'users';
const api = `https://api.github.com/${namespace}/${repo.owner.login}/packages/npm/${encodeURIComponent(manifest.name.split('/')[1])}`;
const packageResponse = await fetch(api, {headers});
if (!packageResponse.ok) throw new Error(`Package lookup failed: HTTP ${packageResponse.status}`);
const metadata = await packageResponse.json();
if (metadata.repository?.full_name?.toLowerCase() !== repoName.toLowerCase()) throw new Error('Package is linked to a different repository.');
const versions = [];
for (let page = 1; page <= 100; page++) {
  const response = await fetch(`${api}/versions?per_page=100&page=${page}`, {headers});
  if (!response.ok) throw new Error(`Version listing failed: HTTP ${response.status}`);
  const items = await response.json();
  versions.push(...items);
  if (items.length < 100) break;
}
if (!versions.some(item => item.name === manifest.version)) throw new Error('The current SDK must be published before removing its predecessor.');
const old = versions.find(item => item.name === target);
if (!old) {
  console.log(`${manifest.name}@${target} is already absent.`);
} else {
  const removal = await fetch(`${api}/versions/${old.id}`, {method: 'DELETE', headers});
  if (removal.status !== 204) throw new Error(`Version removal failed: HTTP ${removal.status}`);
  console.log(`Removed ${manifest.name}@${target}; kept ${manifest.version}.`);
}
