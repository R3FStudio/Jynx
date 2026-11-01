import { discoverLocalApi } from '../../packages/sdk-js/src/discovery/node.js';

const { baseUrl, apiKey } = await discoverLocalApi();
const response = await fetch(`${baseUrl}/models`, { headers: { authorization: `Bearer ${apiKey}` } });
if (!response.ok) {
  const body = await response.text();
  throw new Error(`Local API returned ${response.status}: ${body}`);
}
const result = await response.json();
if (!Array.isArray(result.data)) throw new Error('Local API did not return a model list');
console.log(JSON.stringify({ baseUrl, models: result.data.map((model) => model.id) }, null, 2));
