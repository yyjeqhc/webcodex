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
# Phase 4: MCP tools/list must expose replace_in_file / write_project_file and
# the count must be 25 (23 Phase 3 tools + 2 Phase 4 structured-edit tools).
if [ "${tools_count:-0}" = "25" ]; then
    pass "MCP tools/list count is 25 (Phase 4 parity)"
else
    fail "MCP tools/list count expected 25, got ${tools_count:-0}"
fi
for tname in replace_in_file write_project_file; do
    if echo "$TOOLS_LIST_BODY" | grep -q "\"$tname\""; then
        pass "MCP tools/list exposes $tname"
    else
        fail "MCP tools/list missing $tname"
    fi
done

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
# 6c. validate_patch (patch preflight / dry-run) against the agent project
# ----------------------------------------------------------------------------

log "---- validate_patch (patch preflight) ----"

# Build a JSON request body containing a patch, using python3 for safe
# JSON string escaping (patches contain newlines and special chars).
build_validate_body() {
    local patch="$1"
    python3 -c '
import json, sys
print(json.dumps({"project": sys.argv[1], "patch": sys.argv[2]}))
' "$RUNTIME_PROJECT_ID" "$patch"
}

# A patch that creates a new file — always applies cleanly to a clean repo.
GOOD_PATCH='diff --git a/VALIDATE_PROBE.md b/VALIDATE_PROBE.md
new file mode 100644
--- /dev/null
+++ b/VALIDATE_PROBE.md
@@ -0,0 +1 @@
+preflight
'

# A patch whose context does not match — cannot apply.
BAD_PATCH='--- a/README.md
+++ b/README.md
@@ -1,1 +1,1 @@
-NONEXISTENT_CONTEXT_LINE
+replacement
'

# Capture the worktree state before validation (should be clean: committed).
pre_status="$(api_post /api/projects/git_status "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
pre_porcelain="$(json_get "$pre_status" output.stdout)"

# validate_patch with an applicable patch — can_apply must be true.
good_body="$(build_validate_body "$GOOD_PATCH")"
body="$(api_post /api/projects/validate_patch "$good_body")"
vp_success="$(json_get "$body" success)"
vp_can_apply="$(json_get "$body" output.can_apply)"
if [ "$vp_success" = "True" ] && [ "$vp_can_apply" = "True" ]; then
    pass "validate_patch(applicable) returns can_apply=true"
else
    fail "validate_patch(applicable) did not return can_apply=true (success=$vp_success can_apply=$vp_can_apply body=${body:0:300})"
fi
vp_affected="$(json_get "$body" output.affected_files)"
if echo "$vp_affected" | grep -q "VALIDATE_PROBE.md"; then
    pass "validate_patch(applicable) returns affected_files"
else
    fail "validate_patch(applicable) missing affected_files (got: ${vp_affected:0:200})"
fi
vp_stat="$(json_get "$body" output.stat)"
if [ -n "$vp_stat" ] && [ "$vp_stat" != "None" ] && [ "$vp_stat" != "" ]; then
    pass "validate_patch(applicable) returns stat"
else
    fail "validate_patch(applicable) missing stat (got: ${vp_stat:0:200})"
fi

# validate_patch with a patch larger than the shell command limit. The patch is
# sent to the agent as stdin, not embedded in the command string.
LARGE_PATCH="$(python3 - <<'PY'
print("diff --git a/LARGE_VALIDATE_PROBE.md b/LARGE_VALIDATE_PROBE.md")
print("new file mode 100644")
print("--- /dev/null")
print("+++ b/LARGE_VALIDATE_PROBE.md")
print("@@ -0,0 +1,220 @@")
for i in range(220):
    print(f"+line-{i:03d}-" + ("x" * 48))
PY
)"
LARGE_PATCH="${LARGE_PATCH}"$'\n'
large_body="$(build_validate_body "$LARGE_PATCH")"
body="$(api_post /api/projects/validate_patch "$large_body")"
vp_large_success="$(json_get "$body" success)"
vp_large_can_apply="$(json_get "$body" output.can_apply)"
if [ "$vp_large_success" = "True" ] && [ "$vp_large_can_apply" = "True" ]; then
    pass "validate_patch handles patch larger than command limit"
else
    fail "validate_patch large patch failed (success=$vp_large_success can_apply=$vp_large_can_apply body=${body:0:300})"
fi

