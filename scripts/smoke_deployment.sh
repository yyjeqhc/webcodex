#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# WebCodex — Deployment Smoke
#
# Lightweight smoke test against an ALREADY-DEPLOYED WebCodex instance.
# It does NOT start a server or agent. It verifies the public surface is
# reachable, auth works, and the GPT Actions + MCP endpoints respond.
#
# Usage:
#   WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
#   WEBCODEX_TOKEN="<your-secret>" \
#   bash scripts/smoke_deployment.sh
#
# Environment overrides:
#   WEBCODEX_PUBLIC_URL  (or BASE_URL)  base URL of the deployed instance
#   WEBCODEX_TOKEN       (or TOKEN)     bearer token; NEVER printed by this script
#   SMOKE_TIMEOUT    per-curl timeout in seconds (default 15)
#
# What it checks:
#   1. GET  /openapi.json            -> valid OpenAPI JSON with paths.
#   2. POST /api/runtime/status      -> success == true.
#   3. POST /api/projects/list       -> success == true.
#   4. POST /mcp initialize          -> result.protocolVersion non-empty.
#   5. POST /mcp tools/list          -> result.tools is a non-empty array.
#
# It uses only curl + python3 (no jq dependency) and never prints the token.
#
# Exit codes:
#   0  all checks passed
#   1  one or more checks failed
#   2  environment/dependency error
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

# Resolve base URL (WEBCODEX_PUBLIC_URL preferred, BASE_URL fallback).
BASE_URL="${WEBCODEX_PUBLIC_URL:-${BASE_URL:-}}"
TOKEN="${WEBCODEX_TOKEN:-${TOKEN:-}}"
TIMEOUT="${SMOKE_TIMEOUT:-15}"

if [ -z "$BASE_URL" ]; then
    echo "[smoke] WEBCODEX_PUBLIC_URL (or BASE_URL) is required" >&2
    exit 2
fi
if [ -z "$TOKEN" ]; then
    echo "[smoke] WEBCODEX_TOKEN (or TOKEN) is required" >&2
    exit 2
fi

# Normalize: strip trailing slash.
BASE_URL="${BASE_URL%/}"

if ! command -v curl >/dev/null 2>&1; then
    echo "[smoke] curl is required" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "[smoke] python3 is required (for JSON checks)" >&2
    exit 2
fi

PASS=0
FAIL=0

log() { printf '[smoke] %s\n' "$*"; }
pass() { PASS=$((PASS + 1)); printf '[smoke][ok]   %s\n' "$*"; }
fail() { FAIL=$((FAIL + 1)); printf '[smoke][FAIL] %s\n' "$*" >&2; }

# curl wrapper: auth + timeout. The token is passed via a header file to avoid
# ever leaking it on the command line / process list. Body printed to stdout.
AUTH_HEADER_FILE="$(mktemp -t webcodex-smoke-auth-XXXXXX)"
trap 'rm -f "$AUTH_HEADER_FILE"' INT TERM EXIT
printf 'Authorization: Bearer %s\n' "$TOKEN" > "$AUTH_HEADER_FILE"
chmod 600 "$AUTH_HEADER_FILE"

api_get() {
    local path="$1"
    curl -sS --max-time "$TIMEOUT" \
        -H @"$AUTH_HEADER_FILE" \
        "${BASE_URL}${path}" 2>/dev/null
}

api_post() {
    local path="$1"
    local body="${2:-}"
    if [ -z "$body" ]; then
        body="{}"
    fi
    curl -sS --max-time "$TIMEOUT" \
        -H @"$AUTH_HEADER_FILE" \
        -H "Content-Type: application/json" \
        -X POST "${BASE_URL}${path}" \
        -d "$body" 2>/dev/null
}

# Extract a JSON field with python3 (no jq). Prints "" on any error.
json_get() {
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

# ----------------------------------------------------------------------------
# 1. GET /openapi.json
# ----------------------------------------------------------------------------

log "GET /openapi.json"
body="$(api_get /openapi.json || true)"
paths_json="$(python3 -c '
import json, sys
try:
    d = json.loads(sys.stdin.read())
    print(len(d.get("paths", {})))
except Exception:
    print(0)
' <<<"$body" 2>/dev/null || echo 0)"
if [ "${paths_json:-0}" -gt 0 ]; then
    pass "/openapi.json returns a schema with ${paths_json} path(s)"
else
    fail "/openapi.json did not return a valid OpenAPI schema (paths=0)"
fi

# ----------------------------------------------------------------------------
# 2. POST /api/runtime/status
# ----------------------------------------------------------------------------

log "POST /api/runtime/status"
body="$(api_post /api/runtime/status '{}')"
ok="$(json_get "$body" success)"
if [ "$ok" = "True" ]; then
    pass "/api/runtime/status success=true"
else
    fail "/api/runtime/status not success (got: ${body:0:300})"
fi

# ----------------------------------------------------------------------------
# 3. POST /api/projects/list
# ----------------------------------------------------------------------------

log "POST /api/projects/list"
body="$(api_post /api/projects/list '{}')"
ok="$(json_get "$body" success)"
if [ "$ok" = "True" ]; then
    pass "/api/projects/list success=true"
else
    fail "/api/projects/list not success (got: ${body:0:300})"
fi

# ----------------------------------------------------------------------------
# 4. POST /mcp initialize
# ----------------------------------------------------------------------------

log "POST /mcp initialize"
body="$(api_post /mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}')"
proto="$(json_get "$body" result.protocolVersion)"
if [ -n "$proto" ] && [ "$proto" != "" ]; then
    pass "/mcp initialize returns protocolVersion=$proto"
else
    fail "/mcp initialize did not return a protocolVersion (got: ${body:0:300})"
fi

# ----------------------------------------------------------------------------
# 5. POST /mcp tools/list
# ----------------------------------------------------------------------------

log "POST /mcp tools/list"
body="$(api_post /mcp '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')"
tools_count="$(python3 -c '
import json, sys
try:
    d = json.loads(sys.stdin.read())
    print(len(d.get("result", {}).get("tools", [])))
except Exception:
    print(0)
' <<<"$body" 2>/dev/null || echo 0)"
if [ "${tools_count:-0}" -gt 0 ]; then
    pass "/mcp tools/list returned ${tools_count} tool(s)"
else
    fail "/mcp tools/list returned no tools (got: ${body:0:300})"
fi

# ----------------------------------------------------------------------------
# Summary
# ----------------------------------------------------------------------------

log "---- summary ----"
log "passed: $PASS"
log "failed: $FAIL"

if [ "$FAIL" -ne 0 ]; then
    exit 1
fi

log "deployment smoke PASSED"
exit 0
