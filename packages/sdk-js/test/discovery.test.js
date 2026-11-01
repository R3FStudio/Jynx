import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { discoverLocalApi } from '../src/discovery/node.js';

test('accepts only loopback discovery endpoints', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'jynx-sdk-'));
  const path = join(dir, 'server.json');
  try {
    await writeFile(
      path,
      JSON.stringify({ api: 'openai-compatible', baseUrl: 'http://127.0.0.1:12347/v1', apiKey: 'key' }),
    );
    assert.equal((await discoverLocalApi(path)).apiKey, 'key');
    await writeFile(
      path,
      JSON.stringify({ api: 'openai-compatible', baseUrl: 'https://example.com/v1', apiKey: 'key' }),
    );
    await assert.rejects(discoverLocalApi(path), /local Jynx API/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
