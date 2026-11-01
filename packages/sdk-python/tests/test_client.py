import io
import json
import sys
import unittest
from pathlib import Path
from urllib.error import HTTPError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from jynx_sdk import JynxApiError, JynxClient


class TestClient(unittest.TestCase):
    def test_completion_auth_and_payload(self):
        seen = []

        def open_request(request):
            seen.append(request)
            return io.BytesIO(json.dumps({"choices": [{"message": {"content": "hello"}}]}).encode())

        client = JynxClient(api_key="secret", opener=open_request)
        result = client.create_chat_completion(model="local", messages=[{"role": "user", "content": "hi"}])
        self.assertEqual(result["choices"][0]["message"]["content"], "hello")
        self.assertEqual(seen[0].get_header("Authorization"), "Bearer secret")
        self.assertFalse(json.loads(seen[0].data)["stream"])

    def test_stream(self):
        response = io.BytesIO(b'data: {"choices":[{"delta":{"content":"hello"}}]}\n\ndata: [DONE]\n\n')
        client = JynxClient(opener=lambda request: response)
        chunks = list(client.stream_chat_completion(model="local", messages=[]))
        self.assertEqual(chunks[0]["choices"][0]["delta"]["content"], "hello")

    def test_health_reports_http_error(self):
        def unavailable(request):
            raise HTTPError(request.full_url, 503, "unavailable", {}, io.BytesIO(b'{"error":{"message":"No local model is loaded."}}'))

        client = JynxClient(opener=unavailable)
        with self.assertRaises(JynxApiError) as caught:
            client.health()
        self.assertEqual(caught.exception.status, 503)
        self.assertEqual(str(caught.exception), "No local model is loaded.")


if __name__ == "__main__":
    unittest.main()
