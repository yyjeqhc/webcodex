#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# Private Drop — Zero-Config WebSocket Agent E2E Smoke
#
# Starts a real `private-drop` server and a `private-drop-agent` connected over
# the WebSocket transport, then exercises the full GPT Actions + MCP surface via
# curl to prove the runtime is wired end-to-end on a single host.
#
# What this proves:
#   - Server boots with DROP_TOKEN auth and no server-side projects.toml.
#   - Agent registers over WebSocket and announces a project.
#   - listProjects / getRuntimeStatus see the agent-registered project.
#   - readProjectFile / getProjectGitStatus route to the agent.
#   - runCodexTask starts an async job on the agent (using a stub CODEX_BIN,
#     NOT the real Codex CLI) and job status/log round-trip.
#   - MCP initialize / tools/list / tools/call(list_projects) work.
#   - /openapi.json still exposes the expected GPT Actions operation set and
#     omits legacy/admin paths.
#
# What this does NOT do:
#   - It does not touch the real ChatGPT web UI.
#   - It does not invoke the real Codex CLI (a stub binary is used).
#   - It does not implement QUIC.
#
# Environment overrides:
#   E2E_PORT            bind port (default: auto-pick a free port)
#   E2E_TOKEN           Bearer token (default: e2e-smoke-token)
#   E2E_CLIENT_ID       agent client_id (default: e2e-agent)
#   E2E_PROJECT_ID      agent project id (default: smoke-proj)
#   E2E_TRANSPORT       agent transport (default: websocket; polling fallback)
#   E2E_TIMEOUT_SECS    overall wall-clock cap (default: 180)
#   E2E_KEEPALIVE_WAIT_SECS
#                       seconds to idle before the keepalive-online recheck
#                       (default: 2; raise to ~35 to span a real ping/pong)
#   E2E_SKIP_RUN        if set to "1", skip `cargo run` and only syntax-check
#   CARGO_BIN           cargo binary (default: cargo)
#
# Exit codes:
#   0  all smoke checks passed
#   1  one or more checks failed
#   2  environment/dependency error
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

CARGO_BIN="${CARGO_BIN:-cargo}"
TOKEN="${E2E_TOKEN:-e2e-smoke-token}"
CLIENT_ID="${E2E_CLIENT_ID:-e2e-agent}"
PROJECT_ID="${E2E_PROJECT_ID:-smoke-proj}"
TRANSPORT="${E2E_TRANSPORT:-websocket}"
TIMEOUT_SECS="${E2E_TIMEOUT_SECS:-180}"
# Runtime project id exposed by the agent: agent:<client_id>:<project_id>
RUNTIME_PROJECT_ID="agent:${CLIENT_ID}:${PROJECT_ID}"

PASS=0
FAIL=0
SERVER_PID=""
AGENT_PID=""
TMP_ROOT=""
SERVER_LOG=""
AGENT_LOG=""
START_EPOCH=$(date +%s)

# ----------------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------------

log() { printf '[e2e] %s\n' "$*"; }

fail() {
    FAIL=$((FAIL + 1))
    printf '[e2e][FAIL] %s\n' "$*" >&2
}

pass() {
    PASS=$((PASS + 1))
    printf '[e2e][ok]   %s\n' "$*"
}

elapsed() {
    echo $(( $(date +%s) - START_EPOCH ))
}

remaining_time() {
    local used; used=$(elapsed)
    echo $(( TIMEOUT_SECS - used ))
}

# Hard overall deadline: bail out if exceeded.
check_deadline() {
    if [ "$(remaining_time)" -le 0 ]; then
        fail "overall timeout (${TIMEOUT_SECS}s) exceeded"
        cleanup
        print_logs_hint
        exit 1
    fi
}

# Find a free TCP port on 127.0.0.1.
find_free_port() {
    python3 -c "
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
" 2>/dev/null || {
        # Fallback when python3 is unavailable.
        local p
        for p in 18080 18081 18082 18083 18084; do
            if ! (echo >/dev/tcp/127.0.0.1/$p) 2>/dev/null; then
                echo "$p"
                return
            fi
        done
        echo 18080
    }
}

