#!/usr/bin/env bash
set -uo pipefail

# Minimal WebCodex coding-loop eval harness.
#
# This harness measures deterministic runtime/tool-loop mechanics only. It
# starts a local WebCodex server and agent, exposes a disposable local project,
# runs three scripted cases through /api/tools/call, and emits a final JSON
# summary as the last stdout line.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

CARGO_BIN="${CARGO_BIN:-cargo}"
TOKEN="${EVAL_TOKEN:-eval-coding-loop-token}"
CLIENT_ID="${EVAL_CLIENT_ID:-eval-agent}"
PROJECT_ID="${EVAL_PROJECT_ID:-coding-loop-eval}"
TRANSPORT="${EVAL_TRANSPORT:-websocket}"
TIMEOUT_SECS="${EVAL_TIMEOUT_SECS:-240}"
RUNTIME_PROJECT_ID="agent:${CLIENT_ID}:${PROJECT_ID}"

PASS=0
FAIL=0
SERVER_PID=""
AGENT_PID=""
TMP_ROOT=""
DATA_DIR=""
PROJECTS_DIR=""
AGENT_TOML=""
TEST_REPO=""
SERVER_LOG=""
AGENT_LOG=""
CASE_SUMMARIES_FILE=""
CASE_WARNINGS_FILE=""
LAST_BODY=""
LAST_SESSION_ID=""
START_EPOCH="$(date +%s)"

CASE_NAME=""
CASE_TOOL_CALLS=0
CASE_RAW_SHELL_CALLS=0
CASE_STRUCTURED_EDIT_CALLS=0
CASE_FAILED_TOOL_CALLS=0
CASE_RECOVERED_FAILED_TOOL_CALLS=0
CASE_FINISH_CALLED=0
CASE_FINISH_SUCCEEDED=0
CASE_ASSERT_FAILURES=0
CASE_WORKSPACE_CLEAN=false

log() { printf '[eval] %s\n' "$*"; }

elapsed() {
    echo $(( $(date +%s) - START_EPOCH ))
}

remaining_time() {
    local used
    used="$(elapsed)"
    echo $(( TIMEOUT_SECS - used ))
}

check_deadline() {
    if [ "$(remaining_time)" -le 0 ]; then
        log "overall timeout (${TIMEOUT_SECS}s) exceeded"
        exit 1
    fi
}

print_logs_hint() {
    cat >&2 <<EOF

[eval] ---- log locations ----
[eval] server log: ${SERVER_LOG:-<none>}
[eval] agent log:  ${AGENT_LOG:-<none>}
[eval] temp root:  ${TMP_ROOT:-<none>}
EOF
}

