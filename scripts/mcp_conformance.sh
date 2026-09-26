#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# The conformance referee is intentionally immutable. Updating it is a reviewed
# baseline change because scenario/check semantics can change between commits.
BASELINE="tests/fixtures/mcp/conformance/baseline.json"
HARNESS_COMMIT="$(python3 - "$BASELINE" <<'PY'
import json
import pathlib
import re
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
commit = value.get("harness_commit", "")
if not re.fullmatch(r"[0-9a-f]{40}", commit):
    raise SystemExit("baseline harness_commit must be a full lowercase Git SHA")
print(commit)
PY
)"
WORK_ROOT="${WEBPI_MCP_CONFORMANCE_WORK_ROOT:-target/mcp-conformance}"
HARNESS_DIR_EXTERNAL=0
if [ "${WEBPI_MCP_CONFORMANCE_HARNESS_DIR+x}" = x ]; then
  HARNESS_DIR="${WEBPI_MCP_CONFORMANCE_HARNESS_DIR}"
  HARNESS_DIR_EXTERNAL=1
else
  HARNESS_DIR="$WORK_ROOT/harness"
fi
REPORT_ROOT_EXTERNAL=0
if [ "${WEBPI_MCP_CONFORMANCE_REPORT_ROOT+x}" = x ]; then
  REPORT_ROOT="${WEBPI_MCP_CONFORMANCE_REPORT_ROOT}"
  REPORT_ROOT_EXTERNAL=1
else
  REPORT_ROOT="$WORK_ROOT/reports"
fi
PROFILES=("$@")
HARNESS_OWNER_MARKER=".webcodex-mcp-conformance-harness"
REPORT_OWNER_MARKER=".webcodex-mcp-conformance-reports"
harness_build_dir=""
fixture_dir=""
fixture_pid=""
cleanup_started=0

if [ "$#" -eq 0 ]; then
  PROFILES=("2026-07-28" "2025-11-25")
fi
for profile in "${PROFILES[@]}"; do
  case "$profile" in
    2026-07-28|2025-11-25) ;;
    *) echo "unsupported MCP conformance profile: $profile" >&2; exit 2 ;;
  esac
done

if [ -z "$WORK_ROOT" ] || [ -z "$HARNESS_DIR" ] || [ -z "$REPORT_ROOT" ]; then
  echo "MCP conformance work, harness, and report paths must be non-empty" >&2
  exit 2
fi
mkdir -p "$WORK_ROOT"

fixture_alive() {
  [ -n "$fixture_pid" ] || return 1
  kill -0 -- "-$fixture_pid" 2>/dev/null || kill -0 "$fixture_pid" 2>/dev/null
}

signal_fixture() {
  local signal="$1"
  kill "-$signal" -- "-$fixture_pid" 2>/dev/null || kill "-$signal" "$fixture_pid" 2>/dev/null || true
}

stop_fixture() {
  [ -n "$fixture_pid" ] || return 0
  touch "$fixture_dir/stop" 2>/dev/null || true
  for _ in $(seq 1 50); do
    if ! fixture_alive; then break; fi
    sleep 0.1
  done
  if fixture_alive; then
    signal_fixture TERM
  fi
  for _ in $(seq 1 50); do
    if ! fixture_alive; then break; fi
    sleep 0.1
  done
  if fixture_alive; then
    signal_fixture KILL
  fi
  wait "$fixture_pid" 2>/dev/null || true
  fixture_pid=""
}

