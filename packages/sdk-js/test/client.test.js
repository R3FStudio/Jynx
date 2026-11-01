import assert from 'node:assert/strict';
import { test } from 'node:test';
import { JynxApiError, JynxClient } from '../src/index.js';

test('lists models and authenticates chat completions', async () => {
  const calls = [];
  const client = new JynxClient({
    baseUrl: 'http://127.0.0.1:12347/v1/',
    apiKey: 'secret',
    fetchImpl: async (url, init) => {
      calls.push({ url, init });
      return Response.json(
        url.endsWith('/models') ? { data: [{ id: 'local' }] } : { choices: [{ message: { content: 'hi' } }] },
      );
    },
  });
  assert.deepEqual(await client.listModels(), [{ id: 'local' }]);
  assert.equal(
    (await client.createChatCompletion({ model: 'local', messages: [{ role: 'user', content: 'hello' }] })).choices[0]
      .message.content,
    'hi',
  );
  assert.equal(calls[0].init.headers.authorization, 'Bearer secret');
  assert.equal(JSON.parse(calls[1].init.body).stream, false);
});

test('reports API errors with status and message', async () => {
  const client = new JynxClient({
    fetchImpl: async () => Response.json({ error: { message: 'No local model is loaded.' } }, { status: 503 }),
  });
  await assert.rejects(
    client.listModels(),
    (error) => error instanceof JynxApiError && error.status === 503 && error.message === 'No local model is loaded.',
  );
});