cleanup_background_processes() {
    if [ -n "${AGENT_PID:-}" ] && kill -0 "$AGENT_PID" 2>/dev/null; then
        kill "$AGENT_PID" 2>/dev/null || true
        sleep 1
        kill -9 "$AGENT_PID" 2>/dev/null || true
        wait "$AGENT_PID" 2>/dev/null || true
    fi
    if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        sleep 1
        kill -9 "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

on_abort() {
    local status=$?
    cleanup_background_processes
    if [ "$status" -ne 0 ]; then
        print_logs_hint
    fi
}

trap on_abort INT TERM EXIT

if [ "${EVAL_SKIP_RUN:-0}" = "1" ]; then
    log "EVAL_SKIP_RUN=1: skipping service startup and eval execution"
    python3 - <<'PY'
import json

cases = [
    {"case": "inspect_only", "planned": True},
    {"case": "small_structured_line_edit", "planned": True},
    {"case": "failed_call_recovery", "planned": True},
]
summary = {
    "skipped": True,
    "cases_total": 3,
    "cases_passed": 0,
    "cases_failed": 0,
    "tool_calls_total": 0,
    "raw_shell_calls": 0,
    "raw_shell_ratio": 0.0,
    "structured_edit_calls": 0,
    "failed_tool_calls": 0,
    "recovered_failed_tool_calls": 0,
    "workspace_clean_after_each_case": None,
    "finish_coding_task_success_rate": None,
    "cases": cases,
}
print(json.dumps(summary, separators=(",", ":"), sort_keys=True))
PY
    trap - INT TERM EXIT
    exit 0
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "[eval] curl is required" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "[eval] python3 is required for script-layer JSON parsing" >&2
    exit 2
fi
if ! command -v git >/dev/null 2>&1; then
    echo "[eval] git is required" >&2
    exit 2
fi
if ! command -v "$CARGO_BIN" >/dev/null 2>&1; then
    echo "[eval] ${CARGO_BIN} is required" >&2
    exit 2
fi

find_free_port() {
    python3 - <<'PY'
import socket

s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

wait_for_port() {
    local port="$1"
    local budget="${2:-60}"
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

api_post() {
    local path="$1"
    local body="${2:-}"
    if [ -z "$body" ]; then
        body="{}"
    fi
    curl -sS --max-time 20 \
        -H "Authorization: Bearer ${TOKEN}" \
        -H "Content-Type: application/json" \
        -X POST "http://127.0.0.1:${PORT}${path}" \
        -d "$body" 2>/dev/null
}

json_get() {
    local json="$1"
    local path="$2"
    python3 - "$json" "$path" <<'PY'
import json
import sys

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
elif cur is None:
    print("")
else:
    print(cur)
PY
}

json_body() {
    local tool="$1"
    local params="$2"
    python3 - "$tool" "$params" <<'PY'
import json
import sys

tool = sys.argv[1]
params = json.loads(sys.argv[2])
print(json.dumps({"tool": tool, "params": params}, separators=(",", ":")))
PY
}

params_start_task() {
    local title="$1"
    python3 - "$RUNTIME_PROJECT_ID" "$title" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "title": sys.argv[2],
    "mode": "normal",
    "include_runtime_status": False,
    "include_git": True,
    "include_recent_commits": False,
    "include_rules": False,
    "bind_current": False,
}, separators=(",", ":")))
PY
}

params_finish_task() {
    local session_id="$1"
    python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "include_diff": True,
    "include_hygiene": True,
    "include_handoff": True,
    "include_validation_summary": True,
}, separators=(",", ":")))
PY
}

call_tool() {
    local tool="$1"
    local params
    local body

    if [ "$#" -ge 2 ] && [ -n "$2" ]; then
        params="$2"
    else
        params="{}"
    fi

    body="$(json_body "$tool" "$params")"

    CASE_TOOL_CALLS=$((CASE_TOOL_CALLS + 1))
    case "$tool" in
        run_shell|run_job)
            CASE_RAW_SHELL_CALLS=$((CASE_RAW_SHELL_CALLS + 1))
            ;;
    esac
    case "$tool" in
        replace_line_range|insert_at_line|delete_line_range|apply_text_edits)
            CASE_STRUCTURED_EDIT_CALLS=$((CASE_STRUCTURED_EDIT_CALLS + 1))
            ;;
    esac
    if [ "$tool" = "finish_coding_task" ]; then
        CASE_FINISH_CALLED=$((CASE_FINISH_CALLED + 1))
    fi

    LAST_BODY="$(api_post /api/tools/call "$body" || true)"

    if [ "$(json_get "$LAST_BODY" success)" != "True" ]; then
        CASE_FAILED_TOOL_CALLS=$((CASE_FAILED_TOOL_CALLS + 1))
    elif [ "$tool" = "finish_coding_task" ]; then
        CASE_FINISH_SUCCEEDED=$((CASE_FINISH_SUCCEEDED + 1))
    fi
}

case_warn() {
    local message="$1"
    python3 - "$message" <<'PY' >>"$CASE_WARNINGS_FILE"
import json
import sys

print(json.dumps(sys.argv[1]))
PY
}

case_ok() {
    PASS=$((PASS + 1))
    log "[ok]   ${CASE_NAME}: $*"
}

