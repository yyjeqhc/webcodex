"""Real stdio + loopback HTTP tests. No API account or network service required."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


ADAPTER = Path(__file__).resolve().parents[1] / "adapter.py"
SECRET = "private-test-token"


class GatewayTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        self.action = lambda handler: self.json_response(handler, {
            "status": "completed", "output": [{"type": "message", "content": [
                {"type": "output_text", "text": "评审结果 🌎"}]}]})
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                owner.requests.append((self.path, self.headers["Authorization"], body))
                try:
                    owner.action(self)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, *_):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.process = None

    def tearDown(self):
        if self.process:
            self.process.stdin.close()
            self.process.wait(timeout=3)
            stderr = self.process.stderr.read()
            self.process.stdout.close()
            self.process.stderr.close()
            self.assertNotIn(SECRET.encode(), stderr)
        self.server.shutdown()
        self.server.server_close()

    @staticmethod
    def json_response(handler, body, status=200):
        raw = json.dumps(body, ensure_ascii=False).encode()
        handler.send_response(status)
        handler.send_header("Content-Type", "application/json")
        handler.send_header("Content-Length", str(len(raw)))
        handler.end_headers()
        handler.wfile.write(raw)

    @staticmethod
    def sse(handler, events):
        raw = b"".join(b"data: " + (event.encode() if isinstance(event, str) else
                       json.dumps(event, ensure_ascii=False).encode()) + b"\r\n\r\n" for event in events)
        handler.send_response(200)
        handler.send_header("Content-Type", "text/event-stream; charset=utf-8")
        handler.send_header("Content-Length", str(len(raw)))
        handler.end_headers()
        # Split UTF-8 and SSE boundaries deliberately across network writes.
        for byte in raw:
            handler.wfile.write(bytes([byte]))
            handler.wfile.flush()

    def start(self, *args):
        self.process = subprocess.Popen([sys.executable, str(ADAPTER), "--base-url",
            "http://127.0.0.1:%d/v1" % self.server.server_port, "--model", "test-model", *args],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={"MODEL_API_KEY": SECRET})
        self.messages = queue.Queue()

        def reader():
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
        threading.Thread(target=reader, daemon=True).start()
        self.send(1, "initialize", {"protocolVersion": 1, "clientCapabilities": {}})
        self.assertEqual(self.get()["result"]["protocolVersion"], 1)
        self.send(2, "session/new", {"cwd": "/unused", "mcpServers": []})
        self.session = self.get()["result"]["sessionId"]

    def send(self, rid, method, params):
        frame = {"jsonrpc": "2.0", "method": method, "params": params}
        if rid is not None:
            frame["id"] = rid
        self.process.stdin.write(json.dumps(frame).encode() + b"\n")
        self.process.stdin.flush()

    def get(self):
        return self.messages.get(timeout=3)

    def prompt(self, blocks=None):
        self.send(3, "session/prompt", {"sessionId": self.session,
                  "prompt": blocks if blocks is not None else [{"type": "text", "text": "完整上下文"}]})

    def terminal(self):
        text = ""
        while True:
            frame = self.get()
            self.assertNotIn(SECRET, json.dumps(frame))
            if "id" in frame:
                return text, frame
            text += frame["params"]["update"]["content"]["text"]

    def test_responses_json_and_single_use(self):
        self.start()
        self.prompt()
        text, frame = self.terminal()
        self.assertEqual(text, "评审结果 🌎")
        self.assertEqual(frame["result"]["stopReason"], "end_turn")
        path, auth, body = self.requests[0]
        self.assertEqual(path, "/v1/responses")
        self.assertEqual(auth, "Bearer " + SECRET)
        self.assertEqual(body["input"][0]["content"][0]["text"], "完整上下文")
        self.assertFalse(body["store"])
        self.assertTrue(body["stream"])
        self.assertNotIn("tools", body)
        self.prompt()
        self.assertEqual(self.get()["error"]["message"], "session_already_used")
        self.assertEqual(len(self.requests), 1)

    def test_responses_sse_multibyte(self):
        self.action = lambda h: self.sse(h, [
            {"type": "response.output_text.delta", "delta": "你好 🌎"},
            {"type": "response.completed", "response": {"status": "completed", "output": []}}])
        self.start()
        self.prompt()
        text, frame = self.terminal()
        self.assertEqual(text, "你好 🌎")
        self.assertEqual(frame["result"]["stopReason"], "end_turn")

    def test_completion_returns_before_transport_eof(self):
        release = threading.Event()
        def open_stream(h):
            h.send_response(200)
            h.send_header("Content-Type", "text/event-stream")
            h.end_headers()
            h.wfile.write(b'data: {"type":"response.output_text.delta","delta":"finished"}\n\ndata: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
            h.wfile.flush()
            release.wait(5)  # EOF is withheld until after the prompt result.
        self.action = open_stream
        self.start("--timeout", "1")
        try:
            self.prompt()
            text, frame = self.terminal()
            self.assertEqual(text, "finished")
            self.assertEqual(frame["result"]["stopReason"], "end_turn")
            self.assertEqual(len(self.requests), 1)
        finally:
            release.set()

    def test_chat_sse(self):
        self.action = lambda h: self.sse(h, [
            {"choices": [{"delta": {"content": "review"}, "finish_reason": None}]},
            {"choices": [{"delta": {}, "finish_reason": "stop"}]}, "[DONE]"])
        self.start("--api", "chat-completions")
        self.prompt()
        text, frame = self.terminal()
        self.assertEqual(text, "review")
        self.assertEqual(frame["result"]["stopReason"], "end_turn")
        self.assertEqual(self.requests[0][0], "/v1/chat/completions")
        self.assertEqual(self.requests[0][2]["messages"][0]["content"], "完整上下文")

    def test_chat_json(self):
        self.action = lambda h: self.json_response(h, {"choices": [
            {"message": {"content": "answer"}, "finish_reason": "stop"}]})
        self.start("--api", "chat-completions")
        self.prompt()
        self.assertEqual(self.terminal()[0], "answer")

    def test_chat_done_without_normal_stop_is_failure(self):
        self.action = lambda h: self.sse(h, [
            {"choices": [{"delta": {"content": "partial"}}]}, "[DONE]"])
        self.start("--api", "chat-completions")
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "incomplete_model_response")

    def test_http_error_never_echoes_body_or_token(self):
        self.action = lambda h: self.json_response(h, {"error": SECRET}, 401)
        self.start()
        self.prompt()
        text, frame = self.terminal()
        self.assertEqual(text, "")
        self.assertEqual(frame["error"]["data"]["kind"], "http_401")
        self.assertEqual(len(self.requests), 1)

    def test_cancel_before_headers(self):
        arrived = threading.Event()
        release = threading.Event()

        def slow(h):
            arrived.set()
            release.wait(2)
            self.json_response(h, {"status": "completed", "output": []})
        self.action = slow
        self.start()
        self.prompt()
        self.assertTrue(arrived.wait(2))
        self.send(None, "session/cancel", {"sessionId": self.session})
        self.assertEqual(self.terminal()[1]["result"]["stopReason"], "cancelled")
        release.set()
        time.sleep(.05)
        self.assertTrue(self.messages.empty())
        self.assertEqual(len(self.requests), 1)

    def test_timeout_is_failure(self):
        self.action = lambda h: time.sleep(.4)
        self.start("--timeout", ".1")
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "network_timeout")

    def test_cancel_during_stream_suppresses_late_updates(self):
        release = threading.Event()

        def slow(h):
            h.send_response(200)
            h.send_header("Content-Type", "text/event-stream")
            h.end_headers()
            h.wfile.write(b'data: {"type":"response.output_text.delta","delta":"first"}\n\n')
            h.wfile.flush()
            release.wait(2)
            h.wfile.write(b'data: {"type":"response.output_text.delta","delta":"late"}\n\n')
            h.wfile.flush()
        self.action = slow
        self.start()
        self.prompt()
        self.assertEqual(self.get()["params"]["update"]["content"]["text"], "first")
        self.send(None, "session/cancel", {"sessionId": self.session})
        self.assertEqual(self.get()["result"]["stopReason"], "cancelled")
        release.set()
        time.sleep(.05)
        self.assertTrue(self.messages.empty())

    def test_output_limit(self):
        self.start("--max-output-bytes", "4")
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "output_limit")

    def test_response_limit(self):
        self.start("--max-response-bytes", "4")
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "response_limit")

    def test_invalid_and_oversize_prompt_no_http(self):
        self.start("--max-prompt-bytes", "4")
        self.prompt([{"type": "image", "data": "abc"}])
        self.assertEqual(self.get()["error"]["message"], "unsupported_content")
        self.prompt()
        self.assertEqual(self.get()["error"]["message"], "prompt_limit")
        self.process.stdin.write(b"{invalid\n")
        self.process.stdin.flush()
        self.assertEqual(self.get()["error"]["code"], -32700)
        self.assertEqual(self.requests, [])

    def test_invalid_unicode_prompt_no_http(self):
        self.start()
        self.prompt([{"type": "text", "text": "\ud800"}])
        self.assertEqual(self.get()["error"]["message"], "invalid_prompt")
        self.send({}, "session/prompt", {})
        self.assertEqual(self.get()["error"]["code"], -32600)
        self.assertEqual(self.requests, [])

    def test_missing_credential_is_typed_failure(self):
        self.start("--api-key-env", "ABSENT_MODEL_API_KEY")
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "credential_unavailable")
        self.assertEqual(self.requests, [])

    def test_incomplete_stream_is_failure(self):
        self.action = lambda h: self.sse(h, [{"type": "response.output_text.delta", "delta": "partial"}])
        self.start()
        self.prompt()
        text, frame = self.terminal()
        self.assertEqual(text, "partial")
        self.assertEqual(frame["error"]["message"], "incomplete_model_response")

    def test_model_tool_is_failure(self):
        self.action = lambda h: self.json_response(h, {"status": "completed", "output": [
            {"type": "function_call", "name": "run_code", "arguments": "{}"}]})
        self.start()
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "unsupported_model_tool")

    def test_empty_result_is_failure(self):
        self.action = lambda h: self.json_response(h, {"status": "completed", "output": []})
        self.start()
        self.prompt()
        self.assertEqual(self.terminal()[1]["error"]["message"], "empty_model_response")


if __name__ == "__main__":
    unittest.main()
