# Jynx JavaScript SDK

JavaScript and TypeScript clients for Jynx model APIs and the Jynx Harness. Requires Node.js 22.12 or newer and has no runtime package dependencies.

```sh
npm install @r3fstudios/jynx-sdk
```

```js
import { JynxClient } from '@r3fstudios/jynx-sdk';

const client = new JynxClient({ baseUrl: 'http://127.0.0.1:8080/v1' });
console.log(await client.listModels());
```

Supply a server URL and optional API key to connect to a compatible model API. The SDK does not start inference. Import `connectLocal` from `@r3fstudios/jynx-sdk/discovery` when a compatible host has written a loopback discovery file.

For agent tools and approvals, import `HarnessClient` from `@r3fstudios/jynx-sdk/harness` and supply the path to a separately installed `jynx-harness` executable. This Node-only client multiplexes JSON-line requests and agent events and supports context, threads, runtime state, and MCP configuration. It does not convert an HTTP endpoint into a harness process.

Read the [SDK guide](https://github.com/R3FStudio/Jynx/blob/main/docs/reference/sdks.md), [harness protocol](https://github.com/R3FStudio/Jynx/blob/main/docs/reference/protocol.md), and [API definition](https://github.com/R3FStudio/Jynx/blob/main/api/local-model.openapi.yaml). Report issues in the [source repository](https://github.com/R3FStudio/Jynx/issues).

Licensed under GPL-3.0-only. See LICENSE and COPYRIGHT in this package.
