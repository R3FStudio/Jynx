import { JynxProtocolError } from '../errors.js';


export async function* parseSse(body) {
  if (!body) throw new JynxProtocolError('Streaming response has no body');
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let pending = '';
  let done = false;
  try {
    while (!done) {
      const chunk = await reader.read();
      done = chunk.done;
      pending += decoder.decode(chunk.value, { stream: !done });
      pending = pending.replace(/\r\n/g, '\n');
      let boundary;
      while ((boundary = pending.indexOf('\n\n')) >= 0) {
        const frame = pending.slice(0, boundary);
        pending = pending.slice(boundary + 2);
        const data = frame
          .split('\n')
          .filter((line) => line.startsWith('data:'))
          .map((line) => line.slice(5).trimStart())
          .join('\n');
        if (!data) continue;
        if (data === '[DONE]') return;
        try {
          yield JSON.parse(data);
        } catch {
          throw new JynxProtocolError('Streaming response contains invalid JSON');
        }
      }
    }
  } finally {
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
}