# Wait until a TCP port accepts connections, with a per-call budget.
wait_for_port() {
    local port="$1"
    local budget="${2:-30}"
    local tries=0
    while [ "$tries" -lt "$budget" ]; do
        check_deadline
        if (echo >/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
            return 0
        fi
        tries=$((tries + 1))
        sleep 1
    done
    return 1
}

# curl wrapper with auth + timeout. Prints body to stdout.
api_post() {
    local path="$1"
    local body="${2:-}"
    # Avoid `${2:-{}}` here: bash parses the `}` ambiguously and appends a
    # stray `}` to non-empty bodies, which breaks strict JSON parsing on the
    # server. Default explicitly instead.
    if [ -z "$body" ]; then
        body="{}"
    fi
    curl -sS --max-time 10 \
        -H "Authorization: Bearer ${TOKEN}" \
        -H "Content-Type: application/json" \
        -X POST "http://127.0.0.1:${PORT}${path}" \
        -d "$body" 2>/dev/null
}

api_get() {
    local path="$1"
    curl -sS --max-time 10 \
        -H "Authorization: Bearer ${TOKEN}" \
        "http://127.0.0.1:${PORT}${path}" 2>/dev/null
}

# Extract a JSON field with python3 (no jq dependency required).
json_get() {
    # json_get '<json>' '<dot.path>'
    local json="$1"
    local path="$2"
    python3 - "$json" "$path" <<'PY'
import json, sys
try:
    obj = json.loads(sys.argv[1])
except Exception:
    print("")
    sys.exit(0)
cur = obj
for part in sys.argv[2].split("."):
    if part == "":
        break
    if isinstance(cur, list):
        try:
            cur = cur[int(part)]
        except Exception:
            print("")
            sys.exit(0)
    elif isinstance(cur, dict):
        cur = cur.get(part)
        if cur is None:
            print("")
            sys.exit(0)
    else:
        print("")
        sys.exit(0)
if isinstance(cur, (dict, list)):
    print(json.dumps(cur))
else:
    print(cur if cur is not None else "")
PY
}

# Assert a JSON response has success == true.
assert_success() {
    local label="$1"
    local body="$2"
    local ok
    ok="$(json_get "$body" success)"
    if [ "$ok" = "True" ]; then
        pass "$label"
        return 0
    else
        fail "$label (success != true; body: ${body:0:300})"
        return 1
    fi
}

print_logs_hint() {
    cat >&2 <<EOF

[e2e] ---- log locations ----
[e2e] server log: ${SERVER_LOG:-<none>}
[e2e] agent log:  ${AGENT_LOG:-<none>}
[e2e] temp root:  ${TMP_ROOT:-<none>}
EOF
}

# ----------------------------------------------------------------------------
# Cleanup
# ----------------------------------------------------------------------------

cleanup() {
    trap - INT TERM EXIT
    log "cleaning up background processes"
    if [ -n "${AGENT_PID:-}" ] && kill -0 "$AGENT_PID" 2>/dev/null; then
        kill "$AGENT_PID" 2>/dev/null || true
        sleep 1
        kill -9 "$AGENT_PID" 2>/dev/null || true
    fi
    if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        sleep 1
        kill -9 "$SERVER_PID" 2>/dev/null || true
    fi
    # Wait briefly for the cargo parent processes to tear down children.
    sleep 1
}

trap cleanup INT TERM EXIT

# ----------------------------------------------------------------------------
# Dependency checks
# ----------------------------------------------------------------------------

if ! command -v curl >/dev/null 2>&1; then
    echo "[e2e] curl is required" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "[e2e] python3 is required (for JSON checks and port discovery)" >&2
    exit 2
fi
if ! command -v git >/dev/null 2>&1; then
    echo "[e2e] git is required" >&2
    exit 2
fi

if [ "${E2E_SKIP_RUN:-0}" = "1" ]; then
    log "E2E_SKIP_RUN=1: skipping execution (syntax-only)"
    exit 0
fi

# ----------------------------------------------------------------------------
# 1. Pick a port, then build the temporary runtime layout
# ----------------------------------------------------------------------------

PORT="${E2E_PORT:-$(find_free_port)}"
BASE="http://127.0.0.1:${PORT}"

TMP_ROOT="$(mktemp -d -t private-drop-e2e-XXXXXX)"
DATA_DIR="$TMP_ROOT/data"
PROJECTS_DIR="$TMP_ROOT/projects.d"
CODEX_STUB="$TMP_ROOT/codex-stub.sh"
AGENT_TOML="$TMP_ROOT/agent.toml"
TEST_REPO="$TMP_ROOT/smoke-repo"
SERVER_LOG="$TMP_ROOT/server.log"
AGENT_LOG="$TMP_ROOT/agent.log"

mkdir -p "$DATA_DIR" "$PROJECTS_DIR" "$TEST_REPO"
log "temp root: $TMP_ROOT"

# A stub Codex CLI. The server builds a command like:
#   <CODEX_BIN> [--approval-mode <mode>] <prompt>
# `--approval-mode` is only emitted when CODEX_APPROVAL_MODE (or the request
# approval_mode) is a non-disabled value. The stub just echoes and exits 0 so
# the job completes successfully without depending on the real Codex CLI.
cat > "$CODEX_STUB" <<'STUB'
#!/usr/bin/env bash
# Private Drop E2E stub for CODEX_BIN. NOT the real Codex CLI.
echo "codex-stub: invoked with $# arg(s)"
echo "codex-stub: prompt preview: ${*: -1}"
echo "codex-stub: completed"
exit 0
STUB
chmod +x "$CODEX_STUB"

# Initialize a tiny git repo as the agent project so git_status works.
(
    cd "$TEST_REPO"
    git init -b main >/dev/null 2>&1
    git config user.email "e2e@test.local"
    git config user.name "E2E Smoke"
    printf '# Smoke Project\n\nUsed by the private-drop E2E harness.\n' > README.md
    printf 'fn main() { println!("smoke"); }\n' > src.rs 2>/dev/null || {
        mkdir -p src
        printf 'fn main() { println!("smoke"); }\n' > src/main.rs
    }
    git add . >/dev/null 2>&1
    git commit -m "smoke init" >/dev/null 2>&1 || true
)

# Agent-side project file describing the local repo.
cat > "$PROJECTS_DIR/${PROJECT_ID}.toml" <<EOF
id = "${PROJECT_ID}"
path = "${TEST_REPO}"
name = "Smoke Project"
allow_patch = true
kind = "rust"
description = "E2E smoke project"
EOF

# Agent config: WebSocket preferred transport. owner is arbitrary because
# DROP_TOKEN auth marks the principal as bootstrap (any owner allowed).
cat > "$AGENT_TOML" <<EOF
server_url = "http://127.0.0.1:${PORT}"
token = "${TOKEN}"
client_id = "${CLIENT_ID}"
display_name = "E2E Agent"
owner = "e2e"
projects_dir = "${PROJECTS_DIR}"
poll_interval_ms = 500
transport = "${TRANSPORT}"

[policy]
allow_raw_shell = true
allow_cwd_anywhere = true
max_timeout_secs = 60
max_output_bytes = 262144
EOF

log "using port: $PORT"
log "transport: $TRANSPORT"
log "runtime project id: $RUNTIME_PROJECT_ID"

# ----------------------------------------------------------------------------
# 3. Start the server
# ----------------------------------------------------------------------------

log "starting server (cargo run --bin private-drop)"
DROP_ADDR="127.0.0.1:${PORT}" \
DROP_DATA="$DATA_DIR" \
DROP_TOKEN="$TOKEN" \
CODEX_BIN="$CODEX_STUB" \
CODEX_DEFAULT_TIMEOUT_SECS="30" \
CODEX_APPROVAL_MODE="full-auto" \
RUST_LOG="info" \
"$CARGO_BIN" run --quiet --bin private-drop >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

if ! wait_for_port "$PORT" 40; then
    fail "server did not start listening on $PORT within budget"
    print_logs_hint
    exit 1
fi
pass "server listening on $PORT"

# ----------------------------------------------------------------------------
# 4. Start the agent
# ----------------------------------------------------------------------------

log "starting agent (cargo run --bin private-drop-agent, transport=$TRANSPORT)"
"$CARGO_BIN" run --quiet --bin private-drop-agent -- --config "$AGENT_TOML" >"$AGENT_LOG" 2>&1 &
AGENT_PID=$!

# Wait for the agent to register by polling runtime_status for the client.
log "waiting for agent registration..."
REGISTERED=0
for _ in $(seq 1 60); do
    check_deadline
    body="$(api_post /api/runtime/status '{}' || true)"
    agent_count="$(json_get "$body" output.agents.count)"
    if [ "$agent_count" = "1" ]; then
        REGISTERED=1
        break
    fi
    sleep 1
done

if [ "$REGISTERED" -ne 1 ]; then
    fail "agent did not register within budget"
    print_logs_hint
    exit 1
fi
pass "agent registered (transport=$TRANSPORT)"

# ----------------------------------------------------------------------------
# 4b. Keepalive liveness smoke
# ----------------------------------------------------------------------------
# After a brief idle period the agent must still report online. This is a
# light regression guard for the WebSocket ping/pong liveness fix: a
# connected-but-idle agent must not decay to stale merely because no job
# requests are flowing. (The full 60s online window is exercised by unit
# tests via last_seen injection; here we only confirm no immediate drop so
# the default e2e stays fast. Override the wait with
# E2E_KEEPALIVE_WAIT_SECS, e.g. 35 to span one real ping/pong cycle.)
KEEPALIVE_WAIT="${E2E_KEEPALIVE_WAIT_SECS:-2}"
log "keepalive liveness check (idle ${KEEPALIVE_WAIT}s)"
sleep "$KEEPALIVE_WAIT"
check_deadline
body="$(api_post /api/runtime/status '{}' || true)"
agent_connected="$(json_get "$body" output.agents.clients.0.connected)"
agent_status="$(json_get "$body" output.agents.clients.0.status)"
agent_transport="$(json_get "$body" output.agents.clients.0.transport)"
if [ "$agent_connected" = "True" ] && [ "$agent_status" = "online" ]; then
    pass "agent still online after idle wait (transport=$agent_transport)"
else
    fail "agent went stale after idle wait (connected=$agent_connected status=$agent_status transport=$agent_transport)"
fi

# ----------------------------------------------------------------------------
# 5. GPT Actions surface smoke
# ----------------------------------------------------------------------------

log "---- GPT Actions surface ----"

# getRuntimeStatus
body="$(api_post /api/runtime/status '{}')"
assert_success "getRuntimeStatus" "$body" || true

# listProjects — must include the agent-registered project id.
body="$(api_post /api/projects/list '{}')"
assert_success "listProjects" "$body" || true
# Verify the runtime project id appears in the list.
list_json="$(json_get "$body" output)"
if echo "$list_json" | grep -q "\"$RUNTIME_PROJECT_ID\""; then
    pass "listProjects contains $RUNTIME_PROJECT_ID"
else
    fail "listProjects did not contain $RUNTIME_PROJECT_ID (got: ${list_json:0:200})"
fi

# getProjectGitStatus — routes to the agent, runs `git status --porcelain`.
body="$(api_post /api/projects/git_status "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
assert_success "getProjectGitStatus" "$body" || true

# readProjectFile — reads README.md through the agent.
body="$(api_post /api/projects/read_file "{\"project\":\"$RUNTIME_PROJECT_ID\",\"path\":\"README.md\"}")"
assert_success "readProjectFile(README.md)" "$body" || true
readme_content="$(json_get "$body" output.content)"
if echo "$readme_content" | grep -q "Smoke Project"; then
    pass "readProjectFile returns README content"
else
    fail "readProjectFile content mismatch (got: ${readme_content:0:120})"
fi

# getProjectGitDiff — routes to the agent, runs `git diff`.
body="$(api_post /api/projects/git_diff "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
assert_success "getProjectGitDiff" "$body" || true

# runProjectShellCommand — runs `echo hi` through the agent.
body="$(api_post /api/projects/run_shell "{\"project\":\"$RUNTIME_PROJECT_ID\",\"command\":\"echo hi\"}")"
assert_success "runProjectShellCommand" "$body" || true
shell_stdout="$(json_get "$body" output.stdout)"
if echo "$shell_stdout" | grep -q "hi"; then
    pass "runProjectShellCommand returns echo output"
else
    fail "runProjectShellCommand output mismatch (got: ${shell_stdout:0:120})"
fi

# runCodexTask — starts an async job on the agent using the stub CODEX_BIN.
body="$(api_post /api/codex/run "{\"project\":\"$RUNTIME_PROJECT_ID\",\"prompt\":\"Summarize this repo in one line.\",\"timeout_secs\":20}")"
assert_success "runCodexTask" "$body" || true
JOB_ID="$(json_get "$body" output.job_id)"
if [ -z "$JOB_ID" ] || [ "$JOB_ID" = "" ]; then
    fail "runCodexTask did not return a job_id (body: ${body:0:300})"
else
    pass "runCodexTask returned job_id=$JOB_ID"
fi

# Poll job status until terminal.
JOB_TERMINAL=0
for _ in $(seq 1 40); do
    check_deadline
    body="$(api_post /api/jobs/status "{\"job_id\":\"$JOB_ID\"}")"
    status="$(json_get "$body" output.status)"
    case "$status" in
        completed|failed|stopped|lost)
            JOB_TERMINAL=1
            break
            ;;
        *)
            sleep 1
            ;;
    esac
