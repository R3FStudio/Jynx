# Examples

Enable **Console → Local API**, load a local model, then run either example from the repository root:

```sh
node examples/local-chat.mjs "Explain this code"
python examples/local_chat.py "Explain this code"
```

The examples discover the loopback endpoint from Jynx's user-data folder. The app publishes that file only while Local API sharing is enabled.

After `npm run build:harness`, run `node examples/harness-diagnostics.mjs` to query the Harness process through the JavaScript SDK. Pass an explicit binary path as the first argument if it is elsewhere.