# validate_patch with a non-applicable patch — can_apply must be false.
bad_body="$(build_validate_body "$BAD_PATCH")"
body="$(api_post /api/projects/validate_patch "$bad_body")"
vp2_success="$(json_get "$body" success)"
vp2_can_apply="$(json_get "$body" output.can_apply)"
if [ "$vp2_success" = "True" ] && [ "$vp2_can_apply" = "False" ]; then
    pass "validate_patch(non-applicable) returns can_apply=false"
else
    fail "validate_patch(non-applicable) did not return can_apply=false (success=$vp2_success can_apply=$vp2_can_apply body=${body:0:300})"
fi

# Worktree must be unchanged after both validations (dry-run never writes).
post_status="$(api_post /api/projects/git_status "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
post_porcelain="$(json_get "$post_status" output.stdout)"
if [ "$pre_porcelain" = "$post_porcelain" ] && \
   ! echo "$post_porcelain" | grep -q "VALIDATE_PROBE" && \
   ! echo "$post_porcelain" | grep -q "LARGE_VALIDATE_PROBE"; then
    pass "validate_patch does not modify the worktree"
else
    fail "validate_patch modified the worktree (pre=${pre_porcelain:0:120} post=${post_porcelain:0:120})"
fi

# MCP tools/list must expose validate_patch.
if echo "$TOOLS_LIST_BODY" | grep -q '"validate_patch"'; then
    pass "MCP tools/list exposes validate_patch"
else
    fail "MCP tools/list missing validate_patch"
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
    "getProjectGitDiffSummary", "listProjectFiles", "searchProjectText",
    "validateProjectPatch", "applyProjectPatch", "applyProjectPatchChecked",
    "runProjectShellCommand", "deleteProjectFiles", "gitRestorePaths",
    "discardUntrackedFiles", "listRuntimeJobs", "getRuntimeJobTail",
    "callRuntimeTool",
}
missing = expected_ops - ops_set
extra = ops_set - expected_ops
if missing:
    errors.append(f"missing operationIds: {sorted(missing)}")
if extra:
    errors.append(f"unexpected operationIds: {sorted(extra)}")

# Phase 3: operation count must stay small (<= 30) and exactly 22 this phase.
if len(ops_set) > 30:
    errors.append(f"too many operations: {len(ops_set)} (must be <= 30)")
if len(ops_set) != 22:
    errors.append(f"operation count must be 22 this phase, got {len(ops_set)}")

# Phase 2: each operation description must fit the <= 300 char budget.
for path, methods in schema.get("paths", {}).items():
    for method, op in methods.items():
        desc = op.get("description", "") or ""
        if len(desc) > 300:
            errors.append(
                f"{method} {path} operationId {op.get('operationId')} "
                f"description too long: {len(desc)} chars"
            )

# Forbidden legacy/admin/internal paths must not appear in the schema paths.
# Phase 3 promotes validate_patch, list_files, search_text, git_diff_summary,
# jobs/list, and jobs/tail to dedicated GPT Actions, so they are no longer
# forbidden. jobs/stop, audit, shell, codex legacy, console, and /mcp remain
# forbidden.
forbidden = ["/api/audit/sessions", "/api/audit/session", "/api/audit/stats",
             "/api/jobs/stop",
             "/api/projects/replace_in_file", "/api/projects/write_file",
             "/api/messages", "/api/files", "/api/desktop/task_op", "/api/desktop/task",
             "/api/shell/run", "/api/shell/job", "/api/shell/file",
             "/mcp", "/openapi.json", "/console", "/console/app.js", "/console/styles.css"]
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
# 7c. Phase 2: generic callRuntimeTool / /api/tools/list enhancements
# ----------------------------------------------------------------------------

log "---- Phase 2: callRuntimeTool / tools/list ----"

# /api/tools/list must return names + count alongside the back-compat tools array.
body="$(api_post /api/tools/list '{}')"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "/api/tools/list returns success"
else
    fail "/api/tools/list did not return success (body: ${body:0:300})"
fi
tl_names="$(json_get "$body" names)"
tl_count="$(json_get "$body" count)"
tl_tools_count="$(echo "$body" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d.get("output",{}).get("tools",[]) if isinstance(d.get("output"),dict) else d.get("tools",[])))' 2>/dev/null || echo 0)"
# tools array is top-level in the tools/list response.
tl_tools_count="$(echo "$body" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d.get("tools",[])))' 2>/dev/null || echo 0)"
if [ "${tl_count:-0}" -gt 0 ] 2>/dev/null && [ "${tl_count}" = "${tl_tools_count}" ]; then
    pass "/api/tools/list names/count match tools array (count=$tl_count)"
else
    fail "/api/tools/list names/count mismatch (count=$tl_count tools=$tl_tools_count body=${body:0:200})"