case_fail() {
    FAIL=$((FAIL + 1))
    CASE_ASSERT_FAILURES=$((CASE_ASSERT_FAILURES + 1))
    log "[FAIL] ${CASE_NAME}: $*"
    case_warn "$*"
}

assert_success() {
    local label="$1"
    local body="$2"
    if [ "$(json_get "$body" success)" = "True" ]; then
        case_ok "$label"
    else
        case_fail "$label (body: ${body:0:240})"
    fi
}

assert_failure() {
    local label="$1"
    local body="$2"
    if [ "$(json_get "$body" success)" != "True" ] && [ -n "$(json_get "$body" error)" ]; then
        case_ok "$label"
    else
        case_fail "$label (expected controlled failure; body: ${body:0:240})"
    fi
}

workspace_clean_local() {
    local status
    status="$(git -C "$TEST_REPO" status --porcelain --untracked-files=normal 2>/dev/null || true)"
    [ -z "$status" ]
}

cleanup_temp_repo_worktree() {
    git -C "$TEST_REPO" restore --staged --worktree . >/dev/null 2>&1 || true
    git -C "$TEST_REPO" clean -fd >/dev/null 2>&1 || true
    rm -rf "$TEST_REPO/target" >/dev/null 2>&1 || true
}

begin_case() {
    CASE_NAME="$1"
    CASE_TOOL_CALLS=0
    CASE_RAW_SHELL_CALLS=0
    CASE_STRUCTURED_EDIT_CALLS=0
    CASE_FAILED_TOOL_CALLS=0
    CASE_RECOVERED_FAILED_TOOL_CALLS=0
    CASE_FINISH_CALLED=0
    CASE_FINISH_SUCCEEDED=0
    CASE_ASSERT_FAILURES=0
    CASE_WORKSPACE_CLEAN=false
    CASE_WARNINGS_FILE="$TMP_ROOT/${CASE_NAME}.warnings.jsonl"
    : >"$CASE_WARNINGS_FILE"
    log "---- case: ${CASE_NAME} ----"
    cleanup_temp_repo_worktree
    if workspace_clean_local; then
        case_ok "starts from a clean temporary worktree"
    else
        case_fail "temporary worktree is dirty before case"
    fi
}

record_case_summary() {
    local passed="$1"
    local workspace_clean="$2"
    python3 - \
        "$CASE_NAME" \
        "$passed" \
        "$CASE_TOOL_CALLS" \
        "$CASE_RAW_SHELL_CALLS" \
        "$CASE_STRUCTURED_EDIT_CALLS" \
        "$CASE_FAILED_TOOL_CALLS" \
        "$CASE_RECOVERED_FAILED_TOOL_CALLS" \
        "$workspace_clean" \
        "$CASE_FINISH_CALLED" \
        "$CASE_FINISH_SUCCEEDED" \
        "$CASE_WARNINGS_FILE" <<'PY' >>"$CASE_SUMMARIES_FILE"
import json
import sys

warnings = []
try:
    with open(sys.argv[11], "r", encoding="utf-8") as handle:
        warnings = [json.loads(line) for line in handle if line.strip()]
except FileNotFoundError:
    warnings = []

summary = {
    "case": sys.argv[1],
    "passed": sys.argv[2] == "true",
    "tool_calls": int(sys.argv[3]),
    "raw_shell_calls": int(sys.argv[4]),
    "structured_edit_calls": int(sys.argv[5]),
    "failed_tool_calls": int(sys.argv[6]),
    "recovered_failed_tool_calls": int(sys.argv[7]),
    "workspace_clean": sys.argv[8] == "true",
    "finish_coding_task_calls": int(sys.argv[9]),
    "finish_coding_task_successes": int(sys.argv[10]),
    "warnings": warnings,
}
print(json.dumps(summary, separators=(",", ":"), sort_keys=True))
PY
}

