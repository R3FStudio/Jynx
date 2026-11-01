import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp, writeFile, readFile, mkdir, access, rm} from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {privacyText, privacyHtml, syncPrivacy} from '../privacy-assets.mjs';

test('policy rendering preserves external destinations and escapes untrusted markup', () => {
  const text = '# Privacy\n\n## Destinations\n\n[Provider](https://example.com/?a=1&b=2) and `local` <script>alert(1)</script>.\n';
  assert.match(privacyText(text), /Provider \(https:\/\/example.com/);
  const html = privacyHtml(text);
  assert.match(html, /<h2>Destinations<\/h2>/);
  assert.match(html, /href="https:\/\/example.com\/\?a=1&amp;b=2"/);
  assert.match(html, /&lt;script&gt;/);
  assert.ok(!html.includes('<script>'));
});

test('SDK policies stay identical and optional private source is never created by syncing', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-policy-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  const policy = '# Fixture Privacy\n\nData stays local unless a remote feature is selected.\n';
  await writeFile(path.join(root, 'PRIVACY.md'), policy);
  await syncPrivacy(root);
  for (const file of ['packages/sdk-js/PRIVACY.md', 'packages/sdk-python/PRIVACY.md', 'packages/sdk-python/src/jynx_sdk/PRIVACY.md']) assert.equal(await readFile(path.join(root, file), 'utf8'), policy);
  await assert.rejects(access(path.join(root, 'packages/private')));
  await mkdir(path.join(root, 'packages/private/Desktop'), {recursive: true});
  await writeFile(path.join(root, 'packages/private/Desktop/package.json'), '{}');
  await syncPrivacy(root);
  assert.equal(await readFile(path.join(root, 'packages/private/Desktop/PRIVACY.md'), 'utf8'), policy);
  assert.match(await readFile(path.join(root, 'packages/private/Desktop/assets/privacy/PRIVACY.html'), 'utf8'), /Fixture Privacy/);
});