fi
if echo "$tl_names" | grep -q "git_diff_summary" && echo "$tl_names" | grep -q "list_tools"; then
    pass "/api/tools/list names include git_diff_summary and list_tools"
else
    fail "/api/tools/list names missing expected tools (got: ${tl_names:0:200})"
fi
if [ "$(json_get "$body" categories)" != "None" ] && [ "$(json_get "$body" recommended_flows)" != "None" ]; then
    pass "/api/tools/list includes categories and recommended_flows"
else
    fail "/api/tools/list missing categories/recommended_flows (body: ${body:0:200})"
fi

# callRuntimeTool: params omitted -> list_tools succeeds.
body="$(api_post /api/tools/call '{"tool":"list_tools"}')"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "callRuntimeTool(list_tools) params omitted succeeds"
else
    fail "callRuntimeTool(list_tools) params omitted failed (body: ${body:0:300})"
fi

# callRuntimeTool: params null -> list_tools succeeds.
body="$(api_post /api/tools/call '{"tool":"list_tools","params":null}')"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "callRuntimeTool(list_tools) params null succeeds"
else
    fail "callRuntimeTool(list_tools) params null failed (body: ${body:0:300})"
fi

# callRuntimeTool: arguments alias -> list_tools succeeds.
body="$(api_post /api/tools/call '{"tool":"list_tools","arguments":null}')"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "callRuntimeTool(list_tools) arguments alias succeeds"
else
    fail "callRuntimeTool(list_tools) arguments alias failed (body: ${body:0:300})"
fi

# callRuntimeTool: git_diff_summary against the agent project succeeds.
body="$(api_post /api/tools/call "{\"tool\":\"git_diff_summary\",\"params\":{\"project\":\"$RUNTIME_PROJECT_ID\"}}")"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "callRuntimeTool(git_diff_summary) routes to agent and succeeds"
else
    fail "callRuntimeTool(git_diff_summary) failed (body: ${body:0:300})"
fi

# callRuntimeTool: unknown tool returns a useful error (not a 5xx / empty).
body="$(api_post /api/tools/call '{"tool":"definitely_not_a_tool"}')"
unk_err="$(json_get "$body" error)"
if [ -n "$unk_err" ] && [ "$unk_err" != "None" ] && \
   echo "$unk_err" | grep -q "definitely_not_a_tool" && \
   (echo "$unk_err" | grep -q "listRuntimeTools" || echo "$unk_err" | grep -q "list_tools"); then
    pass "callRuntimeTool(unknown tool) returns useful discovery hint"
else
    fail "callRuntimeTool(unknown tool) error not useful (got: ${unk_err:0:200})"
fi

# ----------------------------------------------------------------------------
# 7d. Phase 3: dedicated mutation actions (apply_patch_checked, delete_files,
#     git_restore_paths, discard_untracked) against probe files only
# ----------------------------------------------------------------------------
#
# These are executable mutations with side effects. To avoid breaking the
# smoke repo, every probe operates ONLY on throwaway probe files inside the
# temporary TEST_REPO (never on README.md, src.rs, or any real project file).
# Probe files are removed afterwards so the worktree returns to a clean state.

log "---- Phase 3: dedicated mutation actions (probe files only) ----"

# Build a JSON request body with python3 for safe escaping. The argument is a
# JSON string that is parsed and re-serialized (validates + normalizes).
build_body() {
    python3 -c '
import json, sys
obj = json.loads(sys.argv[1])
print(json.dumps(obj))
' "$1"
}

# applyProjectPatchChecked — apply a probe patch that creates a new file,
# then verify via git_diff_summary that the probe file appears as untracked.
PROBE_PATCH='diff --git a/APPLY_CHECKED_PROBE.txt b/APPLY_CHECKED_PROBE.txt
new file mode 100644
--- /dev/null
+++ b/APPLY_CHECKED_PROBE.txt
@@ -0,0 +1 @@
+probe
'
apc_body="$(python3 -c '
import json, sys
print(json.dumps({"project": sys.argv[1], "patch": sys.argv[2]}))
' "$RUNTIME_PROJECT_ID" "$PROBE_PATCH")"
body="$(api_post /api/projects/apply_patch_checked "$apc_body")"
apc_success="$(json_get "$body" success)"
if [ "$apc_success" = "True" ]; then
    pass "applyProjectPatchChecked(probe) returns success"
else
    fail "applyProjectPatchChecked(probe) failed (body: ${body:0:300})"
