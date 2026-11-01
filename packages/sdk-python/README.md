# Jynx Python SDK

A standard-library HTTP client for compatible model APIs. Requires Python 3.10 or newer.

```sh
python -m pip install jynx-python-sdk
```

```python
from jynx_sdk import JynxClient

client = JynxClient(base_url="http://127.0.0.1:8080/v1")
print(client.list_models())
```

The installed distribution is named `jynx-python-sdk`; Python code imports `jynx_sdk`.

Use an explicit base URL or `connect_local` with an optional host-written loopback discovery file. `stream_chat_completion` yields decoded SSE data. `JynxApiError` exposes the HTTP status and response body. The client does not start a model runtime or implement the stdio agent protocol.

Read the [SDK guide](https://github.com/R3FStudio/Jynx/blob/main/docs/reference/sdks.md) and [API definition](https://github.com/R3FStudio/Jynx/blob/main/api/local-model.openapi.yaml). Report issues in the [source repository](https://github.com/R3FStudio/Jynx/issues).

Licensed under GPL-3.0-only. See LICENSE and COPYRIGHT in this package.

See the [Jynx privacy policy](https://github.com/R3FStudio/Jynx/blob/main/PRIVACY.md), also included as PRIVACY.md in the distribution. SDK requests go to the configured endpoint; applications using the SDK are responsible for their own additional data handling and notices.
