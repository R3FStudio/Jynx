"""Dependency-free client for Jynx's optional local model endpoint."""

import json
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen


class JynxApiError(Exception):
    def __init__(self, message: str, status: int, body: object):
        super().__init__(message)
        self.status = status
        self.body = body


class JynxProtocolError(Exception):
    pass


class JynxClient:
    def __init__(self, base_url: str = "http://127.0.0.1:12347/v1", api_key: str = "", opener=urlopen):
        parsed = urlsplit(base_url)
        if parsed.scheme not in ("http", "https") or not parsed.netloc or parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ValueError("base_url must be an HTTP URL without credentials, query, or fragment")
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self._opener = opener

    def _request(self, path: str, payload: dict | None = None, *, stream: bool = False):
        headers = {"Accept": "text/event-stream" if stream else "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        if payload is not None:
            headers["Content-Type"] = "application/json"
        request = Request(
            self.base_url + path,
            data=None if payload is None else json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST" if payload is not None else "GET",
        )
        return self._open(request)

    def _open(self, request: Request):
        try:
            return self._opener(request)
        except HTTPError as error:
            raw = error.read().decode("utf-8", errors="replace")
            try:
                body = json.loads(raw)
            except json.JSONDecodeError:
                body = raw
            details = body.get("error") if isinstance(body, dict) else None
            message = details.get("message") if isinstance(details, dict) else None
            raise JynxApiError(message or f"Jynx returned HTTP {error.code}", error.code, body) from error

    @staticmethod
    def _json(response):
        with response:
            try:
                return json.load(response)
            except (ValueError, UnicodeError) as error:
                raise JynxProtocolError("Jynx returned invalid JSON") from error

    def health(self):
        origin = urlsplit(self.base_url)
        return self._json(self._request_origin(f"{origin.scheme}://{origin.netloc}/health"))

    def _request_origin(self, url: str):
        headers = {"Accept": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return self._open(Request(url, headers=headers))

    def list_models(self) -> list[dict]:
        return self._json(self._request("/models"))["data"]

    def create_chat_completion(self, *, model: str, messages: list[dict], **options) -> dict:
        return self._json(self._request("/chat/completions", {"model": model, "messages": messages, **options, "stream": False}))

    def stream_chat_completion(self, *, model: str, messages: list[dict], **options):
        """Yield decoded OpenAI-compatible SSE chunks until [DONE]."""
        response = self._request("/chat/completions", {"model": model, "messages": messages, **options, "stream": True}, stream=True)
        with response:
            data_lines = []
            for raw in response:
                line = raw.decode("utf-8").rstrip("\r\n")
                if line.startswith("data:"):
                    data_lines.append(line[5:].lstrip())
                elif not line and data_lines:
                    data = "\n".join(data_lines)
                    data_lines.clear()
                    if data == "[DONE]":
                        return
                    try:
                        yield json.loads(data)
                    except json.JSONDecodeError as error:
                        raise JynxProtocolError("Streaming response contains invalid JSON") from error