fi
# Verify the probe file now shows up in the worktree via git_diff_summary.
body="$(api_post /api/projects/git_diff_summary "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
gds_changed="$(json_get "$body" output.changed_files)"
if echo "$gds_changed" | grep -q "APPLY_CHECKED_PROBE.txt"; then
    pass "applyProjectPatchChecked probe file visible in git_diff_summary"
else
    fail "applyProjectPatchChecked probe file not in diff summary (got: ${gds_changed:0:200})"
fi

# deleteProjectFiles — delete the probe file created above.
del_body="$(build_body "{\"project\":\"$RUNTIME_PROJECT_ID\",\"paths\":[\"APPLY_CHECKED_PROBE.txt\"]}")"
body="$(api_post /api/projects/delete_files "$del_body")"
del_success="$(json_get "$body" success)"
if [ "$del_success" = "True" ]; then
    pass "deleteProjectFiles(probe) returns success"
else
    fail "deleteProjectFiles(probe) failed (body: ${body:0:300})"
fi
# Verify the probe file is gone via list_files root listing.
body="$(api_post /api/projects/list_files "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
lpf_entries="$(json_get "$body" output.entries)"
if ! echo "$lpf_entries" | grep -q "APPLY_CHECKED_PROBE.txt"; then
    pass "deleteProjectFiles removed probe file"
else
    fail "deleteProjectFiles did not remove probe file (got: ${lpf_entries:0:200})"
fi

# discardUntrackedFiles — create a fresh untracked probe file, then discard it.
body="$(api_post /api/projects/run_shell "{\"project\":\"$RUNTIME_PROJECT_ID\",\"command\":\"printf probe > UNTRACKED_PROBE.txt\"}")"
disc_body="$(build_body "{\"project\":\"$RUNTIME_PROJECT_ID\",\"paths\":[\"UNTRACKED_PROBE.txt\"]}")"
body="$(api_post /api/projects/discard_untracked "$disc_body")"
disc_success="$(json_get "$body" success)"
if [ "$disc_success" = "True" ]; then
    pass "discardUntrackedFiles(probe) returns success"
else
    fail "discardUntrackedFiles(probe) failed (body: ${body:0:300})"
fi
body="$(api_post /api/projects/list_files "{\"project\":\"$RUNTIME_PROJECT_ID\"}")"
lpf_entries="$(json_get "$body" output.entries)"
if ! echo "$lpf_entries" | grep -q "UNTRACKED_PROBE.txt"; then
    pass "discardUntrackedFiles removed untracked probe file"
else
    fail "discardUntrackedFiles did not remove probe file (got: ${lpf_entries:0:200})"
fi

# gitRestorePaths — create a tracked probe file, commit it, modify it, then
# restore it. This verifies restore returns the file to its committed state.
body="$(api_post /api/projects/run_shell "{\"project\":\"$RUNTIME_PROJECT_ID\",\"command\":\"printf original > RESTORE_PROBE.txt && git add RESTORE_PROBE.txt && git commit -m probe >/dev/null 2>&1\"}")"
body="$(api_post /api/projects/run_shell "{\"project\":\"$RUNTIME_PROJECT_ID\",\"command\":\"printf modified > RESTORE_PROBE.txt\"}")"
rest_body="$(build_body "{\"project\":\"$RUNTIME_PROJECT_ID\",\"paths\":[\"RESTORE_PROBE.txt\"]}")"
body="$(api_post /api/projects/git_restore_paths "$rest_body")"
rest_success="$(json_get "$body" success)"
if [ "$rest_success" = "True" ]; then
    pass "gitRestorePaths(probe) returns success"
else
    fail "gitRestorePaths(probe) failed (body: ${body:0:300})"
fi
body="$(api_post /api/projects/read_file "{\"project\":\"$RUNTIME_PROJECT_ID\",\"path\":\"RESTORE_PROBE.txt\"}")"
restore_content="$(json_get "$body" output.content)"
if echo "$restore_content" | grep -q "original"; then
    pass "gitRestorePaths restored probe file to committed content"
else
    fail "gitRestorePaths did not restore content (got: ${restore_content:0:120})"
fi

# Clean up the tracked probe file so the worktree returns to a clean state.
body="$(api_post /api/projects/run_shell "{\"project\":\"$RUNTIME_PROJECT_ID\",\"command\":\"git rm -f RESTORE_PROBE.txt >/dev/null 2>&1 && git commit -m cleanup-probe >/dev/null 2>&1\"}")" || true

# ----------------------------------------------------------------------------
# 7e. Phase 4: structured edit tools (replace_in_file / write_project_file)
#     via callRuntimeTool and MCP, against probe files only
# ----------------------------------------------------------------------------

log "---- Phase 4: structured edit tools (probe files only) ----"