done

if [ "$JOB_TERMINAL" -ne 1 ]; then
    fail "job $JOB_ID did not reach a terminal status in time"
else
    if [ "$status" = "completed" ]; then
        pass "job $JOB_ID reached terminal status: $status"
    else
        fail "job $JOB_ID reached terminal status: $status (expected completed)"
    fi
fi

# getRuntimeJobLog — read bounded stdout for the job.
body="$(api_post /api/jobs/log "{\"job_id\":\"$JOB_ID\"}")"
assert_success "getRuntimeJobLog" "$body" || true
log_stdout="$(json_get "$body" output.stdout)"
if echo "$log_stdout" | grep -q "codex-stub"; then
    pass "getRuntimeJobLog contains stub output"
else
    fail "getRuntimeJobLog did not contain stub output (got: ${log_stdout:0:160})"
fi

# ----------------------------------------------------------------------------
# 6. MCP surface smoke
# ----------------------------------------------------------------------------

log "---- MCP surface (/mcp) ----"

# initialize
body="$(api_post /mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}')"
proto="$(json_get "$body" result.protocolVersion)"
if [ -n "$proto" ] && [ "$proto" != "" ]; then
    pass "MCP initialize returns protocolVersion=$proto"
else
    fail "MCP initialize did not return a protocolVersion (body: ${body:0:300})"
