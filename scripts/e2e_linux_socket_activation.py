#!/usr/bin/env python3
"""Focused Linux socket-activation, listener-continuity, and graceful-drain proof.

This deliberately does not require PID 1 systemd. It has three focused scenarios:
1. systemd-socket-activate -> inherited fd 3 -> real webcodex-server -> HTTP.
2. A parent-owned TCP listener survives Server A termination and is inherited by
   Server B while bounded client probes classify success/reset/timeout/refused.
3. A real WebCodex run_process request is already executing on a real WebSocket
   Runner when Server A receives SIGTERM; the response completes before A exits,
   then Server B inherits the same listener and the Runner reconnects.

Listener continuity requires refused == 0 under the bounded probe/backlog load.
Resets and timeouts are always reported separately; L2 does not claim they are zero.
"""

from __future__ import annotations

import argparse
import errno
import http.client
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Dict, Optional

HTTP_FD_NAME = "webcodex-http"
LISTEN_FD = 3
HOST = "127.0.0.1"
PROBE_PATH = "/openapi.json"
TOKEN = "linux-socket-activation-e2e-token"
RUNNER_CLIENT_ID = "linux-graceful-drain-e2e"
RUNNER_PROJECT_ID = "graceful-drain"
RUNTIME_PROJECT_ID = f"agent:{RUNNER_CLIENT_ID}:{RUNNER_PROJECT_ID}"


def reserve_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        sock.bind((HOST, 0))
        return int(sock.getsockname()[1])
    finally:
        sock.close()


def server_env(port: int, data_dir: Path) -> Dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "WEBCODEX_ADDR": f"{HOST}:{port}",
            "WEBCODEX_DATA": str(data_dir),
            "WEBCODEX_TOKEN": TOKEN,
            "RUST_LOG": env.get("RUST_LOG", "warn"),
        }
    )
    for key in ("LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"):
        env.pop(key, None)
    return env


def terminate(proc: Optional[subprocess.Popen[bytes]]) -> None:
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def http_probe(port: int, timeout: float) -> str:
    conn = http.client.HTTPConnection(HOST, port, timeout=timeout)
    try:
        conn.request("GET", PROBE_PATH, headers={"Connection": "close"})
        response = conn.getresponse()
        response.read(1024)
        return "success" if 200 <= response.status < 500 else "reset"
    except ConnectionRefusedError:
        return "refused"
    except (ConnectionResetError, BrokenPipeError):
        return "reset"
    except socket.timeout:
        return "timeout"
    except OSError as error:
        if error.errno == errno.ECONNREFUSED:
            return "refused"
        if error.errno in (errno.ECONNRESET, errno.EPIPE):
            return "reset"
        if error.errno in (errno.ETIMEDOUT, errno.EAGAIN):
            return "timeout"
        raise
    finally:
        conn.close()


def api_post_json(port: int, path: str, payload: Dict[str, object], timeout: float) -> tuple[int, Dict[str, object]]:
    body = json.dumps(payload).encode()
    conn = http.client.HTTPConnection(HOST, port, timeout=timeout)
    try:
        conn.request(
            "POST",
            path,
            body=body,
            headers={
                "Authorization": f"Bearer {TOKEN}",
                "Content-Type": "application/json",
                "Content-Length": str(len(body)),
                "Connection": "close",
            },
        )
        response = conn.getresponse()
        raw = response.read()
        parsed = json.loads(raw.decode()) if raw else {}
        if not isinstance(parsed, dict):
            raise RuntimeError(f"expected JSON object from {path}, got {type(parsed).__name__}")
        return response.status, parsed
    finally:
        conn.close()


def wait_agent_online(port: int, proc: subprocess.Popen[bytes], timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    last = "not-started"
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"runner exited before online with code {proc.returncode}")
        try:
            status, body = api_post_json(port, "/api/runtime/status", {}, 1.0)
            online = body.get("output", {}).get("agents", {}).get("online_count") if status == 200 else None
            last = f"status={status} online={online!r}"
            if online == 1:
                return
        except (OSError, ValueError, RuntimeError) as error:
            last = repr(error)
        time.sleep(0.05)
    raise RuntimeError(f"runner did not become online; last={last}")


def wait_ready(port: int, proc: subprocess.Popen[bytes], timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    last = "not-started"
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"server exited before readiness with code {proc.returncode}")
        try:
            last = http_probe(port, 0.5)
            if last == "success":
                return
        except OSError as error:
            last = repr(error)
        time.sleep(0.05)
    raise RuntimeError(f"server did not become HTTP-ready; last probe={last}")