# write_project_file via callRuntimeTool — create EDIT_PROBE.txt.
wpf_body="$(python3 -c '
import json, sys
print(json.dumps({
    "tool": "write_project_file",
    "params": {
        "project": sys.argv[1],
        "path": "EDIT_PROBE.txt",
        "content": "hello world\n"
    }
}))
' "$RUNTIME_PROJECT_ID")"
body="$(api_post /api/tools/call "$wpf_body")"
wpf_success="$(json_get "$body" success)"
wpf_created="$(json_get "$body" output.created)"
if [ "$wpf_success" = "True" ] && [ "$wpf_created" = "True" ]; then
    pass "callRuntimeTool(write_project_file) creates EDIT_PROBE.txt"
else
    fail "callRuntimeTool(write_project_file) did not create probe (success=$wpf_success created=$wpf_created body=${body:0:300})"
fi
wpf_sha="$(json_get "$body" output.sha256)"
if [ -n "$wpf_sha" ] && [ "$wpf_sha" != "None" ] && [ ${#wpf_sha} -eq 64 ]; then
    pass "write_project_file returns 64-char sha256"
else
    fail "write_project_file missing sha256 (got: $wpf_sha)"
fi

# readProjectFile confirms the probe content.
body="$(api_post /api/projects/read_file "{\"project\":\"$RUNTIME_PROJECT_ID\",\"path\":\"EDIT_PROBE.txt\"}")"
if echo "$(json_get "$body" output.content)" | grep -q "hello world"; then
    pass "readProjectFile confirms EDIT_PROBE.txt content"
else
    fail "readProjectFile did not confirm probe content (got: ${body:0:200})"
fi

# replace_in_file via callRuntimeTool — change "world" -> "rust".
rif_body="$(python3 -c '
import json, sys
print(json.dumps({
    "tool": "replace_in_file",
    "params": {
        "project": sys.argv[1],
        "path": "EDIT_PROBE.txt",
        "old": "world",
        "new": "rust"
    }
}))
' "$RUNTIME_PROJECT_ID")"
body="$(api_post /api/tools/call "$rif_body")"
rif_success="$(json_get "$body" success)"
rif_changed="$(json_get "$body" output.changed)"
if [ "$rif_success" = "True" ] && [ "$rif_changed" = "True" ]; then
    pass "callRuntimeTool(replace_in_file) edits EDIT_PROBE.txt"
else
    fail "callRuntimeTool(replace_in_file) did not edit probe (success=$rif_success changed=$rif_changed body=${body:0:300})"
fi

# readProjectFile confirms the edited content.
body="$(api_post /api/projects/read_file "{\"project\":\"$RUNTIME_PROJECT_ID\",\"path\":\"EDIT_PROBE.txt\"}")"
if echo "$(json_get "$body" output.content)" | grep -q "hello rust"; then
    pass "readProjectFile confirms replace_in_file edit"
else
    fail "readProjectFile did not confirm edit (got: ${body:0:200})"
fi

# replace_in_file with a missing old must fail WITHOUT modifying the file.
rif_miss="$(python3 -c '
import json, sys
print(json.dumps({
    "tool": "replace_in_file",
    "params": {
        "project": sys.argv[1],
        "path": "EDIT_PROBE.txt",
        "old": "does-not-exist",
        "new": "x"
    }
}))
' "$RUNTIME_PROJECT_ID")"
body="$(api_post /api/tools/call "$rif_miss")"
if [ "$(json_get "$body" success)" = "False" ]; then
    pass "replace_in_file(missing old) fails"
else
    fail "replace_in_file(missing old) unexpectedly succeeded (body: ${body:0:200})"
fi
body="$(api_post /api/projects/read_file "{\"project\":\"$RUNTIME_PROJECT_ID\",\"path\":\"EDIT_PROBE.txt\"}")"
if echo "$(json_get "$body" output.content)" | grep -q "hello rust"; then
    pass "replace_in_file(missing old) left file unchanged"
else
    fail "replace_in_file(missing old) modified the file (got: ${body:0:200})"
fi

# deleteProjectFiles removes the probe so the worktree returns to clean.
del_body="$(build_body "{\"project\":\"$RUNTIME_PROJECT_ID\",\"paths\":[\"EDIT_PROBE.txt\"]}")"
body="$(api_post /api/projects/delete_files "$del_body")"
if [ "$(json_get "$body" success)" = "True" ]; then
    pass "deleteProjectFiles removes EDIT_PROBE.txt"
else
    fail "deleteProjectFiles did not remove probe (body: ${body:0:300})"
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
