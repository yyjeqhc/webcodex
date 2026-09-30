#!/usr/bin/env python3
"""Bounded, text-only ACP v1 bridge to an operator-configured model API."""
import argparse
import http.client
import ipaddress
import json
import os
import socket
import sys
import threading
import time
import urllib.parse
import uuid


class Failure(Exception):
    def __init__(self, kind):
        self.kind = kind


def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


class Adapter:
    def __init__(self, config, output):
        self.config = config
        self.output = output
        self.lock = threading.RLock()
        self.initialized = False
        self.session = None
        self.used = False
        self.active = None

    def send(self, value):
        with self.lock:
            self.output.write(encode(dict(jsonrpc="2.0", **value)) + b"\n")
            self.output.flush()

    def error(self, rid, kind, code=-32000):
        self.send({"id": rid, "error": {"code": code, "message": kind,
                                           "data": {"kind": kind}}})

    def finish(self, task, reason=None, failure=None):
        with self.lock:
            if self.active is not task:
                return
            self.active = None
            task["cancel"].set()
            connection = task.get("connection")
            sock = connection.sock if connection else None
            if sock:
                try:
                    sock.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            if failure:
                self.error(task["id"], failure)
            else:
                self.send({"id": task["id"], "result": {"stopReason": reason}})

    def handle(self, frame):
        if not isinstance(frame, dict):
            self.error(None, "invalid_request", -32600)
            return
        rid = frame.get("id")
        method = frame.get("method")
        params = frame.get("params", {})
        if (frame.get("jsonrpc") != "2.0" or not isinstance(method, str)
                or not isinstance(params, dict)
                or not (rid is None or type(rid) is int or isinstance(rid, str) and len(rid) <= 128)):
            self.error(None, "invalid_request", -32600)
            return
        if method == "session/cancel":
            if params.get("sessionId") == self.session and self.active:
                self.finish(self.active, "cancelled")
            return
        if rid is None:
            return
        try:
            if method == "initialize":
                if params.get("protocolVersion") != 1 or self.initialized:
                    raise Failure("invalid_initialize")
                self.initialized = True
                self.send({"id": rid, "result": {"protocolVersion": 1,
                           "agentCapabilities": {"loadSession": False,
                           "promptCapabilities": {}}, "authMethods": [],
                           "agentInfo": {"name": "webcodex-model-gateway", "version": "1"}}})
            elif method == "session/new":
                if (not self.initialized or self.session is not None
                        or not isinstance(params.get("cwd"), str)
                        or params.get("mcpServers") != []):
                    raise Failure("invalid_session")
                self.session = str(uuid.uuid4())
                self.send({"id": rid, "result": {"sessionId": self.session}})
            elif method == "session/prompt":
                if self.session is None or params.get("sessionId") != self.session:
                    raise Failure("invalid_session")
                if self.used:
                    raise Failure("session_already_used")
                blocks = params.get("prompt")
                if not isinstance(blocks, list) or not blocks:
                    raise Failure("invalid_prompt")
                texts = []
                for block in blocks:
                    if not isinstance(block, dict):
                        raise Failure("unsupported_content")
                    if block.get("type") == "text" and isinstance(block.get("text"), str):
                        texts.append(block["text"])
                    elif block.get("type") == "resource_link" and isinstance(block.get("uri"), str):
                        # Baseline ACP resource links are passed as references, never fetched.
                        texts.append("Resource reference: " + block["uri"])
                    else:
                        raise Failure("unsupported_content")
                prompt = "\n\n".join(texts)
                try:
                    prompt_size = len(prompt.encode("utf-8"))
                except UnicodeError:
                    raise Failure("invalid_prompt") from None
                if not prompt.strip() or prompt_size > self.config.max_prompt_bytes:
                    raise Failure("prompt_limit")
                self.used = True
                task = {"id": rid, "cancel": threading.Event(),
                        "deadline": time.monotonic() + self.config.timeout}
                self.active = task
                threading.Thread(target=self.run_prompt, args=(task, prompt), daemon=True).start()
                threading.Thread(target=self.watch_deadline, args=(task,), daemon=True).start()
            else:
                self.error(rid, "method_not_found", -32601)
        except Failure as exc:
            self.error(rid, exc.kind, -32602)

    def watch_deadline(self, task):
        if not task["cancel"].wait(max(0, task["deadline"] - time.monotonic())):
            self.finish(task, failure="network_timeout")

    def run_prompt(self, task, prompt):
        try:
            self.request(task, prompt)
            self.finish(task, "end_turn")
        except Failure as exc:
            self.finish(task, failure=exc.kind)
        except (TimeoutError, socket.timeout):
            self.finish(task, failure="network_timeout")
        except Exception:
            # Exceptions, response bodies and URLs may contain credentials.
            self.finish(task, failure="network_or_protocol_error")

    def chunk(self, task, text):
        if not isinstance(text, str):
            raise Failure("invalid_model_response")
        with self.lock:
            if self.active is not task:
                raise Failure("cancelled")
            task["output_bytes"] += len(text.encode("utf-8"))
            if task["output_bytes"] > self.config.max_output_bytes:
                raise Failure("output_limit")
            for offset in range(0, len(text), 2048):
                self.send({"method": "session/update", "params": {"sessionId": self.session,
                           "update": {"sessionUpdate": "agent_message_chunk",
                                      "content": {"type": "text", "text": text[offset:offset + 2048]}}}})

    def request(self, task, prompt):
        config = self.config
        token = os.environ.get(config.api_key_env)
        if not token or "\r" in token or "\n" in token:
            raise Failure("credential_unavailable")
        url = urllib.parse.urlsplit(config.base_url)
        conn_class = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
        conn = conn_class(url.hostname, url.port, timeout=config.timeout)
        task["connection"] = conn
        task["output_bytes"] = 0
        task["model_stopped"] = False
        if config.api == "responses":
            payload = {"model": config.model, "stream": True, "store": False,
                       "input": [{"role": "user", "content": [{"type": "input_text", "text": prompt}]}]}
            endpoint = "responses"
        else:
            payload = {"model": config.model, "stream": True,
                       "messages": [{"role": "user", "content": prompt}]}
            endpoint = "chat/completions"
        try:
            if task["cancel"].is_set():
                raise Failure("cancelled")
            conn.request("POST", url.path.rstrip("/") + "/" + endpoint, encode(payload),
                         {"Authorization": "Bearer " + token, "Content-Type": "application/json",
                          "Accept": "text/event-stream, application/json"})
            response = conn.getresponse()
            if response.status != 200:
                raise Failure("http_" + str(response.status))
            is_sse = response.getheader("Content-Type", "").split(";", 1)[0].strip() == "text/event-stream"
            total = 0
            pending = b""
            data = []
            while True:
                remaining = task["deadline"] - time.monotonic()
                if task["cancel"].is_set():
                    raise Failure("cancelled")
                if remaining <= 0:
                    raise Failure("network_timeout")
                if conn.sock is not None:
                    conn.sock.settimeout(remaining)
                raw = response.read1(4096)
                if not raw:
                    break
                total += len(raw)
                if total > config.max_response_bytes:
                    raise Failure("response_limit")
                pending += raw
                if is_sse:
                    while b"\n" in pending:
                        line, pending = pending.split(b"\n", 1)
                        line = line.rstrip(b"\r")
                        if not line:
                            if data:
                                event = b"\n".join(data).decode("utf-8")
                                data = []
                                if self.event(task, event):
                                    # API completion is the boundary, not HTTP EOF:
                                    # a proxy may keep the transport alive afterwards.
                                    # Ignore trailing transport bytes, close once in
                                    # finally, and never retry a completed request.
                                    if not task["output_bytes"]:
                                        raise Failure("empty_model_response")
                                    return
                        elif line.startswith(b"data:"):
                            data.append(line[5:].lstrip(b" "))
            if is_sse:
                raise Failure("incomplete_model_response")
            else:
                self.document(task, json.loads(pending))
            if not task["output_bytes"]:
                raise Failure("empty_model_response")
        finally:
            conn.close()

    def event(self, task, data):
        if data == "[DONE]":
            if self.config.api == "responses" or not task["model_stopped"]:
                raise Failure("incomplete_model_response")
            return True
        value = json.loads(data)
        if self.config.api == "responses":
            kind = value.get("type")
            if kind == "response.output_text.delta":
                self.chunk(task, value.get("delta"))
            elif kind in ("response.output_item.added", "response.output_item.done"):
                self.reject_tools([value.get("item")])
            elif kind == "response.completed":
                if value.get("response", {}).get("status") != "completed":
                    raise Failure("incomplete_model_response")
                self.reject_tools(value.get("response", {}).get("output", []))
                return True
            elif kind in ("error", "response.failed", "response.incomplete"):
                raise Failure("model_failed")
            elif kind and ("function_call" in kind or "tool_call" in kind):
                raise Failure("unsupported_model_tool")
        else:
            choices = value.get("choices", [])
            if len(choices) > 1:
                raise Failure("invalid_model_response")
            for choice in choices:
                delta = choice.get("delta", {})
                if delta.get("tool_calls") or delta.get("function_call"):
                    raise Failure("unsupported_model_tool")
                if delta.get("content") is not None:
                    self.chunk(task, delta["content"])
                if choice.get("finish_reason") not in (None, "stop"):
                    raise Failure("incomplete_model_response")
                if choice.get("finish_reason") == "stop":
                    task["model_stopped"] = True
            if "error" in value:
                raise Failure("model_failed")
        return False

    @staticmethod
    def reject_tools(output):
        if not isinstance(output, list) or any(not isinstance(item, dict) or item.get("type") not in ("message", "reasoning") for item in output):
            raise Failure("unsupported_model_tool")

    def document(self, task, value):
        if self.config.api == "responses":
            if value.get("status") != "completed":
                raise Failure("incomplete_model_response")
            output = value.get("output", [])
            self.reject_tools(output)
            for item in output:
                for content in item.get("content", []):
                    if content.get("type") == "output_text":
                        self.chunk(task, content.get("text"))
        else:
            choices = value.get("choices", [])
            if len(choices) != 1 or choices[0].get("finish_reason") != "stop":
                raise Failure("incomplete_model_response")
            message = choices[0].get("message", {})
            if message.get("tool_calls") or message.get("function_call"):
                raise Failure("unsupported_model_tool")
            self.chunk(task, message.get("content"))


