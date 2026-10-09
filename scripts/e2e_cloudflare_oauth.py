#!/usr/bin/env python3
"""Opt-in, loopback-only Named/Quick OAuth acceptance with real Server/Runner.

Build explicitly with native TLS roots (the default production build uses webpki):
  cargo build --locked -p webcodex -p webcodex-runner -p webcodex-cli \
    --features reqwest/rustls-tls-native-roots --bin webcodex-server --bin webcodex-runner --bin webcodex
  python3 scripts/e2e_cloudflare_oauth.py --bin-dir target/debug

Requires Unix, Python 3 and openssl. An ephemeral CA is trusted only by these
child processes via SSL_CERT_FILE. HTTPS_PROXY routes only the fixture hosts
to a verified local TLS reverse proxy; neither global DNS nor trust is changed.
The fake cloudflared reports readiness; all OAuth/MCP/project traffic traverses
the actual dedicated ingress, Server and WebSocket Runner. A real asynchronous
Job retains its original receipt and one execution effect across a proxy outage.
OAuth discovery/challenges and negotiated MCP UI resources must report each
active entry's exact origin. No Cloudflare account, external model, OS service,
production state or TLS verification bypass is used.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import select
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from urllib.parse import parse_qs, urlencode, urlsplit

from e2e_session_continuity_ws import Smoke, check

HOSTS = ("named.webcodex.test", "first.trycloudflare.com", "second.trycloudflare.com",
         "third.trycloudflare.com", "fourth.trycloudflare.com")
# Exact LOCAL_USER authority profile, including the Job execution scope.
SCOPES = ["runtime:read", "runner:manage", "session:collaborate",
          "project:read", "project:write", "job:run"]
REDIRECT = "https://client.webcodex.test/callback"
MAX_BODY = 2 * 1024 * 1024


def reserve_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def private_file(path, content):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_text(content)
    path.chmod(0o600)


def stop_tree(process):
    if process is None:
        return
    try:
        os.killpg(process.pid, signal.SIGINT)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=20)
    except subprocess.TimeoutExpired:
        pass
    # A direct child exiting is not proof that its descendants exited.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=5)


class Fixture(Smoke):
    def __init__(self, bin_dir, root):
        super().__init__(bin_dir, root)
        self.processes = []
        self.drains = []
        self.servers = []
        self.network_blocked = False
        self.fake_pids = root / "cloudflared-pids"
        self.ingress = reserve_port()
        self.environment = root / "environment"
        self.environment.mkdir(mode=0o700)
        # Keep this test independent of ambient proxy, OpenAI and tunnel settings.
        for key in list(self.env):
            if key.upper().endswith("_PROXY") or key.startswith(("OPENAI_", "CLOUDFLARE_", "TUNNEL_")):
                self.env.pop(key)
        self.env.update({"HOME": str(root), "WEBCODEX_MCP_HOST_PROFILE": "host_code_mode",
                         "WEBCODEX_TUNNEL_ENVIRONMENT": str(self.environment),
                         "WEBCODEX_OAUTH2_ENABLED": "true"})

    def launch(self, binary, args, env, stdin=subprocess.DEVNULL):
        process = subprocess.Popen([str(self.bin_dir / binary), *args], env=env,
                                   stdin=stdin, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        self.processes.append(process)
        # Drain continuously, retaining only a bounded diagnostic tail in memory.
        tail = bytearray()
        def drain():
            with process.stdout:
                while block := process.stdout.read(8192):
                    tail.extend(block)
                    del tail[:-131072]
        thread = threading.Thread(target=drain, daemon=True)
        thread.start()
        self.drains.append((thread, tail))
        return process

    def topology(self):
        cert = self.root / "certificate.pem"
        key = self.root / "key.pem"
        ca = self.root / "ca.pem"
        ca_key = self.root / "ca-key.pem"
        csr = self.root / "request.pem"
        config = self.root / "openssl.cnf"
        private_file(config, "[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n"
                     "[dn]\nCN=WebCodex disposable acceptance\n[ext]\n"
                     "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n"
                     "extendedKeyUsage=serverAuth\nsubjectAltName=" +
                     ",".join(["DNS:" + h for h in HOSTS] + ["IP:127.0.0.1"]) + "\n")
        commands = [
            ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
             "-subj", "/CN=WebCodex disposable CA", "-addext", "basicConstraints=critical,CA:TRUE",
             "-keyout", str(ca_key), "-out", str(ca)],
            ["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-config", str(config),
             "-keyout", str(key), "-out", str(csr)],
            ["openssl", "x509", "-req", "-in", str(csr), "-CA", str(ca), "-CAkey", str(ca_key),
             "-CAcreateserial", "-days", "1", "-extfile", str(config), "-extensions", "ext", "-out", str(cert)],
        ]
        for command in commands:
            subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, check=True, timeout=15)
        key.chmod(0o600)
        ca_key.chmod(0o600)
        self.tls_context = ssl.create_default_context(cafile=str(ca))
        fixture = self
        class Reverse(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def forward(self):
                self.connection.settimeout(15)
                if fixture.network_blocked:
                    self.send_error(503)
                    return
                size = int(self.headers.get("Content-Length", "0"))
                if not 0 <= size <= MAX_BODY:
                    self.send_error(413)
                    return
                body = self.rfile.read(size) if size else None
                headers = {k: v for k, v in self.headers.items()
                           if k.lower() not in ("connection", "transfer-encoding", "accept-encoding")}
                upstream = http.client.HTTPConnection("127.0.0.1", fixture.ingress, timeout=20)
                try:
                    upstream.request(self.command, self.path, body, headers)
                    response = upstream.getresponse()
                    data = response.read(MAX_BODY + 1)
                    check(len(data) <= MAX_BODY, "oversized public response")
                    self.send_response(response.status)
                    for name, value in response.getheaders():
                        if name.lower() not in ("connection", "transfer-encoding", "content-length"):
                            self.send_header(name, value)
                    self.send_header("Content-Length", str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (OSError, http.client.HTTPException):
                    self.send_error(502)
                finally:
                    upstream.close()
            do_GET = do_POST = forward
            def log_message(self, *_):
                pass
        tls = ThreadingHTTPServer(("127.0.0.1", 0), Reverse)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(str(cert), str(key))
        tls.socket = context.wrap_socket(tls.socket, server_side=True)
        self.tls_port = tls.server_port
        class Connect(BaseHTTPRequestHandler):
            def do_CONNECT(self):
                if self.path not in {h + ":443" for h in HOSTS}:
                    self.send_error(403)
                    return
                self.connection.settimeout(15)
                with socket.create_connection(("127.0.0.1", fixture.tls_port), timeout=5) as peer:
                    self.send_response(200)
                    self.end_headers()
                    deadline = time.monotonic() + 30
                    while time.monotonic() < deadline:
                        ready, _, _ = select.select([self.connection, peer], [], [], 1)
                        for source in ready:
                            data = source.recv(65536)
                            if not data:
                                return
                            (peer if source is self.connection else self.connection).sendall(data)
            def log_message(self, *_):
                pass
        proxy = ThreadingHTTPServer(("127.0.0.1", 0), Connect)
        for server in (tls, proxy):
            server.daemon_threads = True
            self.servers.append(server)
            threading.Thread(target=server.serve_forever, daemon=True).start()
        self.env.update({"SSL_CERT_FILE": str(ca), "HTTPS_PROXY": f"http://127.0.0.1:{proxy.server_port}",
                         "NO_PROXY": "localhost,127.0.0.1,::1"})
        fake = self.root / "cloudflared"
        counter = self.root / "quick-count"
        private_file(fake, f"#!{sys.executable}\n" +
                     "import os,pathlib,sys,time\n"
                     "if '--version' in sys.argv:\n print('cloudflared version 2026.7.3');sys.exit(0)\n"
                     f"with pathlib.Path({str(self.fake_pids)!r}).open('a') as f:f.write(str(os.getpid())+'\\n')\n"
                     "assert sys.stdin.read() == ''\n"
                     "assert '--config' in sys.argv and '--no-autoupdate' in sys.argv\n"
                     "assert pathlib.Path(sys.argv[sys.argv.index('--config')+1]).read_text().strip() == '{}'\n"
                     "if '--token-file' in sys.argv:\n"
                     " assert '--url' not in sys.argv\n"
                     " assert pathlib.Path(sys.argv[sys.argv.index('--token-file')+1]).read_text().strip()\n"
                     " print('Registered tunnel connection',flush=True)\n"
                     "else:\n"
                     f" p=pathlib.Path({str(counter)!r});n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))\n"
                     " assert n <= 4\n"
                     " print('https://' + ['first','second','third','fourth'][n-1] + '.trycloudflare.com',flush=True)\n"
                     "while True:time.sleep(1)\n")
        fake.chmod(0o700)
        self.env["WEBCODEX_CLOUDFLARED_BIN"] = str(fake)

    def environment_files(self):
        request = {"service_scope": "user", "mode": {"kind": "user_create", "listen": f"127.0.0.1:{self.port}"},
                   "server_url": self.url, "project": None, "runner": True,
                   "account": {"name": "fixture", "identity": str(os.getuid()), "home": str(self.root)},
                   "binaries": {k: str(self.bin_dir / b) for k, b in
                                [("cli", "webcodex"), ("server", "webcodex-server"), ("runner", "webcodex-runner")]}}
        private_file(self.environment / "environment.json", json.dumps({"schema_version": 1,
                     "environment_id": "fixture", "request": request, "username": "cf-owner",
                     "runner_client_id": "cf-runner", "projects": [], "configured": True}))
        private_file(self.environment / "server/cloudflare-ingress.json", json.dumps({"port": self.ingress}))
        private_file(self.environment / "server/webcodex.env",
                     f"WEBCODEX_ADDR=127.0.0.1:{self.port}\nWEBCODEX_TOKEN={self.bootstrap}\n")
        records = []
        for name, provider in [("named", {"kind": "cloudflare_named", "public_origin": "https://" + HOSTS[0],
                                           "tunnel_id": "fixture-named"}), ("quick", {"kind": "cloudflare_quick"}),
                               ("standalone", {"kind": "cloudflare_quick"})]:
            standalone = name == "standalone"
            configuration_id = str(uuid.uuid4())
            records.append({"profile_id": name, "configuration_id": configuration_id,
                            "provider": provider, "name": name, "host_mode": "standalone" if standalone else "embedded",
                            "autostart": standalone, "revision": 1, "runtime_revision": 1, "installed": False, "started": False})
            binding = f"WEBCODEX_TUNNEL_PROFILE_ID={name}\nWEBCODEX_TUNNEL_PROVIDER=cloudflare_{'named' if name == 'named' else 'quick'}\n"
            if name == "named":
                reference = "00000000-0000-4000-8000-000000000001"
                binding += f"CLOUDFLARE_TUNNEL_TOKEN_REF={reference}\n"
                private_file(self.environment / f"server/tunnels/named/tokens/{reference}", secrets.token_urlsafe(32))
            private_file(self.environment / f"server/tunnels/{name}/webcodex.env", binding)
            private_file(self.environment / f"server/tunnels/{name}/readiness.json", "{}")
            if standalone:
                self.standalone_binding = self.environment / "server/tunnels/standalone/runtime.json"
                private_file(self.standalone_binding, json.dumps({"profile_id": name,
                    "configuration_id": configuration_id, "provider": provider,
                    "host_mode": "standalone", "autostart": True, "revision": 1, "runtime_revision": 1,
                    "owner_username": "cf-owner", "runner_client_id": "cf-runner", "ingress_port": self.ingress,
                    "token_ref": None, "local_server_url": self.url, "bootstrap_token": self.bootstrap}))
        private_file(self.environment / "tunnel.json", json.dumps(records))

    def public(self, origin, path, method="GET", body=None, token=None, headers=None, form=False):
        headers = {"Host": urlsplit(origin).netloc, **(headers or {})}
        if token:
            headers["Authorization"] = "Bearer " + token
        if body is not None:
            headers["Content-Type"] = "application/x-www-form-urlencoded" if form else "application/json"
            body = (urlencode(body) if form else json.dumps(body)).encode()
        connection = http.client.HTTPSConnection("127.0.0.1", self.tls_port, context=self.tls_context, timeout=25)
        try:
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            data = response.read(MAX_BODY + 1)
            check(len(data) <= MAX_BODY, "public response exceeded bound")
            return response.status, {name.lower(): value for name, value in response.getheaders()}, data
        finally:
            connection.close()

    def control(self, profile, action="status", **extra):
        result = self.post("/api/connections/cloudflare", {"action": action, "profile_id": profile, **extra},
                           self.bootstrap if action == "configure_oauth" else self.owner)
        check("error" not in result, f"Cloudflare {action}: {result.get('error')}")
        return result

    def start_profile(self, profile):
        status = self.control(profile)
        self.control(profile, "start", expected_revision=status["configured_revision"],
                     server_instance_id=status["server_instance_id"])
        deadline = time.monotonic() + 75
        while time.monotonic() < deadline:
            status = self.control(profile)
            check(status["lifecycle"] != "error", f"Cloudflare admission failed: {status.get('reason_code')}")
            if status["lifecycle"] == "running":
                check(status["public_origin"], "running origin missing")
                return status
            time.sleep(.2)
        raise AssertionError("Cloudflare admission deadline exceeded")

    def stop_profile(self, profile):
        status = self.control(profile)
        self.control(profile, "stop", server_instance_id=status["server_instance_id"],
                     process_generation=status["process_generation"])

    def wait_lifecycle(self, profile, lifecycle, deadline=None):
        deadline = min(time.monotonic() + 15, deadline or float("inf"))
        while time.monotonic() < deadline:
            status = self.control(profile)
            if status["lifecycle"] == lifecycle:
                return status
            time.sleep(.2)
        raise AssertionError(f"Cloudflare {lifecycle} observation deadline exceeded")

    def authorize(self, profile, status, client=None):
        origin = status["public_origin"]
        if client is None:
            client = self.control(profile, "configure_oauth", server_instance_id=status["server_instance_id"],
                                  process_generation=status["process_generation"],
                                  redirect_uri=REDIRECT, scopes=SCOPES)
            check(isinstance(client.get("client_secret"), str), "new OAuth secret absent")
        verifier = secrets.token_urlsafe(48)
        challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
        parameters = {"response_type": "code", "client_id": client["client_id"], "redirect_uri": REDIRECT,
                      "scope": " ".join(SCOPES), "state": secrets.token_urlsafe(16),
                      "resource": origin + "/mcp", "code_challenge": challenge, "code_challenge_method": "S256"}
        return_to = "/oauth/authorize?" + urlencode(parameters)
        code, _, _ = self.public(origin, return_to)
        check(code == 200, "OAuth login form unavailable")
        code, headers, _ = self.public(origin, "/oauth/authorize/login", "POST",
                                      {"return_to": return_to, "token": self.owner}, form=True,
                                      headers={"Origin": origin})
        check(code == 302, "owner OAuth browser login denied")
        cookie = headers.get("set-cookie", "").split(";")[0]
        check(cookie and self.owner not in cookie, "safe browser session cookie absent")
        code, headers, _ = self.public(origin, "/oauth/authorize/consent", "POST",
                                      parameters | {"decision": "allow"}, form=True,
                                      headers={"Origin": origin, "Cookie": cookie})
        check(code == 302, "OAuth consent denied")
        query = parse_qs(urlsplit(headers.get("location", "")).query)
        check(query.get("state") == [parameters["state"]] and "code" in query, "OAuth redirect/code mismatch")
        code, _, raw = self.public(origin, "/oauth/token", "POST", {
            "grant_type": "authorization_code", "code": query["code"][0], "client_id": client["client_id"],
            "client_secret": client["client_secret"], "redirect_uri": REDIRECT, "resource": origin + "/mcp",
            "code_verifier": verifier}, form=True)
        check(code == 200, "S256 code exchange denied")
        tokens = json.loads(raw)
        check(isinstance(tokens.get("access_token"), str) and isinstance(tokens.get("refresh_token"), str), "OAuth tokens missing")
        return client, tokens

    def mcp(self, origin, token, method="tools/list", params=None, *, ui=False):
        self.rpc_id += 1
        params = dict(params or {})
        params["_meta"] = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
                           "io.modelcontextprotocol/clientCapabilities": {}}
        if ui:
            params["_meta"]["io.modelcontextprotocol/clientCapabilities"] = {"extensions": {
                "io.modelcontextprotocol/ui": {"mimeTypes": ["text/html;profile=mcp-app"]}}}
        headers = {"Accept": "application/json, text/event-stream", "MCP-Protocol-Version": "2026-07-28",
                   "MCP-Method": method}
        if "name" in params:
            headers["MCP-Name"] = params["name"]
        elif method == "resources/read":
            headers["MCP-Name"] = params["uri"]
        status, _, raw = self.public(origin, "/mcp", "POST",
                                    {"jsonrpc": "2.0", "id": self.rpc_id, "method": method, "params": params},
                                    token, headers)
        return status, json.loads(raw)

    def metadata_snapshot(self, origin, access):
        status, _, raw = self.public(origin, "/.well-known/oauth-protected-resource")
        protected = json.loads(raw)
        check(status == 200 and protected.get("resource") == origin + "/mcp"
              and protected.get("authorization_servers") == [origin], "protected resource metadata used a stale origin")
        status, _, raw = self.public(origin, "/.well-known/oauth-authorization-server")
        authorization = json.loads(raw)
        check(status == 200 and authorization.get("issuer") == origin, "OAuth discovery issuer used a stale origin")
        for key, path in (("authorization_endpoint", "/oauth/authorize"), ("token_endpoint", "/oauth/token"),
                          ("revocation_endpoint", "/oauth/revoke")):
            check(authorization.get(key) == origin + path, "OAuth discovery endpoint used a stale origin: " + key)
        check(authorization.get("code_challenge_methods_supported") == ["S256"], "public discovery lost S256 policy")
        status, headers, _ = self.public(origin, "/mcp")
        check(status == 401 and headers.get("www-authenticate") ==
              f'Bearer resource_metadata="{origin}/.well-known/oauth-protected-resource"',
              "OAuth challenge used a stale origin")
        status, reply = self.mcp(origin, access, "resources/list", ui=True)
        resources = reply.get("result", {}).get("resources", [])
        check(status == 200 and resources, "negotiated MCP UI resource inventory missing")
        for resource in resources:
            check(resource.get("_meta", {}).get("ui", {}).get("domain") == origin,
                  "MCP UI resource listing used a stale public origin")
        # The small bundled workbench checks metadata without raising the
        # fixture's response bound to accommodate embedded document libraries.
        resource = next(item for item in resources if "/workbench/" in item.get("uri", ""))
        status, reply = self.mcp(origin, access, "resources/read", {"uri": resource["uri"]}, ui=True)
        contents = reply.get("result", {}).get("contents", [])
        check(status == 200 and contents, "negotiated MCP UI resource read failed")
        check(all(item.get("_meta", {}).get("ui", {}).get("domain") == origin
                  and item.get("uri") == resource["uri"] and item.get("text") for item in contents),
              "MCP UI resource read used a stale public origin")

    def project_round_trip(self, origin, access, expected, replacement):
        self.metadata_snapshot(origin, access)
        status, inventory = self.mcp(origin, access)
        check(status == 200 and {"read_files", "edit_project_files"} <=
              {t["name"] for t in inventory.get("result", {}).get("tools", [])}, "public tools/list failed")
        def tool(name, arguments):
            status, reply = self.mcp(origin, access, "tools/call", {"name": name, "arguments": arguments})
            output = reply.get("result", {}).get("structuredContent", {})
            check(status == 200 and output.get("success") is True, f"public {name} failed")
            return output["output"]
        args = {"project": self.project, "items": [{"path": "one.txt"}]}
        before = tool("read_files", args)["items"][0]["output"]
        check(expected in before["text"], "public project read mismatch")
        tool("edit_project_files", {"project": self.project, "changes": [{"kind": "edit", "path": "one.txt",
             "expected_read_revision": before["read_revision"], "edits": [{"kind": "replace_exact",
             "old_text": expected, "new_text": replacement}]}]})
        check(replacement in tool("read_files", args)["items"][0]["output"]["text"], "public project write mismatch")
        check((self.project_dir / "one.txt").read_text() == replacement + "\n", "Runner write effect missing")

    def job_survives_cloudflare_outage(self, named, access):
        # Start once, then observe that same receipt. Neither transport recovery
        # nor a missing response authorizes replaying the business operation.
        deadline = time.monotonic() + 25
        origin = named["public_origin"]
        marker = self.project_dir / "job-effects.txt"
        release = self.project_dir / "job-release"
        server_pid, runner_pid = self.server.pid, self.runner.pid
        def tool(name, arguments):
            status, reply = self.mcp(origin, access, "tools/call", {"name": name, "arguments": arguments})
            result = reply.get("result", {}).get("structuredContent", {})
            check(status == 200 and result.get("success") is True, f"public Job {name} failed")
            return result["output"]
        program = (
            "from pathlib import Path\nimport sys,time\n"
            "with Path('job-effects.txt').open('a') as f:f.write('once\\n')\n"
            "deadline=time.monotonic()+20\n"
            "while not Path('job-release').exists():\n"
            " if time.monotonic()>=deadline:sys.exit(3)\n"
            " time.sleep(.05)\n"
            "print('completed original job',flush=True)\n"
        )
        started = tool("run_process", {"project": self.project, "executable": sys.executable,
                       "args": ["-u", "-c", program], "interactive": True, "timeout_secs": 25})
        job = started.get("job_id") or started.get("continuation", {}).get("arguments", {}).get("items", [{}])[0].get("job_id")
        check(started.get("execution_state") == "pending" and isinstance(job, str), "real asynchronous Job receipt missing")
        while not marker.exists() and time.monotonic() < deadline:
            time.sleep(.05)
        check(marker.exists() and marker.read_text() == "once\n", "Job did not start exactly once")
        before = tool("observe_jobs", {"items": [{"job_id": job}], "tail_lines": 10})["items"][0]
        check(before["job_id"] == job and not before.get("terminal"), "original Job ended before outage")
        try:
            self.network_blocked = True
            disconnected = self.wait_lifecycle("named", "disconnected", deadline)
            check(disconnected["process_generation"] == named["process_generation"], "network loss restarted connector")
            check(marker.read_text() == "once\n" and not release.exists(), "outage duplicated or ended Job effect")
        finally:
            self.network_blocked = False
        recovered = self.wait_lifecycle("named", "running", deadline)
        check(recovered["process_generation"] == named["process_generation"], "network recovery restarted connector")
        during = tool("observe_jobs", {"items": [{"job_id": job}], "tail_lines": 10})["items"][0]
        check(during["job_id"] == job and not during.get("terminal"), "recovery replaced or terminated original Job")
        release.write_text("finish original Job\n")
        while time.monotonic() < deadline:
            item = tool("observe_jobs", {"items": [{"job_id": job}], "tail_lines": 10, "wait_secs": 1})["items"][0]
            check(item["job_id"] == job, "terminal observation changed Job identity")
            if item.get("terminal"):
                check(item.get("status") == "completed" and item.get("exit_code") == 0, "original Job failed after recovery")
                break
        else:
            raise AssertionError("original Job terminal observation deadline exceeded")
        check(marker.read_text() == "once\n", "Cloudflare reconnect duplicated Job execution")
        check((self.server.pid, self.runner.pid) == (server_pid, runner_pid), "outage restarted Server or Runner")
        check(time.monotonic() < deadline, "Cloudflare Job continuity exceeded 25 seconds")

    def boundaries(self, origin, access):
        for path in ("/api/users/create", "/api/tokens/create", "/api/connections/cloudflare", "/api/tools/call", "/ws/agent", "/openapi.json", "/"):
            status, _, _ = self.public(origin, path, "POST" if path.startswith("/api/") else "GET", {} if path.startswith("/api/") else None, access)
            check(status in ((404, 405) if path == "/" else (404,)),
                  f"private route exposed on public ingress: {path}, status={status}")
        for token in (self.bootstrap, self.owner, self.agent, "wc_account_fixture", "wc_shared_fixture", "wc_project_fixture"):
            status, _ = self.mcp(origin, token)
            check(status == 401, "non-OAuth public MCP credential admitted")
        status, _, _ = self.public(origin, "/mcp", token=access, headers={"Host": "forged.invalid", "X-Forwarded-Host": urlsplit(origin).netloc})
        check(status == 403, "forged Host admitted")
        status, _, _ = self.public(origin, "/mcp", token=access, headers={"Origin": "https://forged.invalid"})
        check(status == 403, "foreign Origin admitted")
        private = self.post("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}, access)
        check(private.get("error") is not None, "public OAuth credential crossed private listener")

    def prepare(self):
        self.topology()
        self.environment_files()
        self.start_server()
        check(self.post("/api/users/create", {"username": "cf-owner"}, self.bootstrap).get("success"), "fixture owner setup")
        self.owner = self.pat("cf-owner", SCOPES)
        agent = self.post("/api/agent-tokens/create", {"username": "cf-owner", "client_id": "cf-runner", "name": "fixture"}, self.bootstrap)
        self.agent = agent["token"]
        self.project_dir = self.root / "project"
        self.project_dir.mkdir()
        registry = self.root / "projects"
        registry.mkdir()
        private_file(registry / "demo.toml", 'id = "demo"\npath = ' + json.dumps(str(self.project_dir)) + '\n')
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.project_dir)], env=self.env,
                       stdin=subprocess.DEVNULL, check=True, timeout=10)
        (self.project_dir / "one.txt").write_text("before\n")
        config = self.root / "runner.toml"
        private_file(config, '\n'.join([f'server_url = "{self.url}"', f'token = {json.dumps(self.agent)}',
                     'client_id = "cf-runner"', 'owner = "cf-owner"', 'transport = "websocket"',
                     f'project_registry_dir = {json.dumps(str(registry))}', '[policy]',
                     f'allowed_roots = [{json.dumps(str(self.project_dir))}]']) + '\n')
        runner_env = {k: v for k, v in self.env.items() if k not in ("HTTPS_PROXY", "SSL_CERT_FILE", "WEBCODEX_TUNNEL_ENVIRONMENT", "WEBCODEX_CLOUDFLARED_BIN")}
        self.runner = self.launch("webcodex-runner", ["--config", str(config)], runner_env)
        self.project = "agent:cf-runner:demo"
        self.wait_ready(self.project, self.owner)

    def run(self):
        self.prepare()
        named = self.start_profile("named")
        client, tokens = self.authorize("named", named)
        self.project_round_trip(named["public_origin"], tokens["access_token"], "before", "named")
        self.boundaries(named["public_origin"], tokens["access_token"])
        check(self.control("named")["observed_authorization"], "used OAuth authorization not observed")
        old_instance = named["server_instance_id"]
        stop_tree(self.server)
        self.start_server()
        self.wait_ready(self.project, self.owner)
        named = self.start_profile("named")
        check(named["server_instance_id"] != old_instance, "Server restart did not replace instance")
        self.project_round_trip(named["public_origin"], tokens["access_token"], "named", "restarted")
        status, _, raw = self.public(named["public_origin"], "/oauth/token", "POST", {
            "grant_type": "refresh_token", "refresh_token": tokens["refresh_token"],
            "client_id": client["client_id"], "client_secret": client["client_secret"]}, form=True)
        check(status == 200 and isinstance(json.loads(raw).get("access_token"), str),
              "Named refresh grant did not survive Server restart")
        self.job_survives_cloudflare_outage(named, tokens["access_token"])
        self.stop_profile("named")
        quick = self.start_profile("quick")
        quick_client, old = self.authorize("quick", quick)
        self.project_round_trip(quick["public_origin"], old["access_token"], "restarted", "quick")
        self.stop_profile("quick")
        quick2 = self.start_profile("quick")
        check(quick2["process_generation"] > quick["process_generation"] and quick2["public_origin"] != quick["public_origin"], "second Quick start did not rotate")
        status, _ = self.mcp(quick2["public_origin"], old["access_token"])
        check(status == 401, "old Quick access survived epoch rotation")
        status, _, _ = self.public(quick2["public_origin"], "/oauth/token", "POST", {
            "grant_type": "refresh_token", "refresh_token": old["refresh_token"], "client_id": quick_client["client_id"],
            "client_secret": quick_client["client_secret"]}, form=True)
        check(status == 400, "old Quick refresh survived epoch rotation")
        _, fresh = self.authorize("quick", quick2, quick_client)
        self.project_round_trip(quick2["public_origin"], fresh["access_token"], "quick", "fresh")
        self.boundaries(quick2["public_origin"], fresh["access_token"])
        self.stop_profile("quick")
        self.standalone()
        print("PASS: verified TLS, active-origin OAuth/UI metadata, real Server/Runner, Named S256 OAuth/read/write, Server restart, original Job completes exactly once across network outage, Quick rotation/fresh OAuth, standalone EOF/crash lease, public whitelist and credential boundary")

    def standalone(self):
        args = ["server", "tunnel", "--provider", "cloudflare_quick", "--runtime-binding",
                str(self.standalone_binding), "--json", "--stop-on-stdin-eof"]
        driver = self.launch("webcodex", args, self.env, stdin=subprocess.PIPE)
        first = self.wait_lifecycle("standalone", "running")
        client, tokens = self.authorize("standalone", first)
        self.project_round_trip(first["public_origin"], tokens["access_token"], "fresh", "standalone")
        driver.stdin.close()
        check(driver.wait(timeout=15) == 0, "standalone EOF shutdown failed")
        self.wait_lifecycle("standalone", "stopped")
        driver = self.launch("webcodex", args, self.env, stdin=subprocess.PIPE)
        second = self.wait_lifecycle("standalone", "running")
        check(second["process_generation"] > first["process_generation"], "standalone generation did not advance")
        status, _ = self.mcp(second["public_origin"], tokens["access_token"])
        check(status == 401, "standalone old Quick grant survived EOF/restart")
        _, fresh = self.authorize("standalone", second, client)
        self.project_round_trip(second["public_origin"], fresh["access_token"], "standalone", "driver")
        os.killpg(driver.pid, signal.SIGKILL)
        driver.wait(timeout=5)
        driver.stdin.close()
        lost = self.wait_lifecycle("standalone", "error")
        check(lost["reason_code"] == "cloudflare_owner_lost" and not lost["public_origin"], "driver crash did not close lease admission")
        status, _, _ = self.public(second["public_origin"], "/mcp", token=fresh["access_token"])
        check(status == 503, "public admission survived crashed standalone owner")
        for action in ("heartbeat", "activate"):
            body = {"action": action, "profile_id": "standalone", "server_instance_id": second["server_instance_id"],
                    "process_generation": second["process_generation"]}
            if action == "activate":
                body.update(expected_revision=1, origin=second["public_origin"])
            rejected = self.post("/api/connections/cloudflare", body, self.owner)
            check(rejected.get("error") == "cloudflare_stale_attempt", "stale standalone callback was admitted")

    def close(self):
        for process in reversed(self.processes):
            stop_tree(process)
        # Managed cloudflared owns a separate group. Reclaim it even if a failing
        # Server required forced termination before its normal cleanup completed.
        if self.fake_pids.exists():
            for pid in map(int, self.fake_pids.read_text().splitlines()):
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        for thread, _ in self.drains:
            thread.join(timeout=3)
            check(not thread.is_alive(), "child output drain did not reach EOF")
        for server in self.servers:
            server.shutdown()
            server.server_close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=Path(__file__).resolve().parents[1] / "target/debug")
    args = parser.parse_args()
    check(os.name == "posix", "this harness requires Unix process groups")
    for binary in ("webcodex-server", "webcodex-runner", "webcodex"):
        check((args.bin_dir / binary).is_file(), "build fixture binaries first: " + binary)
    with tempfile.TemporaryDirectory(prefix="webcodex-cloudflare-oauth-") as directory:
        fixture = Fixture(args.bin_dir.resolve(), Path(directory))
        def deadline(*_):
            raise TimeoutError("overall Cloudflare E2E deadline exceeded")
        signal.signal(signal.SIGALRM, deadline)
        signal.alarm(300)
        try:
            fixture.run()
        finally:
            signal.alarm(0)
            fixture.close()


if __name__ == "__main__":
    main()
