import { normalizeBaseUrl, readJson, request } from './transport/http.js';
import { parseSse } from './stream/sse.js';

export class JynxClient {
  constructor({ baseUrl = 'http://127.0.0.1:12347/v1', apiKey = '', fetchImpl = globalThis.fetch } = {}) {
    if (typeof fetchImpl !== 'function') throw new TypeError('A fetch implementation is required');
    this.baseUrl = normalizeBaseUrl(baseUrl);
    this.apiKey = apiKey;
    this.fetchImpl = fetchImpl;
  }

  async health(options = {}) {
    const url = new URL(this.baseUrl);
    return readJson(
      await request(url.origin, '/health', { ...options, apiKey: this.apiKey, fetchImpl: this.fetchImpl }),
    );
  }

  async listModels(options = {}) {
    const result = await readJson(
      await request(this.baseUrl, '/models', { ...options, apiKey: this.apiKey, fetchImpl: this.fetchImpl }),
    );
    return result.data;
  }

  async createChatCompletion(input, options = {}) {
    const response = await request(this.baseUrl, '/chat/completions', {
      ...options,
      method: 'POST',
      body: { ...input, stream: false },
      apiKey: this.apiKey,
      fetchImpl: this.fetchImpl,
    });
    return readJson(response);
  }

  async *streamChatCompletion(input, options = {}) {
    const response = await request(this.baseUrl, '/chat/completions', {
      ...options,
      method: 'POST',
      body: { ...input, stream: true },
      apiKey: this.apiKey,
      fetchImpl: this.fetchImpl,
    });
    yield* parseSse(response.body);
  }
}