end_case() {
    cleanup_temp_repo_worktree
    if workspace_clean_local; then
        CASE_WORKSPACE_CLEAN=true
        case_ok "temporary worktree clean after cleanup"
    else
        CASE_WORKSPACE_CLEAN=false
        case_fail "temporary worktree is dirty after cleanup"
    fi

    if [ "$CASE_ASSERT_FAILURES" -eq 0 ] && [ "$CASE_WORKSPACE_CLEAN" = "true" ]; then
        record_case_summary true "$CASE_WORKSPACE_CLEAN"
    else
        record_case_summary false "$CASE_WORKSPACE_CLEAN"
    fi
}

assert_start_session() {
    local body="$1"
    local session_id
    LAST_SESSION_ID=""
    assert_success "start_coding_task succeeds" "$body"
    session_id="$(json_get "$body" output.session.session_id)"
    if [[ "$session_id" == wc_sess_* ]]; then
        case_ok "session_id created"
        LAST_SESSION_ID="$session_id"
    else
        case_fail "start_coding_task did not return a wc_sess_* session id"
    fi
}

start_eval_services() {
    PORT="${EVAL_PORT:-$(find_free_port)}"
    TMP_ROOT="$(mktemp -d -t webcodex-eval-coding-loop-XXXXXX)"
    DATA_DIR="$TMP_ROOT/data"
    PROJECTS_DIR="$TMP_ROOT/projects.d"
    AGENT_TOML="$TMP_ROOT/agent.toml"
    TEST_REPO="$TMP_ROOT/coding-loop-project"
    SERVER_LOG="$TMP_ROOT/server.log"
    AGENT_LOG="$TMP_ROOT/agent.log"
    CASE_SUMMARIES_FILE="$TMP_ROOT/case-summaries.jsonl"

    mkdir -p "$DATA_DIR" "$PROJECTS_DIR" "$TEST_REPO/src"
    : >"$CASE_SUMMARIES_FILE"

    log "temp root: $TMP_ROOT"
    log "using port: $PORT"
    log "runtime project id: $RUNTIME_PROJECT_ID"

    (
        cd "$TEST_REPO" || exit 1
        git init -b main >/dev/null 2>&1
        git config user.email "eval@test.local"
        git config user.name "WebCodex Eval"
        cat >Cargo.toml <<'EOF'
[package]
name = "webcodex-eval-temp"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
EOF
        cat >src/lib.rs <<'EOF'
pub fn greeting() -> &'static str {
    "hello"
}

pub fn topic() -> &'static str {
    "coding-loop eval"
}
EOF
        cat >README.md <<'EOF'
# Coding Loop Eval

This disposable project is used by the WebCodex coding-loop eval harness.
It contains the phrase coding-loop eval so search_project_text has a stable match.
EOF
        cat >.gitignore <<'EOF'
/target/
EOF
        "$CARGO_BIN" generate-lockfile >/dev/null 2>&1
        git add . >/dev/null 2>&1
        git commit -m "init eval project" >/dev/null 2>&1
    ) || {
        echo "[eval] failed to initialize temporary Rust project" >&2
        exit 2
    }

    cat >"$PROJECTS_DIR/${PROJECT_ID}.toml" <<EOF
id = "${PROJECT_ID}"
path = "${TEST_REPO}"
name = "Coding Loop Eval"
allow_patch = true
kind = "rust"
description = "Disposable project for the minimal coding-loop eval harness"
EOF

    cat >"$AGENT_TOML" <<EOF
server_url = "http://127.0.0.1:${PORT}"
token = "${TOKEN}"
client_id = "${CLIENT_ID}"
display_name = "Coding Loop Eval Agent"
owner = "eval"
projects_dir = "${PROJECTS_DIR}"
poll_interval_ms = 500
transport = "${TRANSPORT}"

