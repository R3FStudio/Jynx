# Models and runtimes

Jynx manages tasks around a model. The model server performs inference. Rebuilding the CLI does not install weights or update that server.

## Existing local server

Use `--endpoint` and `--model` for a server you already run. Defaults are `http://127.0.0.1:8080` and `local-model`; replace the model ID with the one your server expects. Local compatibility is enabled by default.

`--local` applies the request normalization used by llama.cpp-style servers. `--cloud` preserves the ordering expected by cloud-compatible APIs. Choose the mode for the server, even if that server runs on another machine on your network.

## Managed runtime

The CLI can own a llama.cpp process:

```sh
jynx-cli chat --llama-server /path/to/llama-server --gguf /path/to/model.gguf --model local-model --cwd /path/to/project
```

The runtime executable and GGUF must already exist. `--gpu-layers` controls offload; repeat `--llama-arg` for additional runtime arguments. The CLI stops a process it owns when the session closes. It does not own a server you started separately.

`npm run llama:sync` obtains supported runtime bundles. `npm run start:llama -- --model /path/to/model.gguf` starts a selected local runtime. Weights are a separate download.

## Native reasoning

`--reasoning` selects a model-advertised native value or `default`. Names come from the model/runtime rather than a universal Jynx scale. A value supported by one model can be rejected by another. Jynx can negotiate an advertised compatible value when a server reports supported levels. It does not simulate effort by adding a longer-thinking instruction to the prompt.

## Context

`--context` sets the harness budget; its CLI default is 32768 tokens. The loaded server also needs enough context configured. Increasing the harness number alone does not increase the server's allocation.

Context includes instructions, conversation text, tool definitions, and tool results. Automatic compaction uses verified local pressure where counting is supported, or a real context-overflow response. Shortening what is sent to the model does not delete the full tool audit. `/compact` requests the operation explicitly.

For repeated blank responses, check the runtime, template, native effort, and actual context allocation. Jynx retries transient empty steps while retaining tools and completed results. Persistent empty output remains an error; retries cannot make an incompatible model compatible.
