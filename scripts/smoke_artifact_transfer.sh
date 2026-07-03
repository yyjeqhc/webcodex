#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# WebCodex - Artifact Transfer Smoke
#
# Default mode is documentation-only: it prints the required environment and
# checklist, then exits without reading secrets or contacting a server.
#
# Active mode:
#   WEBCODEX_SMOKE_RUN=1 \
#   WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
#   WEBCODEX_TOKEN="<wc_pat_or_allowed_shared_key>" \
#   bash scripts/smoke_artifact_transfer.sh
#
# This script never prints the token. It uses a pre-registered smoke project
# and writes only fixed smoke artifact paths before deleting them again.
# ============================================================================

DEFAULT_PROJECT_ID="agent:special:webcodex-smoke"
DEFAULT_ARTIFACT_PATH="artifacts/smoke/webcodex-artifact-transfer.txt"
DEFAULT_ABORT_PATH="artifacts/smoke/webcodex-artifact-transfer-abort.txt"
DEFAULT_EXPECTED_OPERATION_COUNT="27"
DEFAULT_MAX_OPERATION_COUNT="30"

print_checklist() {
    cat <<EOF
[smoke] Artifact transfer deployment smoke checklist

Default mode only prints this checklist. It does not read token variables,
contact a server, write files, or delete files.

To run the HTTP smoke explicitly:

  WEBCODEX_SMOKE_RUN=1 \\
  WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \\
  WEBCODEX_TOKEN="<wc_pat_or_allowed_shared_key>" \\
  bash scripts/smoke_artifact_transfer.sh

Optional environment:

  WEBCODEX_SMOKE_PROJECT_ID      default: $DEFAULT_PROJECT_ID
  WEBCODEX_SMOKE_ARTIFACT_PATH   default: $DEFAULT_ARTIFACT_PATH
  WEBCODEX_SMOKE_ABORT_PATH      default: $DEFAULT_ABORT_PATH
  WEBCODEX_EXPECTED_OPERATION_COUNT default: $DEFAULT_EXPECTED_OPERATION_COUNT
  WEBCODEX_MAX_OPERATION_COUNT      default: $DEFAULT_MAX_OPERATION_COUNT
  SMOKE_TIMEOUT                  default: 20 seconds per HTTP call

Preconditions:

  1. The public WebCodex URL is reachable.
  2. The token is a managed wc_pat_* token or a deployment-allowed shared key.
     Do not use wc_agent_*; that token type is only for webcodex-agent.
  3. The smoke project is registered, agent-backed, online, and a git repo.
  4. The smoke project is disposable and clean before the run.

Checks covered by active mode:

  1. GET /openapi.json parses as JSON.
  2. GPT Action operation_count is <= 30; the current recommended count is 27.
  3. Bounded discovery works through /api/tools/list and tool_manifest.
  4. artifact_upload_begin, artifact_upload_chunk, and artifact_upload_finish.
  5. read_project_artifact_metadata and read_project_artifact.
  6. artifact_upload_abort cleanup for a second temporary upload.
  7. delete_project_files cleanup of the committed smoke artifact.
  8. git_status and show_changes report a clean worktree after cleanup.

Active mode refuses non-smoke project ids unless
WEBCODEX_SMOKE_ALLOW_NON_SMOKE_PROJECT=1 is set. Custom artifact paths must stay
under artifacts/smoke/ unless WEBCODEX_SMOKE_ALLOW_CUSTOM_PATHS=1 is set.
EOF
}

if [ "${WEBCODEX_SMOKE_RUN:-0}" != "1" ]; then
    print_checklist
    exit 0
fi

BASE_URL="${WEBCODEX_PUBLIC_URL:-${BASE_URL:-}}"
TOKEN="${WEBCODEX_TOKEN:-${TOKEN:-}}"
PROJECT_ID="${WEBCODEX_SMOKE_PROJECT_ID:-${PROJECT_ID:-$DEFAULT_PROJECT_ID}}"
ARTIFACT_PATH="${WEBCODEX_SMOKE_ARTIFACT_PATH:-$DEFAULT_ARTIFACT_PATH}"
ABORT_PATH="${WEBCODEX_SMOKE_ABORT_PATH:-$DEFAULT_ABORT_PATH}"
EXPECTED_OPERATION_COUNT="${WEBCODEX_EXPECTED_OPERATION_COUNT:-$DEFAULT_EXPECTED_OPERATION_COUNT}"
MAX_OPERATION_COUNT="${WEBCODEX_MAX_OPERATION_COUNT:-$DEFAULT_MAX_OPERATION_COUNT}"
TIMEOUT="${SMOKE_TIMEOUT:-20}"

