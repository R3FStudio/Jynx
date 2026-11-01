# Python SDK

The `jynx_sdk` package provides a standard-library HTTP client for compatible model APIs. Use an explicit base URL or `connect_local` with an optional host-written loopback discovery file.

`stream_chat_completion` yields decoded SSE data. `JynxApiError` exposes the HTTP status and response body. The client does not start a model runtime or implement the stdio agent protocol.

See the [SDK guide](../../docs/reference/sdks.md) and [API definition](../../api/local-model.openapi.yaml). Run `npm run sdk:test:python` at the root to execute the unittest suite.