[policy]
allow_raw_shell = true
allow_cwd_anywhere = true
max_timeout_secs = 60
max_output_bytes = 262144
EOF

    log "starting server"
    WEBCODEX_ADDR="127.0.0.1:${PORT}" \
    WEBCODEX_DATA="$DATA_DIR" \
    WEBCODEX_TOKEN="$TOKEN" \
    CODEX_DEFAULT_TIMEOUT_SECS="30" \
    CODEX_APPROVAL_MODE="full-auto" \
    RUST_LOG="info" \
    "$CARGO_BIN" run --quiet --bin webcodex >"$SERVER_LOG" 2>&1 &
    SERVER_PID=$!

    if ! wait_for_port "$PORT" 90; then
        echo "[eval] server did not start listening on $PORT" >&2
        print_logs_hint
        exit 1
    fi
    log "server listening on $PORT"

    log "starting agent (transport=${TRANSPORT})"
    "$CARGO_BIN" run --quiet --bin webcodex-agent -- --config "$AGENT_TOML" >"$AGENT_LOG" 2>&1 &
    AGENT_PID=$!

    log "waiting for agent registration"
    local registered=0
    local body
    for _ in $(seq 1 90); do
        check_deadline
        body="$(api_post /api/runtime/status '{}' || true)"
        if [ "$(json_get "$body" output.agents.count)" = "1" ]; then
            registered=1
            break
        fi
        sleep 1
    done

    if [ "$registered" -ne 1 ]; then
        echo "[eval] agent did not register within budget" >&2
        print_logs_hint
        exit 1
    fi
    log "agent registered"
}

run_case_inspect_only() {
    local session_id
    local params

    begin_case "inspect_only"

    params="$(params_start_task "eval inspect-only")"
    call_tool "start_coding_task" "$params"
    assert_start_session "$LAST_BODY"
    session_id="$LAST_SESSION_ID"
    if [ -z "$session_id" ]; then
        end_case
        return
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "pattern": "coding-loop eval",
    "path": ".",
    "limit": 10,
}, separators=(",", ":")))
PY
)"
    call_tool "search_project_text" "$params"
    assert_success "search_project_text succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
out = data.get("output") or {}
ok = (
    data.get("success") is True
    and isinstance(out.get("backend"), str)
    and out.get("backend")
    and isinstance(out.get("matches"), list)
    and isinstance(out.get("truncated"), bool)
    and out.get("count", 0) >= 1
)
sys.exit(0 if ok else 1)
PY
    then
        case_ok "search_project_text returns backend/matches/truncated"
    else
        case_fail "search_project_text structured fields missing"
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "README.md",
    "start_line": 1,
    "limit": 6,
    "with_line_numbers": True,
}, separators=(",", ":")))
PY
)"
    call_tool "read_file" "$params"
    assert_success "read_file succeeds" "$LAST_BODY"

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "include_diff": False,
}, separators=(",", ":")))
PY
)"
    call_tool "show_changes" "$params"
    assert_success "show_changes succeeds" "$LAST_BODY"

    params="$(params_finish_task "$session_id")"
    call_tool "finish_coding_task" "$params"
    assert_success "finish_coding_task succeeds" "$LAST_BODY"

    if [ "$CASE_RAW_SHELL_CALLS" -eq 0 ]; then
        case_ok "raw shell runtime calls are zero"
    else
        case_fail "raw shell runtime calls should be zero"
    fi

    end_case
}

run_case_small_structured_line_edit() {
    local session_id
    local params

    begin_case "small_structured_line_edit"

    params="$(params_start_task "eval small structured line edit")"
    call_tool "start_coding_task" "$params"
    assert_start_session "$LAST_BODY"
    session_id="$LAST_SESSION_ID"
    if [ -z "$session_id" ]; then
        end_case
        return
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "src/lib.rs",
    "start_line": 1,
    "limit": 8,
    "with_line_numbers": True,
}, separators=(",", ":")))
PY
)"
    call_tool "read_file" "$params"
    assert_success "read_file with line numbers succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