fi

# tools/list
body="$(api_post /mcp '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')"
TOOLS_LIST_BODY="$body"
tools_count="$(echo "$body" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d.get("result",{}).get("tools",[])))' 2>/dev/null || echo 0)"
if [ "${tools_count:-0}" -gt 0 ]; then
    pass "MCP tools/list returned $tools_count tools"
else
    fail "MCP tools/list returned no tools (body: ${body:0:300})"
fi

# tools/call list_projects — must return structuredContent with the agent project.
body="$(api_post /mcp '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}')"
sc="$(json_get "$body" result.structuredContent)"
sc_success="$(json_get "$sc" success)"
if [ "$sc_success" = "True" ]; then
    pass "MCP tools/call(list_projects) returns structuredContent.success=true"
else
    fail "MCP tools/call(list_projects) structuredContent not success (body: ${body:0:300})"
fi
sc_output="$(json_get "$sc" output)"
if echo "$sc_output" | grep -q "$RUNTIME_PROJECT_ID"; then
    pass "MCP list_projects sees agent project $RUNTIME_PROJECT_ID"
else
    fail "MCP list_projects did not see $RUNTIME_PROJECT_ID (got: ${sc_output:0:200})"
fi

# ----------------------------------------------------------------------------
# 6b. Phase A read-only console tools (REST + MCP) against the agent project
# ----------------------------------------------------------------------------

