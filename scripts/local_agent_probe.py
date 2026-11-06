"""Loopback-only client tool inventory: canned protocol responses, no real model.

Only tool names are retained from requests; request content and headers are not
logged. Both client variants run with the same command builder as acceptance.
"""
import json
import os
import re
import signal
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def run_captured(command, *, cwd, env, prompt, stdout, stderr, timeout):
    with stdout.open("wb") as output, stderr.open("wb") as errors:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
                                   stdout=output, stderr=errors, start_new_session=True)
        start = time.monotonic()
        try:
            process.stdin.write(prompt.encode())
            process.stdin.close()
            while process.poll() is None:
                if time.monotonic() - start > timeout:
                    raise AssertionError("client deadline exceeded; inspect private diagnostics")
                if stdout.stat().st_size > 8 * 1024 * 1024 or stderr.stat().st_size > 2 * 1024 * 1024:
                    raise AssertionError("client output exceeded capture bound")
                time.sleep(0.1)
        finally:
            # The process group belongs to this invocation, including any child
            # left behind after its leader exits. Never inherit the terminal.
            try: os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError: pass
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            finally:
                try: os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError: pass
    if process.returncode:
        raise AssertionError(f"client exited {process.returncode}; inspect private diagnostics")
    return [json.loads(line) for line in stdout.read_text().splitlines() if line.strip()]


def inventory_probe(client, command, cwd, env, root, fixture_token):
    requests = []
    deferred_namespaces = set()
    resolved_tools = set()
    response_count = 0
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_): pass

        def do_GET(self):
            self.send_error(404)

        def do_POST(self):
            nonlocal response_count
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 8 * 1024 * 1024:
                self.send_error(413); return
            body = json.loads(self.rfile.read(size))
            if len(requests) >= 16:
                self.send_error(429); return
            for item in body.get("input", []):
                if item.get("type") == "custom_tool_call_output" and item.get("call_id") == "inventory":
                    resolved_tools.update(re.findall(r"mcp__webcodex__[A-Za-z0-9_]+", json.dumps(item.get("output"))))
            definitions = list(body.get("tools", []))
            for item in body.get("input", []):
                if item.get("type") == "additional_tools":
                    definitions.extend(item.get("tools", []))
            # Deferred MCP namespaces are advertised in developer context;
            # their full members are resolved through Code Mode's ALL_TOOLS.
            for item in body.get("input", []):
                if item.get("role") == "developer":
                    for block in item.get("content", []):
                        deferred_namespaces.update(re.findall(r"mcp__webcodex\b", block.get("text", "")))
            names = []
            def collect(tool):
                if tool.get("type") == "namespace":
                    for member in tool.get("tools", []): collect(member)
                elif tool.get("name"):
                    names.append(tool["name"])
                    # Codex Code Mode exposes executable subtools as typed
                    # declarations in exec's description, not top-level tools.
                    names.extend(re.findall(r"declare const tools: \{ ([A-Za-z0-9_]+)\(", tool.get("description", "")))
                elif tool.get("function", {}).get("name"): names.append(tool["function"]["name"])
                else: names.append(tool.get("type", "unknown"))
            for tool in definitions: collect(tool)
            if names: requests.append(sorted(names))
            if "/messages/count_tokens" in self.path:
                payload = json.dumps({"input_tokens": 1}).encode()
                self.send_response(200); self.send_header("Content-Type", "application/json")
            elif "/messages" in self.path:
                events = [
                    ("message_start", {"type":"message_start", "message":{"id":"msg_fixture", "type":"message", "role":"assistant", "content":[], "model":body.get("model"), "stop_reason":None, "stop_sequence":None, "usage":{"input_tokens":1,"output_tokens":0}}}),
                    ("content_block_start", {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                    ("content_block_delta", {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture"}}),
                    ("content_block_stop", {"type":"content_block_stop","index":0}),
                    ("message_delta", {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":None},"usage":{"output_tokens":1}}),
                    ("message_stop", {"type":"message_stop"})]
                payload = "".join(f"event: {kind}\ndata: {json.dumps(data)}\n\n" for kind,data in events).encode()
                self.send_response(200); self.send_header("Content-Type", "text/event-stream")
            elif self.path.endswith("/responses"):
                item = {"id":"msg_fixture", "type":"message", "role":"assistant", "content":[{"type":"output_text","text":"fixture","annotations":[]}]}
                if response_count == 0 and "exec" in names:
                    # Code Mode resolves deferred MCP tools only at execution.
                    # This fixed metadata-only read cannot call any model or tool.
                    item = {"type":"custom_tool_call", "call_id":"inventory", "name":"exec",
                            "input":"text(ALL_TOOLS.map(({name}) => name));"}
                response_count += 1
                events = [
                    {"type":"response.output_item.added","output_index":0,"item":item},
                    {"type":"response.output_item.done","output_index":0,"item":item},
                    {"type":"response.completed","response":{"id":"resp_fixture","status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}]
                payload = "".join(f"event: {data['type']}\ndata: {json.dumps(data)}\n\n" for data in events).encode()
                self.send_response(200); self.send_header("Content-Type", "text/event-stream")
            else:
                self.send_error(404); return
            self.send_header("Content-Length", str(len(payload))); self.end_headers(); self.wfile.write(payload)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    base = f"http://127.0.0.1:{server.server_port}"
    # No real credentials reach the fake provider. Explicit loopback endpoints
    # and process-local config prevent any paid model fallback.
    env = {k:v for k,v in env.items() if not any(word in k.upper() for word in ("TOKEN", "SECRET", "API_KEY", "PASSWORD"))}
    env.update({"WEBCODEX_LOCAL_TEST_TOKEN": fixture_token, "DISABLE_TELEMETRY":"1", "DISABLE_ERROR_REPORTING":"1"})
    command = list(command)
    if client == "claude":
        # Alternate providers must not override the loopback Anthropic endpoint.
        for key in ("CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY", "ANTHROPIC_CUSTOM_HEADERS"):
            env.pop(key, None)
        env.update({"ANTHROPIC_BASE_URL":base, "ANTHROPIC_API_KEY":"local-fixture",
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":"1"})
    else:
        # The custom provider requires no OpenAI authentication and uses HTTP SSE.
        command[-1:-1] = ["--config", 'model_provider="local_fixture"', "--config",
            'model_providers.local_fixture=' + '{name="Local fixture",base_url="'+base+'/v1",wire_api="responses",requires_openai_auth=false}',
            "--config", 'features.responses_websockets=false']
    try:
        frames = run_captured(command, cwd=cwd, env=env, prompt="Report the available tool names without executing project work.",
                              stdout=root / "probe.jsonl", stderr=root / "probe.stderr", timeout=45)
        if not requests: raise AssertionError("client probe did not expose its tool inventory")
        init = next((f.get("tools") for f in frames if f.get("type") == "system" and f.get("subtype") == "init"), None)
        return {"source":"loopback provider request; canned response, no model", "tools":sorted(set(requests[-1]) | resolved_tools), "client_init_tools":init, "deferred_mcp_namespaces":sorted(deferred_namespaces)}
    finally:
        server.shutdown(); server.server_close(); thread.join(timeout=5)
