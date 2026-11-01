import { JynxApiError, JynxProtocolError } from '../errors.js';

export function normalizeBaseUrl(value) {
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol)) throw new TypeError('baseUrl must use HTTP or HTTPS');
  if (url.username || url.password || url.search || url.hash)
    throw new TypeError('baseUrl cannot contain credentials, query, or fragment');
  return url.href.replace(/\/$/, '');
}

export async function request(baseUrl, path, { apiKey, fetchImpl, method = 'GET', body, signal } = {}) {
  const headers = { accept: 'application/json' };
  if (apiKey) headers.authorization = `Bearer ${apiKey}`;
  if (body !== undefined) headers['content-type'] = 'application/json';
  const response = await fetchImpl(`${baseUrl}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal,
  });
  if (!response.ok) {
    const raw = await response.text();
    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch {
      parsed = raw;
    }
    throw new JynxApiError(
      parsed?.error?.message || `Jynx returned HTTP ${response.status}`,
      response.status,
      parsed,
    );
  }
  return response;
}

export async function readJson(response) {
  try {
    return await response.json();
  } catch {
    throw new JynxProtocolError('Jynx returned invalid JSON');
  }
}