def configuration():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True, help="API root, e.g. https://api.openai.com/v1")
    parser.add_argument("--model", required=True)
    parser.add_argument("--api", choices=("responses", "chat-completions"), default="responses")
    parser.add_argument("--api-key-env", default="MODEL_API_KEY")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--max-prompt-bytes", type=int, default=262144)
    parser.add_argument("--max-response-bytes", type=int, default=2097152)
    parser.add_argument("--max-output-bytes", type=int, default=262144)
    args = parser.parse_args()
    valid_url = False
    try:
        url = urllib.parse.urlsplit(args.base_url)
        try:
            local = url.hostname == "localhost" or ipaddress.ip_address(url.hostname).is_loopback
        except ValueError:
            local = False
        valid_url = (url.hostname and not url.username and not url.password and not url.query
                     and not url.fragment and (url.scheme == "https" or url.scheme == "http" and local))
        valid_url = valid_url and url.port != 0
    except ValueError:
        valid_url = False
    if (not valid_url or not args.model or len(args.model) > 256
            or not 0 < args.timeout <= 600
            or any(not 0 < size <= 16777216 for size in
                   (args.max_prompt_bytes, args.max_response_bytes, args.max_output_bytes))):
        parser.exit(2, "invalid gateway configuration\n")
    return args


def main():
    config = configuration()
    adapter = Adapter(config, sys.stdout.buffer)
    limit = config.max_prompt_bytes + 65536
    while True:
        line = sys.stdin.buffer.readline(limit + 1)
        if not line:
            return
        if len(line) > limit:
            adapter.error(None, "frame_limit", -32600)
            return
        try:
            frame = json.loads(line)
        except (ValueError, UnicodeError):
            adapter.error(None, "parse_error", -32700)
            continue
        try:
            adapter.handle(frame)
        except Exception:
            adapter.error(None, "invalid_request", -32600)


if __name__ == "__main__":
    main()