out = data.get("output") or {}
ok = (
    data.get("success") is True
    and isinstance(out.get("numbered_text"), str)
    and "1 | pub fn greeting" in out.get("numbered_text", "")
    and isinstance(out.get("lines"), list)
)
sys.exit(0 if ok else 1)
PY
    then
        case_ok "read_file returned stable line-number metadata"
    else
        case_fail "read_file line-number metadata missing"
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "src/lib.rs",
    "start_line": 2,
    "end_line": 2,
    "new_text": "    \"hello eval\"\n",
    "expected_old_prefix": "    \"hello\"",
}, separators=(",", ":")))
PY
)"
    call_tool "replace_line_range" "$params"
    assert_success "replace_line_range structured edit succeeds" "$LAST_BODY"

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "include_diff": True,
    "max_hunks": 5,
    "max_hunk_lines": 40,
}, separators=(",", ":")))
PY
)"
    call_tool "show_changes" "$params"
    assert_success "show_changes after edit succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
out = data.get("output") or {}
files = out.get("files") or []
ok = (
    data.get("success") is True
    and out.get("clean") is False
    and any(item.get("path") == "src/lib.rs" for item in files if isinstance(item, dict))
)
sys.exit(0 if ok else 1)
PY
    then
        case_ok "show_changes reports src/lib.rs as changed"
    else
        case_fail "show_changes did not report src/lib.rs as changed"
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "timeout_secs": 60,
}, separators=(",", ":")))
PY
)"
    call_tool "cargo_check" "$params"
    assert_success "cargo_check validation succeeds" "$LAST_BODY"

    params="$(params_finish_task "$session_id")"
    call_tool "finish_coding_task" "$params"
    assert_success "finish_coding_task succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
out = data.get("output") or {}
changes = ((out.get("changes") or {}).get("show_changes") or {})
files = changes.get("files") or []
ok = (
    data.get("success") is True
    and (out.get("workspace") or {}).get("clean") is False
    and any(item.get("path") == "src/lib.rs" for item in files if isinstance(item, dict))
)
sys.exit(0 if ok else 1)
PY
    then
        case_ok "finish_coding_task reports changed file"
    else
        case_fail "finish_coding_task did not report changed file"
    fi

    if [ "$CASE_STRUCTURED_EDIT_CALLS" -eq 1 ]; then
        case_ok "exactly one structured edit tool call recorded"
    else
        case_fail "expected exactly one structured edit tool call"
    fi
    if [ "$CASE_RAW_SHELL_CALLS" -eq 0 ]; then
        case_ok "raw shell runtime calls are zero"
    else
        case_fail "raw shell runtime calls should be zero"
    fi

    end_case
}

run_case_failed_call_recovery() {
    local session_id
    local params

    begin_case "failed_call_recovery"

    params="$(params_start_task "eval failed call recovery")"
    call_tool "start_coding_task" "$params"
    assert_start_session "$LAST_BODY"
    session_id="$LAST_SESSION_ID"
    if [ -z "$session_id" ]; then
        end_case
        return
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "src/lib.rs",
    "start_line": 2,
    "end_line": 2,
    "new_text": "    \"should not apply\"\n",
    "expected_old_prefix": "    \"definitely not the current line\"",
}, separators=(",", ":")))
PY
)"
    call_tool "replace_line_range" "$params"
    assert_failure "replace_line_range wrong guard fails in a controlled way" "$LAST_BODY"

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "src/lib.rs",
    "start_line": 1,
    "limit": 4,
    "with_line_numbers": True,
}, separators=(",", ":")))
PY
)"
    call_tool "read_file" "$params"
    assert_success "read_file after failed edit succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