def run_systemd_smoke(binary: Path, systemd_socket_activate: Path) -> Dict[str, object]:
    port = reserve_port()
    with tempfile.TemporaryDirectory(prefix="webcodex-systemd-socket-smoke-") as root:
        root_path = Path(root)
        log_path = root_path / "server.log"
        env = server_env(port, root_path / "data")
        (root_path / "data").mkdir()
        with log_path.open("wb") as log:
            proc = subprocess.Popen(
                [
                    str(systemd_socket_activate),
                    "--listen",
                    f"{HOST}:{port}",
                    "--setenv",
                    f"WEBCODEX_ADDR={env['WEBCODEX_ADDR']}",
                    "--setenv",
                    f"WEBCODEX_DATA={env['WEBCODEX_DATA']}",
                    "--setenv",
                    f"WEBCODEX_TOKEN={env['WEBCODEX_TOKEN']}",
                    "--setenv",
                    f"RUST_LOG={env['RUST_LOG']}",
                    "--fdname",
                    HTTP_FD_NAME,
                    str(binary),
                ],
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                wait_ready(port, proc)
                result = http_probe(port, 1.0)
                if result != "success":
                    raise RuntimeError(f"post-readiness smoke probe was {result}")
                return {
                    "scenario": "systemd_socket_activate",
                    "http": result,
                    "port": port,
                }
            except Exception as error:
                log.flush()
                tail = log_path.read_text(errors="replace")[-4000:]
                raise RuntimeError(f"systemd socket activation smoke failed: {error}\n{tail}") from error
            finally:
                terminate(proc)


def child_exec(binary: Path, inherited_fd: int) -> None:
    if inherited_fd != LISTEN_FD:
        os.dup2(inherited_fd, LISTEN_FD, inheritable=True)
        os.close(inherited_fd)
    else:
        os.set_inheritable(LISTEN_FD, True)
    env = os.environ.copy()
    env["LISTEN_PID"] = str(os.getpid())
    env["LISTEN_FDS"] = "1"
    env["LISTEN_FDNAMES"] = HTTP_FD_NAME
    os.execve(str(binary), [str(binary)], env)


def spawn_inherited_server(binary: Path, listener: socket.socket, env: Dict[str, str], log) -> subprocess.Popen[bytes]:
    return subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "--child-exec", str(binary), str(listener.fileno())],
        env=env,
        pass_fds=(listener.fileno(),),
        stdout=log,
        stderr=subprocess.STDOUT,
    )


def spawn_runner(binary: Path, config: Path, log) -> subprocess.Popen[bytes]:
    return subprocess.Popen(
        [str(binary), "--config", str(config)],
        stdout=log,
        stderr=subprocess.STDOUT,
    )


def run_continuity(binary: Path) -> Dict[str, object]:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((HOST, 0))
    listener.listen(128)
    port = int(listener.getsockname()[1])
    listener.set_inheritable(True)

    counts = {"success": 0, "reset": 0, "timeout": 0, "refused": 0}
    unexpected = []
    stop = threading.Event()
    lock = threading.Lock()

    def probe_loop() -> None:
        while not stop.is_set():
            try:
                outcome = http_probe(port, 0.08)
                with lock:
                    counts[outcome] += 1
            except Exception as error:  # surfaced separately, never folded into refusal
                with lock:
                    unexpected.append(repr(error))
            stop.wait(0.01)

    with tempfile.TemporaryDirectory(prefix="webcodex-listener-continuity-") as root:
        root_path = Path(root)
        data = root_path / "data"
        data.mkdir()
        log_path = root_path / "server.log"
        env = server_env(port, data)
        server_a: Optional[subprocess.Popen[bytes]] = None
        server_b: Optional[subprocess.Popen[bytes]] = None
        thread: Optional[threading.Thread] = None
        try:
            with log_path.open("ab", buffering=0) as log:
                server_a = spawn_inherited_server(binary, listener, env, log)
                wait_ready(port, server_a)
                thread = threading.Thread(target=probe_loop, name="listener-continuity-probe", daemon=True)
                thread.start()
                time.sleep(0.25)

                terminate(server_a)
                server_a = None
                # Deliberate process gap: the parent alone retains the listener.
                # Probe rate/backlog are bounded so queued connections fit easily.
                time.sleep(0.20)

                server_b = spawn_inherited_server(binary, listener, env, log)
                wait_ready(port, server_b)
                time.sleep(0.35)
        except Exception as error:
            tail = log_path.read_text(errors="replace")[-5000:] if log_path.exists() else ""
            raise RuntimeError(f"listener continuity harness failed: {error}\n{tail}") from error
        finally:
            stop.set()
            if thread is not None:
                thread.join(timeout=2)
            terminate(server_a)
            terminate(server_b)
            listener.close()

    result: Dict[str, object] = {
        "scenario": "parent_owned_listener_restart",
        **counts,
        "unexpected_errors": unexpected,
        "backlog": 128,
        "deliberate_server_gap_ms": 200,
        "port": port,
    }
    if counts["refused"] != 0:
        raise RuntimeError(f"L1 acceptance failed: ECONNREFUSED count={counts['refused']}; result={json.dumps(result)}")
    if unexpected:
        raise RuntimeError(f"unexpected probe errors: {json.dumps(result)}")
    if counts["success"] == 0:
        raise RuntimeError(f"no successful probes recorded: {json.dumps(result)}")
    return result