cleanup() {
  [ "$cleanup_started" -eq 0 ] || return 0
  cleanup_started=1
  stop_fixture
  if [ -n "$fixture_dir" ]; then
    rm -rf "$fixture_dir"
  fi
  if [ -n "$harness_build_dir" ]; then
    rm -rf "$harness_build_dir"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

is_git_worktree() {
  git -C "$1" rev-parse --is-inside-work-tree >/dev/null 2>&1
}

prepare_harness_source() {
  if [ "$HARNESS_DIR_EXTERNAL" -eq 1 ]; then
    if ! is_git_worktree "$HARNESS_DIR"; then
      echo "WEBPI_MCP_CONFORMANCE_HARNESS_DIR must name an existing Git worktree; refusing to modify it: $HARNESS_DIR" >&2
      exit 2
    fi
  elif [ ! -e "$HARNESS_DIR" ]; then
    mkdir -p "$HARNESS_DIR"
    touch "$HARNESS_DIR/$HARNESS_OWNER_MARKER"
    git -C "$HARNESS_DIR" init -q
    git -C "$HARNESS_DIR" remote add origin https://github.com/modelcontextprotocol/conformance.git
  elif ! is_git_worktree "$HARNESS_DIR"; then
    echo "default harness path exists but is not a Git worktree; refusing to remove it: $HARNESS_DIR" >&2
    exit 2
  fi

  if [ "$HARNESS_DIR_EXTERNAL" -eq 0 ]; then
    origin_url="$(git -C "$HARNESS_DIR" remote get-url origin 2>/dev/null || true)"
    case "$origin_url" in
      https://github.com/modelcontextprotocol/conformance.git|https://github.com/modelcontextprotocol/conformance) ;;
      *)
        echo "default harness checkout has an unexpected origin; refusing to modify it: $HARNESS_DIR" >&2
        exit 2
        ;;
    esac
    # Older versions of this script created the same default checkout before the
    # ownership marker existed. Adopt it only when it is already at the pinned
    # commit and has no tracked edits; otherwise ownership is ambiguous and we
    # refuse to mutate it. Externally supplied worktrees are always read-only.
    if [ ! -f "$HARNESS_DIR/$HARNESS_OWNER_MARKER" ]; then
      legacy_actual="$(git -C "$HARNESS_DIR" rev-parse HEAD 2>/dev/null || true)"
      if [ "$legacy_actual" != "$HARNESS_COMMIT" ] || \
         ! git -C "$HARNESS_DIR" diff --quiet --no-ext-diff || \
         ! git -C "$HARNESS_DIR" diff --cached --quiet --no-ext-diff; then
        echo "default harness checkout is unowned and not an exact clean pinned checkout; refusing to modify it: $HARNESS_DIR" >&2
        exit 2
      fi
      touch "$HARNESS_DIR/$HARNESS_OWNER_MARKER"
    fi
  fi

  actual="$(git -C "$HARNESS_DIR" rev-parse HEAD 2>/dev/null || true)"
  if [ "$actual" != "$HARNESS_COMMIT" ]; then
    if [ "$HARNESS_DIR_EXTERNAL" -eq 1 ]; then
      echo "MCP conformance harness must be exactly $HARNESS_COMMIT, got ${actual:-unresolved}; external checkout is never modified" >&2
      exit 2
    fi
    GIT_TERMINAL_PROMPT=0 git -C "$HARNESS_DIR" fetch -q --depth 1 origin "$HARNESS_COMMIT"
    git -C "$HARNESS_DIR" checkout -q --detach FETCH_HEAD
    actual="$(git -C "$HARNESS_DIR" rev-parse HEAD)"
  fi
  if [ "$actual" != "$HARNESS_COMMIT" ]; then
    echo "MCP conformance harness must be exactly $HARNESS_COMMIT, got $actual" >&2
    exit 2
  fi
}

prepare_report_root() {
  if [ -e "$REPORT_ROOT" ] && [ ! -d "$REPORT_ROOT" ]; then
    echo "MCP conformance report root is not a directory: $REPORT_ROOT" >&2
    exit 2
  fi
  if [ ! -e "$REPORT_ROOT" ]; then
    mkdir -p "$REPORT_ROOT"
    touch "$REPORT_ROOT/$REPORT_OWNER_MARKER"
  elif [ ! -f "$REPORT_ROOT/$REPORT_OWNER_MARKER" ]; then
    if find "$REPORT_ROOT" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
      echo "MCP report root is non-empty and lacks the WebPi ownership marker; refusing to delete its contents: $REPORT_ROOT" >&2
      exit 2
    fi
    touch "$REPORT_ROOT/$REPORT_OWNER_MARKER"
  fi
  rm -rf "$REPORT_ROOT/2026-07-28" "$REPORT_ROOT/2025-11-25"
  rm -f "$REPORT_ROOT/fixture.log" "$REPORT_ROOT/server-capabilities.json"
}

prepare_harness_build() {
  harness_build_dir="$(mktemp -d "$WORK_ROOT/harness-build.XXXXXX")"
  git -C "$HARNESS_DIR" archive "$HARNESS_COMMIT" | tar -x -C "$harness_build_dir"
  if [ ! -f "$harness_build_dir/package-lock.json" ]; then
    echo "pinned harness has no package-lock.json" >&2
    exit 2
  fi
  npm --prefix "$harness_build_dir" ci --ignore-scripts --no-audit --no-fund
  npm --prefix "$harness_build_dir" run build
  if [ ! -f "$harness_build_dir/dist/index.js" ]; then
    echo "pinned harness build did not produce dist/index.js" >&2
    exit 2
  fi
}