content = (data.get("output") or {}).get("content", "")
ok = data.get("success") is True and '"hello"' in content and "should not apply" not in content
sys.exit(0 if ok else 1)
PY
    then
        case_ok "failed edit did not corrupt src/lib.rs"
    else
        case_fail "failed edit changed src/lib.rs unexpectedly"
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "path": "src/lib.rs",
    "start_line": 2,
    "end_line": 2,
    "new_text": "    \"recovered\"\n",
    "expected_old_prefix": "    \"hello\"",
}, separators=(",", ":")))
PY
)"
    call_tool "replace_line_range" "$params"
    assert_success "replace_line_range recovery edit succeeds" "$LAST_BODY"

    if [ "$CASE_FAILED_TOOL_CALLS" -ge 1 ]; then
        CASE_RECOVERED_FAILED_TOOL_CALLS=1
        case_ok "failed tool call was followed by successful recovery"
    else
        case_fail "expected at least one failed tool call before recovery"
    fi

    params="$(python3 - "$RUNTIME_PROJECT_ID" "$session_id" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "session_id": sys.argv[2],
    "include_diff": True,
    "max_hunks": 5,
    "max_hunk_lines": 40,
}, separators=(",", ":")))
PY
)"
    call_tool "show_changes" "$params"
    assert_success "show_changes after recovery succeeds" "$LAST_BODY"

    params="$(params_finish_task "$session_id")"
    call_tool "finish_coding_task" "$params"
    assert_success "finish_coding_task succeeds" "$LAST_BODY"
    if python3 - "$LAST_BODY" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])
handoff = (data.get("output") or {}).get("handoff") or {}
counts = handoff.get("counts") or {}
recent = handoff.get("recent_failed_tools") or []
ok = (
    data.get("success") is True
    and counts.get("failed_tool_calls", 0) >= 1
    and any(item.get("tool_name") == "replace_line_range" for item in recent if isinstance(item, dict))
)
sys.exit(0 if ok else 1)
PY
    then
        case_ok "finish_coding_task handoff includes failed tool metadata"
    else
        case_fail "finish_coding_task handoff missing failed tool metadata"
    fi

    if [ "$CASE_RAW_SHELL_CALLS" -eq 0 ]; then
        case_ok "raw shell runtime calls are zero"
    else
        case_fail "raw shell runtime calls should be zero"
    fi

    end_case
}

build_final_summary() {
    python3 - "$CASE_SUMMARIES_FILE" <<'PY'
import json
import sys

cases = []
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if line:
            cases.append(json.loads(line))

tool_calls_total = sum(c["tool_calls"] for c in cases)
raw_shell_calls = sum(c["raw_shell_calls"] for c in cases)
finish_calls = sum(c["finish_coding_task_calls"] for c in cases)
finish_successes = sum(c["finish_coding_task_successes"] for c in cases)

summary = {
    "skipped": False,
    "cases_total": len(cases),
    "cases_passed": sum(1 for c in cases if c["passed"]),
    "cases_failed": sum(1 for c in cases if not c["passed"]),
    "tool_calls_total": tool_calls_total,
    "raw_shell_calls": raw_shell_calls,
    "raw_shell_ratio": (raw_shell_calls / tool_calls_total) if tool_calls_total else 0.0,
    "structured_edit_calls": sum(c["structured_edit_calls"] for c in cases),
    "failed_tool_calls": sum(c["failed_tool_calls"] for c in cases),
    "recovered_failed_tool_calls": sum(c["recovered_failed_tool_calls"] for c in cases),
    "workspace_clean_after_each_case": all(c["workspace_clean"] for c in cases) if cases else False,
    "finish_coding_task_success_rate": (finish_successes / finish_calls) if finish_calls else 0.0,
    "cases": cases,
}
print(json.dumps(summary, separators=(",", ":"), sort_keys=True))
PY
}

start_eval_services
run_case_inspect_only
run_case_small_structured_line_edit
run_case_failed_call_recovery

FINAL_SUMMARY="$(build_final_summary)"
FINAL_FAILED="$(json_get "$FINAL_SUMMARY" cases_failed)"

cleanup_background_processes
trap - INT TERM EXIT

if [ "${FINAL_FAILED:-0}" = "0" ]; then
    if [ "${EVAL_KEEP_TMP:-0}" = "1" ]; then
        log "EVAL_KEEP_TMP=1: keeping temp root $TMP_ROOT"
    else
        rm -rf "$TMP_ROOT"
    fi
else
    print_logs_hint
fi

log "final JSON summary follows"
printf '%s\n' "$FINAL_SUMMARY"

if [ "${FINAL_FAILED:-0}" = "0" ]; then
    exit 0
fi
exit 1
