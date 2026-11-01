import assert from 'node:assert/strict';
import { test } from 'node:test';
import { JynxClient } from '../src/index.js';

test('streams split SSE frames and stops at DONE', async () => {
  const bytes = new TextEncoder();
  const parts = [
    'data: {"choices":[{"delta":{"content":"he"}}]}\r',
    '\n\r\ndata: {"choices":[{"delta":{"content":"llo"}}]}\n\ndata: [DONE]\n\n',
  ];
  const stream = new ReadableStream({
    start(controller) {
      for (const part of parts) controller.enqueue(bytes.encode(part));
      controller.close();
    },
  });
  const client = new JynxClient({
    fetchImpl: async () => new Response(stream, { headers: { 'content-type': 'text/event-stream' } }),
  });
  const chunks = [];
  for await (const chunk of client.streamChatCompletion({
    model: 'local',
    messages: [{ role: 'user', content: 'hello' }],
  }))
    chunks.push(chunk.choices[0].delta.content);
  assert.deepEqual(chunks, ['he', 'llo']);
});
