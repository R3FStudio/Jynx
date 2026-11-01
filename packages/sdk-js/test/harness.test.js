import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { test } from 'node:test';
import { HarnessClient, HarnessRpcError } from '../src/harness/client.js';

function fakeHarness() {
  const child = new EventEmitter();
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.requests = [];
  child.kill = () => {
    child.emit('exit', 0);
    return true;
  };
  let input = '';
  child.stdin.on('data', (bytes) => {
    input += bytes.toString();
    while (input.includes('\n')) {
      const end = input.indexOf('\n');
      const request = JSON.parse(input.slice(0, end));
      child.requests.push(request);
      input = input.slice(end + 1);
      if (request.method === 'agent/run')
        child.stdout.write(
          `${JSON.stringify({ method: 'agent/event', params: { request_id: request.params.request_id, event: { type: 'started' } } })}\n`,
        );
      child.stdout.write(`${JSON.stringify({ id: request.id, result: { ok: true } })}\n`);
    }
  });
  return child;
}

test('multiplexes Harness responses and agent events', async () => {
  const child = fakeHarness();
  const client = new HarnessClient({ binaryPath: 'jynx-harness', spawnImpl: () => child });
  const events = [];
  assert.deepEqual(await client.diagnostics(), { ok: true });
  assert.deepEqual(await client.runAgent({ cwd: '.', messages: [] }, (event) => events.push(event)), { ok: true });
  assert.deepEqual(events, [{ type: 'started' }]);
  client.close();
});

test('turns RPC failures into typed errors', async () => {
  const child = fakeHarness();
  child.stdin.removeAllListeners('data');
  child.stdin.on('data', (bytes) => {
    const request = JSON.parse(bytes.toString());
    child.stdout.write(`${JSON.stringify({ id: request.id, error: { code: -32000, message: 'unknown method' } })}\n`);
  });
  const client = new HarnessClient({ binaryPath: 'jynx-harness', spawnImpl: () => child });
  await assert.rejects(client.call('missing'), (error) => error instanceof HarnessRpcError && error.code === -32000);
  client.close();
});

test('maps thread, runtime, and MCP operations to the Harness protocol', async () => {
  const child = fakeHarness();
  const client = new HarnessClient({ binaryPath: 'jynx-harness', spawnImpl: () => child });
  await client.listThreads({ archived: true, searchTerm: 'draft' });
  await client.saveThread({ id: 'thread-1', name: 'Draft' });
  await client.readThread('thread-1');
  await client.resumeThread('thread-1');
  await client.listThreadTurns('thread-1');
  await client.archiveThread('thread-1');
  await client.unarchiveThread('thread-1');
  await client.renameThread('thread-1', 'Final');
  await client.forkThread('thread-1');
  await client.deleteThread('thread-1');
  await client.getRuntimeState();
  await client.updateRuntimeState({ model: 'local' });
  await client.clearRuntimeState();
  await client.getMcpConfig();
  await client.addMcpServer('files', 'files-server', ['--readonly']);
  assert.deepEqual(child.requests.map(({ method, params }) => [method, params]), [
    ['thread/list', { archived: true, searchTerm: 'draft' }],
    ['thread/save', { thread: { id: 'thread-1', name: 'Draft' } }],
    ['thread/read', { threadId: 'thread-1' }],
    ['thread/resume', { threadId: 'thread-1' }],
    ['thread/turns/list', { threadId: 'thread-1' }],
    ['thread/archive', { threadId: 'thread-1' }],
    ['thread/unarchive', { threadId: 'thread-1' }],
    ['thread/name/set', { threadId: 'thread-1', name: 'Final' }],
    ['thread/fork', { threadId: 'thread-1' }],
    ['thread/delete', { threadId: 'thread-1' }],
    ['runtime/state/get', {}],
    ['runtime/state/update', { patch: { model: 'local' } }],
    ['runtime/state/clear', {}],
    ['mcp/config', {}],
    ['mcp/config/add', { name: 'files', command: 'files-server', args: ['--readonly'] }],
  ]);
  client.close();
});
