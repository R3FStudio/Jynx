"""Find the loopback API endpoint published by the desktop app."""

import json
from pathlib import Path
from urllib.parse import urlsplit

from .client import JynxClient


def discover_local_api(path: str | Path | None = None) -> dict[str, str]:
    current = Path.home() / ".jynx" / "server.json"
    legacy = Path.home() / ".daemonstudio" / "server.json"
    source = Path(path) if path is not None else (current if current.exists() or not legacy.exists() else legacy)
    config = json.loads(source.read_text(encoding="utf-8"))
    if config.get("api") != "openai-compatible" or not isinstance(config.get("apiKey"), str) or not isinstance(config.get("baseUrl"), str):
        raise ValueError("Invalid Jynx discovery file")
    endpoint = urlsplit(config["baseUrl"])
    if endpoint.scheme != "http" or endpoint.hostname != "127.0.0.1" or endpoint.path != "/v1" or endpoint.username or endpoint.password:
        raise ValueError("Discovery endpoint must be the local Jynx API")
    return {"base_url": config["baseUrl"], "api_key": config["apiKey"]}


def connect_local(path: str | Path | None = None) -> JynxClient:
    return JynxClient(**discover_local_api(path))