if [ -z "$BASE_URL" ]; then
    echo "[smoke] WEBCODEX_PUBLIC_URL (or BASE_URL) is required" >&2
    exit 2
fi
if [ -z "$TOKEN" ]; then
    echo "[smoke] WEBCODEX_TOKEN (or TOKEN) is required" >&2
    exit 2
fi

case "$PROJECT_ID" in
    *smoke*) ;;
    *)
        if [ "${WEBCODEX_SMOKE_ALLOW_NON_SMOKE_PROJECT:-0}" != "1" ]; then
            echo "[smoke] refusing non-smoke project id; set WEBCODEX_SMOKE_ALLOW_NON_SMOKE_PROJECT=1 to override" >&2
            exit 2
        fi
        ;;
esac

case "$ARTIFACT_PATH:$ABORT_PATH" in
    artifacts/smoke/*:artifacts/smoke/*) ;;
    *)
        if [ "${WEBCODEX_SMOKE_ALLOW_CUSTOM_PATHS:-0}" != "1" ]; then
            echo "[smoke] refusing artifact paths outside artifacts/smoke/; set WEBCODEX_SMOKE_ALLOW_CUSTOM_PATHS=1 to override" >&2
            exit 2
        fi
        ;;
esac

if ! command -v curl >/dev/null 2>&1; then
    echo "[smoke] curl is required" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "[smoke] python3 is required" >&2
    exit 2
fi

BASE_URL="${BASE_URL%/}"
PASS=0
WARN=0
FAIL=0

log() { printf '[smoke] %s\n' "$*"; }
pass() { PASS=$((PASS + 1)); printf '[smoke][ok]   %s\n' "$*"; }
warn() { WARN=$((WARN + 1)); printf '[smoke][warn] %s\n' "$*" >&2; }
fail() { FAIL=$((FAIL + 1)); printf '[smoke][FAIL] %s\n' "$*" >&2; }

AUTH_HEADER_FILE="$(mktemp -t webcodex-artifact-smoke-auth-XXXXXX)"
trap 'rm -f "$AUTH_HEADER_FILE"' INT TERM EXIT
printf 'Authorization: Bearer %s\n' "$TOKEN" > "$AUTH_HEADER_FILE"
chmod 600 "$AUTH_HEADER_FILE"

api_get() {
    local path="$1"
    curl -sS --max-time "$TIMEOUT" \
        -H @"$AUTH_HEADER_FILE" \
        "${BASE_URL}${path}" 2>/dev/null || true
}

api_post() {
    local path="$1"
    local body="${2:-{}}"
    curl -sS --max-time "$TIMEOUT" \
        -H @"$AUTH_HEADER_FILE" \
        -H "Content-Type: application/json" \
        -X POST "${BASE_URL}${path}" \
        -d "$body" 2>/dev/null || true
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
        if part not in cur:
            print("")
            sys.exit(0)
        cur = cur[part]
    else:
        print("")
        sys.exit(0)

if isinstance(cur, (dict, list)):
    print(json.dumps(cur, separators=(",", ":")))
elif cur is None:
    print("")
else:
    print(cur)
PY
}

tool_call_body() {
    local tool="$1"
    local params_json="${2:-{}}"
    python3 - "$tool" "$params_json" <<'PY'
import json
import sys

tool = sys.argv[1]
params = json.loads(sys.argv[2])
print(json.dumps({"tool": tool, "params": params}, separators=(",", ":")))
PY
}

call_tool() {
    local tool="$1"
    local params_json="${2:-{}}"
    local body
    body="$(tool_call_body "$tool" "$params_json")"
    api_post /api/tools/call "$body"
}

body_preview() {
    local body="$1"
    body="${body//$'\n'/ }"
    printf '%s' "${body:0:300}"
}

success_true() {
    local body="$1"
    [ "$(json_get "$body" success)" = "True" ]
}

check_success() {
    local label="$1"
    local body="$2"
    local error
    if success_true "$body"; then
        pass "$label"
        return 0
    fi
    error="$(json_get "$body" error)"
    if [ -z "$error" ]; then
        error="$(body_preview "$body")"
    fi
    fail "$label failed: $error"
    return 1
}

json_contains_strings() {
    local json="$1"
    local needle="$2"
    python3 - "$json" "$needle" <<'PY'
import json
import sys

try:
    obj = json.loads(sys.argv[1])
except Exception:
    sys.exit(1)
needle = sys.argv[2]

def walk(value):
    if value == needle:
        return True
    if isinstance(value, dict):
        return any(walk(v) for v in value.values())
    if isinstance(value, list):
        return any(walk(v) for v in value)
    return False

sys.exit(0 if walk(obj) else 1)
PY
}

json_tools_include() {
    local json="$1"
    local path="$2"
    shift 2
    python3 - "$json" "$path" "$@" <<'PY'
import json
import sys

try:
    obj = json.loads(sys.argv[1])
except Exception as exc:
    print(f"invalid JSON: {exc}")
    sys.exit(1)

cur = obj
for part in sys.argv[2].split("."):
    if not part:
        continue
    if isinstance(cur, dict):
        cur = cur.get(part)
    elif isinstance(cur, list):
        cur = cur[int(part)]
    else:
        cur = None
    if cur is None:
        print(f"missing path {sys.argv[2]}")
        sys.exit(1)

names = set()
if isinstance(cur, list):
    for item in cur:
        if isinstance(item, dict) and isinstance(item.get("name"), str):
            names.add(item["name"])
        elif isinstance(item, str):
            names.add(item)

missing = [name for name in sys.argv[3:] if name not in names]
if missing:
    print("missing " + ", ".join(missing))
    sys.exit(1)
print("ok")
PY
}

make_json_object() {
    python3 - "$@" <<'PY'
import json
import sys

if len(sys.argv[1:]) % 2 != 0:
    raise SystemExit("expected key/value pairs")

obj = {}
items = sys.argv[1:]
for key, value in zip(items[0::2], items[1::2]):
    if value == "__true__":
        obj[key] = True
    elif value == "__false__":
        obj[key] = False
    elif value == "__null__":
        obj[key] = None
    elif value.startswith("__int__:"):
        obj[key] = int(value[len("__int__:"):])
    elif value.startswith("__json__:"):
        obj[key] = json.loads(value[len("__json__:"):])
    else:
        obj[key] = value

print(json.dumps(obj, separators=(",", ":")))
PY
}

check_openapi() {
    local schema="$1"
    local info
    info="$(python3 - "$schema" <<'PY'
import json
import sys

try:
    schema = json.loads(sys.argv[1])
except Exception as exc:
    print(json.dumps({"ok": False, "error": f"invalid JSON: {exc}"}))
    sys.exit(0)

paths = schema.get("paths")
if not isinstance(paths, dict) or not paths:
    print(json.dumps({"ok": False, "error": "schema has no paths"}))
    sys.exit(0)

ops = []
for path, methods in paths.items():
    if not isinstance(methods, dict):
        continue
    for method, operation in methods.items():
        if isinstance(operation, dict):
            ops.append(operation.get("operationId") or f"{method} {path}")

print(json.dumps({
    "ok": True,
    "paths": len(paths),
    "operation_count": len(ops),
}))
PY
)"

    if [ "$(json_get "$info" ok)" != "True" ]; then
        fail "/openapi.json parse failed: $(json_get "$info" error)"
        return 1
    fi

    local paths
    local operation_count
    paths="$(json_get "$info" paths)"
    operation_count="$(json_get "$info" operation_count)"
    pass "/openapi.json parses as JSON with ${paths} path(s)"

    if [ "$operation_count" -le "$MAX_OPERATION_COUNT" ]; then
        pass "operation_count=${operation_count} is <= ${MAX_OPERATION_COUNT}"
    else
        fail "operation_count=${operation_count} exceeds ${MAX_OPERATION_COUNT}"
    fi

    if [ "$operation_count" -eq "$EXPECTED_OPERATION_COUNT" ]; then
        pass "operation_count matches current recommendation (${EXPECTED_OPERATION_COUNT})"
    else
        warn "operation_count=${operation_count}; current recommendation is ${EXPECTED_OPERATION_COUNT}"
    fi
}

check_clean_git_status() {
    local label="$1"
    local body
    body="$(call_tool git_status "$(make_json_object project "$PROJECT_ID")")"
    if ! check_success "$label git_status succeeds" "$body"; then
        return 1
    fi

    local exit_code
    local stdout
    exit_code="$(json_get "$body" output.exit_code)"
    stdout="$(json_get "$body" output.stdout)"
    if [ "$exit_code" = "0" ] && [ -z "$stdout" ]; then
        pass "$label git_status is clean"
    else
        fail "$label git_status is not clean or not a git repo (exit_code=${exit_code}, stdout=$(body_preview "$stdout"))"
        return 1
    fi

    body="$(call_tool show_changes "$(make_json_object project "$PROJECT_ID" include_diff __false__)")"
    if ! check_success "$label show_changes succeeds" "$body"; then
        return 1
    fi

    if [ "$(json_get "$body" output.git_available)" = "True" ] && \
       [ "$(json_get "$body" output.clean)" = "True" ]; then
        pass "$label show_changes reports git_available=true and clean=true"
    else
        fail "$label show_changes is not clean/git-backed (git_available=$(json_get "$body" output.git_available), clean=$(json_get "$body" output.clean))"
        return 1
    fi
}

log "artifact transfer smoke against $BASE_URL"
log "project: $PROJECT_ID"
log "artifact path: $ARTIFACT_PATH"
log "abort path: $ABORT_PATH"

payload_info="$(python3 - <<'PY'
import base64
import hashlib
import json

data = b"WebCodex artifact transfer smoke\n"
print(json.dumps({
    "bytes": len(data),
    "sha256": hashlib.sha256(data).hexdigest(),
    "base64": base64.b64encode(data).decode("ascii"),
}, separators=(",", ":")))
PY
)"
PAYLOAD_BYTES="$(json_get "$payload_info" bytes)"
PAYLOAD_SHA256="$(json_get "$payload_info" sha256)"
PAYLOAD_BASE64="$(json_get "$payload_info" base64)"

# ---------------------------------------------------------------------------
# Preflight: read-only schema, discovery, project, and git cleanliness checks.
# ---------------------------------------------------------------------------

log "---- preflight ----"

schema="$(api_get /openapi.json)"
check_openapi "$schema" || true

body="$(api_post /api/tools/list '{"summary_only":true,"category":"artifact","limit":20}')"
if json_tools_include "$body" tools \
    artifact_upload_begin artifact_upload_chunk artifact_upload_finish \
    artifact_upload_abort read_project_artifact_metadata read_project_artifact >/dev/null; then
    pass "bounded listRuntimeTools summary exposes artifact transfer tools"
else
    fail "bounded listRuntimeTools summary missing expected artifact tools ($(body_preview "$body"))"
fi

body="$(call_tool tool_manifest "$(make_json_object category artifact include_recommended_flows __false__ include_risk_summary __false__)")"
if check_success "tool_manifest(category=artifact) succeeds" "$body"; then
    if json_tools_include "$body" output.tools \
        artifact_upload_begin artifact_upload_chunk artifact_upload_finish \
        artifact_upload_abort read_project_artifact_metadata read_project_artifact >/dev/null; then
        pass "tool_manifest(category=artifact) exposes artifact transfer tools"
    else
        fail "tool_manifest(category=artifact) missing expected artifact tools"
    fi
fi

body="$(api_post /api/projects/list '{}')"
if check_success "listProjects succeeds" "$body"; then
    if json_contains_strings "$body" "$PROJECT_ID"; then
        pass "listProjects includes $PROJECT_ID"
    else
        fail "listProjects does not include $PROJECT_ID"
    fi
fi

check_clean_git_status "preflight" || true

if [ "$FAIL" -ne 0 ]; then
    log "preflight failed; artifact write and cleanup smoke will not run"
    log "---- summary ----"
    log "passed: $PASS"
    log "warned: $WARN"
    log "failed: $FAIL"
    exit 1
fi

# ---------------------------------------------------------------------------
# Artifact upload/read/abort/delete smoke against fixed smoke paths.
# ---------------------------------------------------------------------------

log "---- artifact upload/read/cleanup ----"

upload_id=""
abort_upload_id=""
artifact_committed=0

begin_params="$(make_json_object \
    project "$PROJECT_ID" \
    path "$ARTIFACT_PATH" \
    expected_bytes "__int__:$PAYLOAD_BYTES" \
    expected_sha256 "$PAYLOAD_SHA256" \
    mime_type text/plain \
    overwrite __true__)"
body="$(call_tool artifact_upload_begin "$begin_params")"
if check_success "artifact_upload_begin succeeds" "$body"; then
    upload_id="$(json_get "$body" output.upload_id)"
    if [ -n "$upload_id" ]; then
        pass "artifact_upload_begin returns upload_id"
    else
        fail "artifact_upload_begin did not return upload_id"
    fi
fi

if [ -n "$upload_id" ]; then
    chunk_params="$(make_json_object \
        project "$PROJECT_ID" \
        path "$ARTIFACT_PATH" \
        upload_id "$upload_id" \
        offset __int__:0 \
        content_base64 "$PAYLOAD_BASE64")"
    body="$(call_tool artifact_upload_chunk "$chunk_params")"
    check_success "artifact_upload_chunk succeeds" "$body" || true

    finish_params="$(make_json_object \
        project "$PROJECT_ID" \
        path "$ARTIFACT_PATH" \
        upload_id "$upload_id")"
    body="$(call_tool artifact_upload_finish "$finish_params")"
    if check_success "artifact_upload_finish succeeds" "$body"; then
        artifact_committed=1
        if [ "$(json_get "$body" output.sha256)" = "$PAYLOAD_SHA256" ]; then
            pass "artifact_upload_finish verifies expected sha256"
        else
            fail "artifact_upload_finish sha256 mismatch"
        fi
    fi
fi

if [ -n "$upload_id" ] && [ "$artifact_committed" -eq 0 ]; then
    abort_unfinished_params="$(make_json_object \
        project "$PROJECT_ID" \
        path "$ARTIFACT_PATH" \
        upload_id "$upload_id")"
    body="$(call_tool artifact_upload_abort "$abort_unfinished_params")"
    check_success "artifact_upload_abort cleanup for unfinished upload succeeds" "$body" || true
fi

if [ "$artifact_committed" -eq 1 ]; then
    metadata_params="$(make_json_object project "$PROJECT_ID" path "$ARTIFACT_PATH")"
    body="$(call_tool read_project_artifact_metadata "$metadata_params")"
    if check_success "read_project_artifact_metadata succeeds" "$body"; then
        if [ "$(json_get "$body" output.bytes)" = "$PAYLOAD_BYTES" ] && \
           [ "$(json_get "$body" output.sha256)" = "$PAYLOAD_SHA256" ]; then
            pass "read_project_artifact_metadata reports expected bytes and sha256"
        else
            fail "read_project_artifact_metadata mismatch"
        fi
    fi

    read_params="$(make_json_object \
        project "$PROJECT_ID" \
        path "$ARTIFACT_PATH" \
        offset __int__:0 \
        length "__int__:$PAYLOAD_BYTES")"
    body="$(call_tool read_project_artifact "$read_params")"
    if check_success "read_project_artifact succeeds" "$body"; then
        if [ "$(json_get "$body" output.content_base64)" = "$PAYLOAD_BASE64" ] && \
           [ "$(json_get "$body" output.eof)" = "True" ]; then
            pass "read_project_artifact returns expected base64 segment"
        else
            fail "read_project_artifact content/eof mismatch"
        fi
    fi
fi

abort_begin_params="$(make_json_object \
    project "$PROJECT_ID" \
    path "$ABORT_PATH" \
    expected_bytes __int__:1 \
    mime_type text/plain \
    overwrite __true__)"
body="$(call_tool artifact_upload_begin "$abort_begin_params")"
if check_success "artifact_upload_begin for abort cleanup succeeds" "$body"; then
    abort_upload_id="$(json_get "$body" output.upload_id)"
fi

if [ -n "$abort_upload_id" ]; then
    abort_params="$(make_json_object \
        project "$PROJECT_ID" \
        path "$ABORT_PATH" \
        upload_id "$abort_upload_id")"
    body="$(call_tool artifact_upload_abort "$abort_params")"
    if check_success "artifact_upload_abort cleanup succeeds" "$body"; then
        if [ "$(json_get "$body" output.aborted)" = "True" ]; then
            pass "artifact_upload_abort reports aborted=true"
        else
            fail "artifact_upload_abort did not report aborted=true"
        fi
    fi
fi

delete_params="$(python3 - "$PROJECT_ID" "$ARTIFACT_PATH" <<'PY'
import json
import sys

print(json.dumps({
    "project": sys.argv[1],
    "paths": [sys.argv[2]],
}, separators=(",", ":")))
PY
)"
body="$(call_tool delete_project_files "$delete_params")"
check_success "delete_project_files cleanup succeeds" "$body" || true

check_clean_git_status "post-cleanup" || true

log "---- summary ----"
log "passed: $PASS"
log "warned: $WARN"
log "failed: $FAIL"

if [ "$FAIL" -ne 0 ]; then
    exit 1
fi

log "artifact transfer smoke PASSED"