log "---- Phase A read-only console tools ----"

# list_project_files via REST — must return a bounded entries array that
# includes README.md (the smoke project always has one).
body="$(api_post /api/projects/list_files "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "list_project_files returns success"
else
    fail "list_project_files did not return success (body: ${body:0:300})"
fi
lpf_entries="$(json_get "$body" output.entries)"
if echo "$lpf_entries" | grep -q "README.md"; then
    pass "list_project_files includes README.md"
else
    fail "list_project_files did not include README.md (got: ${lpf_entries:0:200})"
fi

# search_project_text via REST — must find a bounded match in README.md.
body="$(api_post /api/projects/search_text "{\"project\":\"$RUNTIME_PROJECT_ID\",\"pattern\":\"smoke\",\"limit\":10}")"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "search_project_text returns success"
else
    fail "search_project_text did not return success (body: ${body:0:300})"
fi
spt_count="$(json_get "$body" output.count)"
if [ "${spt_count:-0}" -ge 1 ] 2>/dev/null; then
    pass "search_project_text found $spt_count match(es)"
else
    fail "search_project_text found no matches (got: ${body:0:200})"
fi

# git_diff_summary via REST — read-only; must return porcelain + changed_files.
body="$(api_post /api/projects/git_diff_summary "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "git_diff_summary returns success"
else
    fail "git_diff_summary did not return success (body: ${body:0:300})"
