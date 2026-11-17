import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, copyFile, chmod, writeFile, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { executablePath } from './process-runner.ts';

test('Linux native tools masked by NTFS permissions run with companion files from cache', { skip: process.platform !== 'linux' }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'jynx-executable-test-'));
  const cachedDirectories = new Set();
  try {
    const source = path.join(directory, 'tool');
    await copyFile('/bin/echo', source);
    await chmod(source, 0o600);
    await writeFile(path.join(directory, 'companion.txt'), 'compiler library');
    const cached = await executablePath(source);
    cachedDirectories.add(path.dirname(cached));
    assert.notEqual(cached, source);
    assert.equal(execFileSync(cached, ['works'], { encoding: 'utf8' }).trim(), 'works');
    assert.equal(await readFile(path.join(path.dirname(cached), 'companion.txt'), 'utf8'), 'compiler library');
    assert.equal(await executablePath(source), cached);
    const single = await executablePath(source, source);
    cachedDirectories.add(path.dirname(single));
    assert.notEqual(single, cached);
    assert.equal(execFileSync(single, ['backend'], { encoding: 'utf8' }).trim(), 'backend');
    await chmod(source, 0o700);
    assert.equal(await executablePath(source), source);
  } finally {
    await rm(directory, { recursive: true, force: true });
    for (const cached of cachedDirectories) await rm(cached, { recursive: true, force: true });
  }
});