prepare_harness_source
prepare_report_root
prepare_harness_build
python3 scripts/tests/test_mcp_conformance_report.py

# Compile before starting the bounded readiness clock. A cold WebPi test build
# can take several minutes on small builders; readiness should measure server
# startup, not Rust compilation time.
cargo test --locked -p webcodex --lib mcp_conformance_fixture_server --no-run

fixture_dir="$(mktemp -d "${TMPDIR:-/tmp}/webpi-mcp-conformance.XXXXXX")"
url_file="$fixture_dir/url"
stop_file="$fixture_dir/stop"
fixture_log="$REPORT_ROOT/fixture.log"
# Keep the entire Cargo + test-binary fixture tree in one private process group so
# fallback teardown cannot leave the loopback server alive if Cargo exits first.
WEBPI_MCP_CONFORMANCE_URL_FILE="$url_file" \
WEBPI_MCP_CONFORMANCE_STOP_FILE="$stop_file" \
  python3 -c 'import os, sys; os.setsid(); os.execvp(sys.argv[1], sys.argv[1:])' \
  cargo test --locked -p webcodex --lib mcp_conformance_fixture_server -- --ignored --nocapture \
  >"$fixture_log" 2>&1 &
fixture_pid=$!

ready_deadline=$((SECONDS + 60))
while [ ! -s "$url_file" ]; do
  if ! fixture_alive; then
    echo "MCP conformance fixture exited before publishing its URL" >&2
    tail -80 "$fixture_log" >&2 || true
    exit 1
  fi
  if [ "$SECONDS" -ge "$ready_deadline" ]; then
    echo "timed out waiting for MCP conformance fixture URL" >&2
    tail -80 "$fixture_log" >&2 || true
    exit 1
  fi
  sleep 0.1
done
fixture_url="$(tr -d '\r\n' < "$url_file")"
case "$fixture_url" in
  http://127.0.0.1:*'/mcp') ;;
  *) echo "fixture published unexpected URL: $fixture_url" >&2; exit 1 ;;
esac

capabilities_file="$REPORT_ROOT/server-capabilities.json"
python3 - "$fixture_url" "$capabilities_file" <<'PY'
import json
import pathlib
import sys
import urllib.error
import urllib.request

server_url, output_path = sys.argv[1:]

def rpc_request(payload, headers=None):
    encoded = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    request_headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if headers:
        request_headers.update(headers)
    request = urllib.request.Request(
        server_url,
        data=encoded,
        headers=request_headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            raw = response.read().decode("utf-8")
            content_type = response.headers.get("Content-Type", "")
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise SystemExit(f"capability probe HTTP {exc.code}: {body[:500]}") from exc
    except OSError as exc:
        raise SystemExit(f"capability probe transport error: {exc}") from exc

    messages = []
    if "text/event-stream" in content_type:
        for line in raw.splitlines():
            if not line.startswith("data:"):
                continue
            candidate = line[len("data:"):].strip()
            if candidate:
                messages.append(json.loads(candidate))
    else:
        messages.append(json.loads(raw))
    request_id = payload["id"]
    message = next((item for item in messages if item.get("id") == request_id), None)
    if not isinstance(message, dict):
        raise SystemExit("capability probe did not receive the matching JSON-RPC response")
    if "error" in message:
        raise SystemExit(f"capability probe returned JSON-RPC error: {message['error']}")
    result = message.get("result")
    if not isinstance(result, dict):
        raise SystemExit("capability probe response has no result object")
    capabilities = result.get("capabilities")
    if not isinstance(capabilities, dict):
        raise SystemExit("capability probe result has no capabilities object")
    return capabilities

modern = rpc_request(
    {
        "jsonrpc": "2.0",
        "id": "webcodex-capabilities-2026",
        "method": "server/discover",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
                "io.modelcontextprotocol/clientInfo": {
                    "name": "webcodex-conformance-capability-probe",
                    "version": "1.0",
                },
            }
        },
    },
    {
        "MCP-Protocol-Version": "2026-07-28",
        "Mcp-Method": "server/discover",
    },
)
legacy = rpc_request(
    {
        "jsonrpc": "2.0",
        "id": "webcodex-capabilities-2025",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "webcodex-conformance-capability-probe",
                "version": "1.0",
            },
        },
    }
)