fi
gds_porcelain="$(json_get "$body" output.porcelain)"
gds_changed="$(json_get "$body" output.changed_files)"
if [ "$(json_get "$body" output.changed_files_count)" != "None" ]; then
    pass "git_diff_summary returns changed_files_count"
else
    fail "git_diff_summary missing changed_files_count (got: ${body:0:200})"
fi

# list_jobs via REST — bounded summaries, never stdout/stderr bodies.
body="$(api_post /api/jobs/list '{}')"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "list_jobs returns success"
else
    fail "list_jobs did not return success (body: ${body:0:300})"
fi
lj_serialized="$(json_get "$body" output.jobs)"
if ! echo "$lj_serialized" | grep -qi "stdout\|stderr"; then
    pass "list_jobs summaries omit stdout/stderr bodies"
else
    fail "list_jobs summaries leaked stdout/stderr (got: ${lj_serialized:0:200})"
fi

# job_tail via REST for the completed codex job — bounded tail.
if [ -n "$JOB_ID" ]; then
    body="$(api_post /api/jobs/tail "{\"job_id\":\"$JOB_ID\",\"tail_lines\":50}")"
    if [ "$(json_get "$body" success)" = "True" ]; then
        pass "job_tail returns success"
    else
        fail "job_tail did not return success (body: ${body:0:300})"
    fi
else
    fail "job_tail skipped: no JOB_ID available"
fi

# MCP tools/list must now expose the Phase A tool names.
phase_a_present=1
for tname in list_project_files search_project_text git_diff_summary list_jobs job_tail; do
    if echo "$TOOLS_LIST_BODY" | grep -q "\"$tname\""; then
        :
    else
        phase_a_present=0
        fail "MCP tools/list missing $tname"
    fi
done
if [ "$phase_a_present" = "1" ]; then
    pass "MCP tools/list exposes all Phase A console tools"
fi

# ----------------------------------------------------------------------------
# 7. GPT Actions schema smoke (/openapi.json)
# ----------------------------------------------------------------------------

log "---- GPT Actions schema (/openapi.json) ----"

SCHEMA="$(api_get /openapi.json)"
python3 - "$SCHEMA" "$RUNTIME_PROJECT_ID" <<'PY'
import json, sys
schema = json.loads(sys.argv[1])
errors = []

# Collect operation ids.
ops = []
for path, methods in schema.get("paths", {}).items():
    for method, op in methods.items():
        ops.append(op.get("operationId"))
ops_set = set(ops)

expected_ops = {
    "listRuntimeTools", "listProjects", "getRuntimeStatus",
    "runCodexTask", "getRuntimeJobStatus", "getRuntimeJobLog",
    "readProjectFile", "getProjectGitStatus", "getProjectGitDiff",
    "applyProjectPatch", "runProjectShellCommand", "callRuntimeTool",
}
missing = expected_ops - ops_set
extra = ops_set - expected_ops
if missing:
    errors.append(f"missing operationIds: {sorted(missing)}")
if extra:
    errors.append(f"unexpected operationIds: {sorted(extra)}")

# Forbidden legacy/admin paths must not appear in the schema paths.
forbidden = ["/api/audit/sessions", "/api/audit/session", "/api/audit/stats",
             "/api/jobs/stop"]
