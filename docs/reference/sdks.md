# SDKs

JavaScript and Python HTTP clients need a compatible model API and its base URL. Importing a client does not start inference. The JavaScript Node-only subprocess client controls a built harness over JSON lines; use it for tools, approvals, context, state, and MCP.

## JavaScript HTTP

```js
import { JynxClient } from '@jynx/sdk';
const client = new JynxClient({
  baseUrl: 'http://127.0.0.1:8080/v1',
  apiKey: process.env.JYNX_MODEL_API_KEY,
});
const answer = await client.createChatCompletion({
  model: 'your-model',
  messages: [{ role: 'user', content: 'Hello' }],
});
console.log(answer.choices[0].message.content);
```

`streamChatCompletion` yields decoded streaming chunks. `JynxApiError` exposes status and response information. Optional discovery reads a host-written `~/.jynx/server.json` file and accepts only loopback endpoints.

## JavaScript harness subprocess

```js
import { HarnessClient } from '@jynx/sdk/harness';
const harness = new HarnessClient({ binaryPath: '/path/to/jynx-harness' });
try {
  console.log(await harness.diagnostics());
} finally {
  harness.close();
}
```

`runAgent` uses the shared request shape. An approval event must be shown to the user and resolved with `respondToApproval`; receiving it is not permission. Close the client when its subprocess is no longer needed.

## Python HTTP

```python
from jynx_sdk import JynxClient
client = JynxClient(base_url="http://127.0.0.1:8080/v1")
print(client.list_models())
```

The Python client uses the standard library. `connect_local` reads optional discovery data. `stream_chat_completion` yields SSE data; `JynxApiError` exposes status and body.

Keep request fields, errors, streaming termination, and cancellation aligned across implementations. The HTTP definition is `api/local-model.openapi.yaml`. HTTP and stdio are different interfaces; do not send JSON-RPC methods to a chat-completions route.