relevant_keys = ("completions", "logging", "prompts", "resources", "tools")
def project(capabilities):
    return {key: capabilities[key] for key in relevant_keys if key in capabilities}

pathlib.Path(output_path).write_text(
    json.dumps(
        {
            "2026-07-28": project(modern),
            "2025-11-25": project(legacy),
        },
        indent=2,
        sort_keys=True,
    ) + "\n",
    encoding="utf-8",
)
PY

server_sha="$(git rev-parse HEAD)"
overall=0
for profile in "${PROFILES[@]}"; do
  profile_root="$REPORT_ROOT/$profile"
  raw="$profile_root/raw"
  mkdir -p "$raw"
  log="$profile_root/harness.log"
  set +e
  node "$harness_build_dir/dist/index.js" server \
    --url "$fixture_url" \
    --requirements "$profile" \
    --output-dir "$raw" \
    --timeout 10000 \
    >"$log" 2>&1
  harness_exit=$?
  set -e

  metadata="$profile_root/metadata.json"
  python3 - "$harness_build_dir/requirements/$profile.yaml" "$metadata" "$profile" \
    "$server_sha" "$HARNESS_COMMIT" "$harness_exit" "$capabilities_file" <<'PY'
import json
import pathlib
import sys

(
    requirements_path,
    output_path,
    profile,
    server_sha,
    harness_sha,
    harness_exit,
    capabilities_path,
) = sys.argv[1:]
capability_map = json.loads(pathlib.Path(capabilities_path).read_text(encoding="utf-8"))
server_capabilities = capability_map.get(profile)
if not isinstance(server_capabilities, dict):
    raise SystemExit(f"missing capability probe result for {profile}")
server = []
not_scored = []
section = None
current_not_scored = None

def retain_not_scored(entry):
    if not entry or entry.get("leg") != "server":
        return
    scenario = entry.get("scenario")
    reason = entry.get("reason")
    if not scenario or not reason:
        raise SystemExit(f"malformed server not_scored entry in {requirements_path}: {entry}")
    not_scored.append({"scenario": scenario, "reason": reason})

for raw in pathlib.Path(requirements_path).read_text(encoding="utf-8").splitlines():
    if raw in {"server:", "client:", "not_scored:"}:
        retain_not_scored(current_not_scored)
        current_not_scored = None
        section = raw[:-1]
        continue
    if raw and not raw.startswith(" "):
        retain_not_scored(current_not_scored)
        current_not_scored = None
        section = None
        continue
    if section == "server" and raw.startswith("  - "):
        server.append(raw[4:].strip())
    elif section == "not_scored":
        if raw.startswith("  - scenario: "):
            retain_not_scored(current_not_scored)
            current_not_scored = {"scenario": raw[len("  - scenario: "):].strip()}
        elif current_not_scored is not None and raw.startswith("    leg: "):
            current_not_scored["leg"] = raw[len("    leg: "):].strip()
        elif current_not_scored is not None and raw.startswith("    reason: "):
            current_not_scored["reason"] = raw[len("    reason: "):].strip()
retain_not_scored(current_not_scored)

if not server:
    raise SystemExit(f"no scored server scenarios found in {requirements_path}")
if len(server) != len(set(server)):
    raise SystemExit(f"duplicate scored server scenarios in {requirements_path}")
not_scored_names = [entry["scenario"] for entry in not_scored]
if len(not_scored_names) != len(set(not_scored_names)):
    raise SystemExit(f"duplicate server not_scored scenarios in {requirements_path}")
if set(server) & set(not_scored_names):
    raise SystemExit(f"server scenario is both scored and not_scored in {requirements_path}")

pathlib.Path(output_path).write_text(
    json.dumps(
        {
            "schema_version": 1,
            "profile": profile,
            "server_sha": server_sha,
            "harness_sha": harness_sha,
            "harness_exit_code": int(harness_exit),
            "server_capabilities": server_capabilities,
            "required_scenarios": server,
            "not_scored_scenarios": not_scored,
        },
        indent=2,
        sort_keys=True,
    ) + "\n",
    encoding="utf-8",
)
PY

  summary="$profile_root/summary.json"
  if ! python3 scripts/mcp_conformance_report.py \
      --reports "$raw" \
      --metadata "$metadata" \
      --baseline "$BASELINE" \
      --profile "$profile" \
      --summary "$summary"; then
    overall=1
  fi
done

exit "$overall"