paths = set(schema.get("paths", {}).keys())
for fp in forbidden:
    if fp in paths:
        errors.append(f"forbidden path present in schema: {fp}")

# Legacy /api/codex/* sub-routes (context/edit/git/job/...) must not appear.
# /api/codex/run is the legitimate runCodexTask GPT Action and is allowed.
legacy_codex = ["/api/codex/command_request_op", "/api/codex/command_request",
                "/api/codex/context", "/api/codex/context_batch",
                "/api/codex/apply_patch", "/api/codex/edit",
                "/api/codex/artifact", "/api/codex/git",
                "/api/codex/job", "/api/codex/report",
                "/api/codex/projects"]
for p in paths:
    if p in legacy_codex:
        errors.append(f"legacy codex path present in schema: {p}")

# Descriptions must not claim server-side projects.toml is the runtime source.
blob = json.dumps(schema)
if "projects.toml" in blob and "runtime project source" in blob.lower():
    errors.append("schema mentions projects.toml as runtime project source")

# Every path must be POST-only.
for path, methods in schema.get("paths", {}).items():
    for method in methods:
        if method != "post":
            errors.append(f"non-POST method '{method}' on path {path}")

if errors:
    print("FAIL")
    for e in errors:
        print("  - " + e, file=sys.stderr)
    sys.exit(1)
print(f"OK ops={len(ops_set)} paths={len(paths)}")
PY
if [ $? -eq 0 ]; then
    pass "/openapi.json operation set + POST-only + no legacy/admin paths"
else
    fail "/openapi.json schema checks failed (see stderr above)"
fi

# ----------------------------------------------------------------------------
# 7b. MCP App console (Phase B) — public static entry + protected data API
# ----------------------------------------------------------------------------

log "---- MCP App console (/console) ----"

# The console HTML shell is public (no Bearer auth) and must reference the
# bundled assets. It never embeds the token.
console_html="$(curl -sS --max-time 10 "http://127.0.0.1:${PORT}/console" 2>/dev/null)"
if echo "$console_html" | grep -q "Runtime Console" && \
   echo "$console_html" | grep -q "/console/app.js"; then
    pass "GET /console serves public HTML shell"
else
    fail "GET /console did not return expected HTML shell (got: ${console_html:0:200})"
fi

# The bundled JS is public and must call the protected status endpoint.
console_js="$(curl -sS --max-time 10 "http://127.0.0.1:${PORT}/console/app.js" 2>/dev/null)"
if echo "$console_js" | grep -q "/api/runtime/status"; then
    pass "GET /console/app.js references status endpoint"
else
    fail "GET /console/app.js missing status endpoint reference (got: ${console_js:0:200})"
fi

# The bundle must never embed the DROP_TOKEN env var name in the DOM.
if echo "$console_html" | grep -qi "drop_token"; then
    fail "console HTML leaked DROP_TOKEN literal"
else
    pass "console HTML does not leak DROP_TOKEN literal"
fi

# The protected data API must still reject unauthenticated requests even though
# the console page itself is public.
no_auth_status=$(curl -sS -o /dev/null -w "%{http_code}" --max-time 10 \
    -H "Content-Type: application/json" \
    -X POST "http://127.0.0.1:${PORT}/api/runtime/status" \
    -d '{}' 2>/dev/null)
if [ "$no_auth_status" = "401" ]; then
    pass "POST /api/runtime/status rejects unauthenticated request (401)"
else
    fail "POST /api/runtime/status without token returned HTTP ${no_auth_status} (expected 401)"
fi

# runtime_status now carries per-agent last_seen + stale_count for the console.
status_body="$(api_post /api/runtime/status '{}')"
if [ "$(json_get "$status_body" output.agents.stale_count)" != "None" ]; then
    pass "runtime_status exposes agents.stale_count"
else
    fail "runtime_status missing agents.stale_count"
fi

# ----------------------------------------------------------------------------
# 8. Summary
# ----------------------------------------------------------------------------

log "---- summary ----"
log "passed: $PASS"
log "failed: $FAIL"
log "elapsed: $(elapsed)s / ${TIMEOUT_SECS}s"

if [ "$FAIL" -ne 0 ]; then
    print_logs_hint
    exit 1
fi

log "E2E smoke PASSED"
exit 0
