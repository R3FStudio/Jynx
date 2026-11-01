import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { JynxClient } from '../client.js';

export function discoveryPath(home = homedir()) {
  const current = join(home, '.jynx', 'server.json');
  const legacy = join(home, '.daemonstudio', 'server.json');
  return existsSync(current) || !existsSync(legacy) ? current : legacy;
}

export async function discoverLocalApi(path = discoveryPath()) {
  const config = JSON.parse(await readFile(path, 'utf8'));
  if (config.api !== 'openai-compatible' || typeof config.apiKey !== 'string' || typeof config.baseUrl !== 'string') {
    throw new TypeError('Invalid Jynx discovery file');
  }
  const url = new URL(config.baseUrl);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || url.pathname !== '/v1') {
    throw new TypeError('Discovery endpoint must be the local Jynx API');
  }
  return { baseUrl: config.baseUrl, apiKey: config.apiKey };
}

export async function connectLocal(options = {}) {
  const connection = await discoverLocalApi(options.path);
  return new JynxClient({ ...connection, fetchImpl: options.fetchImpl });
}
