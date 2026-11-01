import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { runNodeBin, waitForExit } from '../process-runner.ts';
import { workspaceRoot } from '../workspace-paths.ts';

test('resolves a package tool independently of the process working directory', async () => {
  const outside = await mkdtemp(path.join(tmpdir(), 'jynx-runner-'));
  try {
    const child = await runNodeBin('eslint', 'eslint', ['--version'], {
      cwd: outside,
      resolveFrom: path.join(workspaceRoot, 'tools/tooling'),
    });
    await assert.doesNotReject(waitForExit(child, 'ESLint'));
  } finally {
    await rm(outside, { recursive: true, force: true });
  }
});
