"""Review regressions for API completion boundaries; no real model endpoint."""
import importlib.util
import io
from pathlib import Path
import threading
import unittest
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location("review_model_adapter", Path(__file__).resolve().parents[1] / "adapter.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class StreamCompletionTest(unittest.TestCase):
    def setUp(self):
        self.output = io.BytesIO()
        self.adapter = module.Adapter(SimpleNamespace(api="responses", max_output_bytes=1000), self.output)
        self.adapter.session = "session"
        self.task = {"id": 1, "cancel": threading.Event(), "output_bytes": 0, "model_stopped": False}
        self.adapter.active = self.task

    def test_responses_tool_item_is_rejected_before_the_final_snapshot(self):
        # Stream item events carry the item type separately from the event type;
        # the final snapshot can be absent or inconsistent on a compatible API.
        with self.assertRaises(module.Failure) as failure:
            self.adapter.event(self.task, '{"type":"response.output_item.added","item":{"type":"function_call","name":"write_file"}}')
        self.assertEqual(failure.exception.kind, "unsupported_model_tool")
        self.assertEqual(self.output.getvalue(), b"")

    def test_responses_refuses_non_message_text_items_at_completion(self):
        with self.assertRaises(module.Failure):
            self.adapter.event(self.task, '{"type":"response.completed","response":{"status":"completed","output":[{"type":"web_search_call"}]}}')

    def test_response_completed_is_a_semantic_terminal_not_an_eof_requirement(self):
        # A compatible server can keep the HTTP stream open after a terminal
        # event. A fake read raises if the adapter asks for EOF after completion;
        # no scheduler sleeps or network timing are needed to prove the boundary.
        self.exercise_terminal_stream("responses", [
            {"type":"response.output_text.delta", "delta":"complete review"},
            {"type":"response.completed", "response":{"status":"completed", "output":[]}},
        ])

    def test_chat_done_after_stop_does_not_wait_for_transport_eof(self):
        self.exercise_terminal_stream("chat-completions", [
            {"choices":[{"delta":{"content":"complete review"}, "finish_reason":None}]},
            {"choices":[{"delta":{}, "finish_reason":"stop"}]},
            "[DONE]",
        ])

    def exercise_terminal_stream(self, api, events):
        import json
        import os
        import time
        from unittest.mock import patch
        wire = b"".join(b"data: " + (event.encode() if isinstance(event, str) else json.dumps(event).encode()) + b"\n\n" for event in events)
        class Response:
            status = 200
            def __init__(self): self.sent = False
            def getheader(self, name, default=""): return "text/event-stream"
            def read1(self, limit):
                if self.sent: raise TimeoutError("transport stayed open after terminal event")
                self.sent = True
                return wire
        class Connection:
            sock = None
            def __init__(self, *args, **kwargs): self.response = Response()
            def request(self, *args, **kwargs): pass
            def getresponse(self): return self.response
            def close(self): pass
        self.adapter.config = SimpleNamespace(api=api, api_key_env="REVIEW_FIXTURE_TOKEN", base_url="http://127.0.0.1/v1", model="fixture", timeout=1, max_response_bytes=4096, max_output_bytes=1000)
        self.task["deadline"] = time.monotonic() + 1
        with patch.dict(os.environ, {"REVIEW_FIXTURE_TOKEN":"not-a-real-credential"}), patch.object(module.http.client, "HTTPConnection", Connection):
            self.adapter.request(self.task, "review")
        self.assertIn(b"complete review", self.output.getvalue())
