# JavaScript SDK

`@jynx/sdk` provides an HTTP client for a compatible model API, optional loopback discovery, and a Node-only subprocess client for Jynx Harness.

The HTTP client uses standard `fetch` and has no runtime package dependencies. Supply a server URL and optional key, or use `connectLocal` when a compatible host has written `~/.jynx/server.json`. Discovery accepts only loopback endpoints.

`@jynx/sdk/harness` controls a built harness process, multiplexes JSON-line requests and agent events, and supports approvals, context, threads, runtime state, and MCP configuration. It does not turn an HTTP endpoint into a harness process.

See the [SDK guide](../../docs/reference/sdks.md), [protocol reference](../../docs/reference/protocol.md), and [API definition](../../api/local-model.openapi.yaml).

Run this package's tests with `npm test` from this directory, or `npm run sdk:test:js` at the root.
