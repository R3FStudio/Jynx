import { connectLocal } from '../packages/sdk-js/src/discovery/node.js';

const prompt = process.argv.slice(2).join(' ') || 'Hello from Jynx';
const client = await connectLocal();
const [model] = await client.listModels();
if (!model) throw new Error('Load a local model in Jynx first.');

for await (const chunk of client.streamChatCompletion({
  model: model.id,
  messages: [{ role: 'user', content: prompt }],
})) {
  process.stdout.write(chunk.choices?.[0]?.delta?.content || '');
}
process.stdout.write('\n');