def run_graceful_inflight(binary: Path, runner_binary: Path) -> Dict[str, object]:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((HOST, 0))
    listener.listen(128)
    port = int(listener.getsockname()[1])
    listener.set_inheritable(True)

    counts = {"success": 0, "reset": 0, "timeout": 0, "refused": 0}
    unexpected = []
    stop = threading.Event()
    lock = threading.Lock()

    def probe_loop() -> None:
        while not stop.is_set():
            try:
                outcome = http_probe(port, 0.08)
                with lock:
                    counts[outcome] += 1
            except Exception as error:
                with lock:
                    unexpected.append(repr(error))
            stop.wait(0.01)

    with tempfile.TemporaryDirectory(prefix="webcodex-graceful-inflight-") as root:
        root_path = Path(root)
        data = root_path / "data"
        projects = root_path / "project-registry"
        project = root_path / "project"
        data.mkdir()
        projects.mkdir()
        project.mkdir()
        started_marker = project / "inflight-started.txt"
        finished_marker = project / "inflight-finished.txt"
        task_script = project / "drain_task.py"
        task_script.write_text(
            "from pathlib import Path\n"
            "import time\n"
            "Path('inflight-started.txt').write_text('started\\n')\n"
            "time.sleep(1.0)\n"
            "Path('inflight-finished.txt').write_text('finished\\n')\n"
            "print('graceful-inflight-complete')\n"
        )
        (project / "README.md").write_text("# graceful drain e2e fixture\n")
        project_toml = projects / f"{RUNNER_PROJECT_ID}.toml"
        project_toml.write_text(
            f"id = {json.dumps(RUNNER_PROJECT_ID)}\n"
            f"path = {json.dumps(str(project))}\n"
            'name = "Graceful Drain E2E"\n'
            "allow_patch = true\n"
        )
        runner_config = root_path / "runner.toml"
        runner_config.write_text(
            f"server_url = {json.dumps(f'http://{HOST}:{port}')}\n"
            f"token = {json.dumps(TOKEN)}\n"
            f"client_id = {json.dumps(RUNNER_CLIENT_ID)}\n"
            'display_name = "Graceful Drain E2E Runner"\n'
            'owner = "e2e"\n'
            f"project_registry_dir = {json.dumps(str(projects))}\n"
            "poll_interval_ms = 200\n"
            'transport = "websocket"\n\n'
            "[policy]\n"
            f"allowed_roots = [{json.dumps(str(project))}]\n"
            "max_timeout_secs = 30\n"
            "max_output_bytes = 262144\n"
        )
        server_log_path = root_path / "server.log"
        runner_log_path = root_path / "runner.log"
        env = server_env(port, data)
        server_a: Optional[subprocess.Popen[bytes]] = None
        server_b: Optional[subprocess.Popen[bytes]] = None
        runner: Optional[subprocess.Popen[bytes]] = None
        probes: Optional[threading.Thread] = None
        request_thread: Optional[threading.Thread] = None
        request_result: Dict[str, object] = {}

        def run_inflight_request() -> None:
            try:
                status, body = api_post_json(
                    port,
                    "/api/tools/call",
                    {
                        "tool": "run_process",
                        "params": {
                            "project": RUNTIME_PROJECT_ID,
                            "executable": "python3",
                            "args": ["drain_task.py"],
                            "timeout_secs": 10,
                        },
                    },
                    8.0,
                )
                request_result.update({"http_status": status, "body": body})
            except Exception as error:
                request_result["error"] = repr(error)

        try:
            with server_log_path.open("ab", buffering=0) as server_log, runner_log_path.open(
                "ab", buffering=0
            ) as runner_log:
                server_a = spawn_inherited_server(binary, listener, env, server_log)
                wait_ready(port, server_a)
                runner = spawn_runner(runner_binary, runner_config, runner_log)
                wait_agent_online(port, runner)

                probes = threading.Thread(target=probe_loop, name="graceful-drain-probe", daemon=True)
                probes.start()
                request_thread = threading.Thread(
                    target=run_inflight_request, name="graceful-inflight-request", daemon=True
                )
                request_thread.start()

                marker_deadline = time.monotonic() + 5.0
                while time.monotonic() < marker_deadline and not started_marker.exists():
                    if request_result:
                        break
                    time.sleep(0.01)
                if not started_marker.exists():
                    raise RuntimeError(f"real run_process never started: {request_result!r}")

                signal_started = time.monotonic()
                server_a.send_signal(signal.SIGTERM)
                request_thread.join(timeout=5.0)
                if request_thread.is_alive():
                    raise RuntimeError("in-flight HTTP request did not complete during graceful drain")
                if "error" in request_result:
                    raise RuntimeError(f"in-flight HTTP request failed: {request_result['error']}")
                if request_result.get("http_status") != 200:
                    raise RuntimeError(f"in-flight HTTP status was {request_result.get('http_status')}: {request_result!r}")
                body = request_result.get("body")
                if not isinstance(body, dict) or body.get("success") is not True:
                    raise RuntimeError(f"in-flight tool response was not successful: {request_result!r}")
                if not finished_marker.exists():
                    raise RuntimeError("in-flight command response returned before command completion marker")

                server_a.wait(timeout=5.0)
                server_a_exit = server_a.returncode
                shutdown_elapsed_ms = int((time.monotonic() - signal_started) * 1000)
                if server_a_exit != 0:
                    raise RuntimeError(f"Server A did not exit normally after SIGTERM: {server_a_exit}")
                server_a = None
                server_b = spawn_inherited_server(binary, listener, env, server_log)
                wait_ready(port, server_b)
                wait_agent_online(port, runner)
                time.sleep(0.25)
                runner_reconnected = True
        except Exception as error:
            server_tail = server_log_path.read_text(errors="replace")[-6000:] if server_log_path.exists() else ""
            runner_tail = runner_log_path.read_text(errors="replace")[-4000:] if runner_log_path.exists() else ""
            raise RuntimeError(
                f"graceful in-flight harness failed: {error}\n--- server ---\n{server_tail}\n--- runner ---\n{runner_tail}"
            ) from error
        finally:
            stop.set()
            if probes is not None:
                probes.join(timeout=2)
            terminate(server_a)
            terminate(server_b)
            terminate(runner)
            listener.close()

    result: Dict[str, object] = {
        "scenario": "graceful_inflight_restart",
        **counts,
        "unexpected_errors": unexpected,
        "backlog": 128,
        "inflight_request_http_status": request_result.get("http_status"),
        "inflight_request_success": isinstance(request_result.get("body"), dict)
        and request_result["body"].get("success") is True,
        "server_a_exit_code": server_a_exit,
        "server_a_shutdown_elapsed_ms": shutdown_elapsed_ms,
        "runner_reconnected": runner_reconnected,
        "port": port,
    }
    if counts["refused"] != 0:
        raise RuntimeError(f"L2 listener continuity failed: ECONNREFUSED count={counts['refused']}; result={json.dumps(result)}")
    if unexpected:
        raise RuntimeError(f"unexpected probe errors: {json.dumps(result)}")
    if counts["success"] == 0:
        raise RuntimeError(f"no successful probes recorded: {json.dumps(result)}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--server-bin", default="target/dogfood/webcodex-server")
    parser.add_argument("--runner-bin", default="target/dogfood/webcodex-runner")
    parser.add_argument("--skip-systemd-smoke", action="store_true")
    parser.add_argument("--skip-continuity", action="store_true")
    parser.add_argument("--skip-graceful-inflight", action="store_true")
    parser.add_argument("--child-exec", nargs=2, metavar=("BINARY", "FD"), help=argparse.SUPPRESS)
    args = parser.parse_args()

    if args.child_exec:
        child_exec(Path(args.child_exec[0]).resolve(), int(args.child_exec[1]))
        return 127

    binary = Path(args.server_bin).resolve()
    if not binary.is_file():
        parser.error(f"server binary does not exist: {binary}")
    runner_binary = Path(args.runner_bin).resolve()
    if not args.skip_graceful_inflight and not runner_binary.is_file():
        parser.error(f"runner binary does not exist: {runner_binary}")

    results = []
    if not args.skip_systemd_smoke:
        tool = shutil.which("systemd-socket-activate")
        if not tool:
            parser.error("systemd-socket-activate is required for the activation smoke")
        results.append(run_systemd_smoke(binary, Path(tool)))
    if not args.skip_continuity:
        results.append(run_continuity(binary))
    if not args.skip_graceful_inflight:
        results.append(run_graceful_inflight(binary, runner_binary))

    print(json.dumps({"ok": True, "results": results}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
