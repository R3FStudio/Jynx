"""Run a prompt through the loaded local model: python examples/local_chat.py Hello."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "packages" / "sdk-python" / "src"))
from jynx_sdk import connect_local

client = connect_local()
models = client.list_models()
if not models:
    raise SystemExit("Load a local model in Jynx first.")
prompt = " ".join(sys.argv[1:]) or "Hello from Jynx"
for chunk in client.stream_chat_completion(model=models[0]["id"], messages=[{"role": "user", "content": prompt}]):
    print(chunk.get("choices", [{}])[0].get("delta", {}).get("content", ""), end="", flush=True)
print()
