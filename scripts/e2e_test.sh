#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# Private Drop E2E Smoke Test
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TOKEN="e2e-test-token"
TMPDIR_DATA=$(mktemp -d)
LOGFILE="$TMPDIR_DATA/server.log"
PASS=0
FAIL=0
TOTAL=0
SERVER_PID=""

# Find a free port
find_free_port() {
    python3 -c "
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
"
}

PORT=$(find_free_port)
BASE="http://127.0.0.1:$PORT"

cleanup() {
    echo ""
    echo "=== Cleanup ==="
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "  Stopping server (PID=$SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        # Wait briefly for graceful shutdown, then force kill
        for _ in $(seq 1 10); do
            kill -0 "$SERVER_PID" 2>/dev/null || break
            sleep 0.2
        done
        kill -9 "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
        echo "  Server stopped."
    else
        echo "  Server PID=$SERVER_PID already exited."
    fi
    rm -rf "$TMPDIR_DATA"
    echo "  Temp dir removed."

    # Report residual processes (do NOT kill them)
    local residual
    residual=$(pgrep -f "private-drop" 2>/dev/null || true)
    if [ -n "$residual" ]; then
        echo "  WARNING: Residual private-drop processes found: $residual"
    fi
}
trap cleanup EXIT

log_pass() {
    PASS=$((PASS + 1))
    TOTAL=$((TOTAL + 1))
    echo "  PASS: $1"
}

log_fail() {
    FAIL=$((FAIL + 1))
    TOTAL=$((TOTAL + 1))
    echo "  FAIL: $1"
    if [ -n "${2:-}" ]; then
        echo "    Response: $2"
    fi
    if [ -f "$LOGFILE" ]; then
        echo "    Last 10 lines of server log:"
        tail -10 "$LOGFILE" | sed 's/^/      /'
    fi
}

assert_eq() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        log_pass "$desc"
    else
        log_fail "$desc" "expected='$expected' got='$actual'"
    fi
}

assert_contains() {
    local desc="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -qF "$needle"; then
        log_pass "$desc"
    else
        log_fail "$desc" "expected to contain '$needle'"
    fi
}

assert_not_empty() {
    local desc="$1" val="$2"
    if [ -n "$val" ]; then
        log_pass "$desc"
    else
        log_fail "$desc" "expected non-empty value"
    fi
}

assert_http_code() {
    local desc="$1" expected="$2" url="$3"
    shift 3
    local code
    code=$(curl -s -o /dev/null -w '%{http_code}' "$@" "$url")
    assert_eq "$desc" "$expected" "$code"
}

# Python JSON parser helper
pyget() {
    local json="$1" path="$2"
    echo "$json" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    for k in '$path'.split('.'):
        if k.isdigit():
            data = data[int(k)]
        else:
            data = data[k]
    print(data)
except Exception:
    print('')
" 2>/dev/null
}

# ============================================================================
# Build
# ============================================================================
echo "=== Building ==="
cargo fmt
cargo test
cargo build --release
echo "Build OK"
echo ""

# ============================================================================
# Check for residual processes (report only)
# ============================================================================
RESIDUAL_BEFORE=$(pgrep -f "private-drop" 2>/dev/null || true)
if [ -n "$RESIDUAL_BEFORE" ]; then
    echo "NOTE: Pre-existing private-drop processes: $RESIDUAL_BEFORE"
fi

# ============================================================================
# Start server
# ============================================================================
echo "=== Starting server ==="
echo "  Port: $PORT"
echo "  Data dir: $TMPDIR_DATA"
echo "  Log file: $LOGFILE"

DROP_TOKEN="$TOKEN" \
DROP_ADDR="127.0.0.1:$PORT" \
DROP_DATA="$TMPDIR_DATA" \
    ./target/release/private-drop > "$LOGFILE" 2>&1 &
SERVER_PID=$!
echo "  Server PID: $SERVER_PID"

# Wait for server to be ready, checking that PID stays alive
echo "  Waiting for server..."
READY=0
for _ in $(seq 1 40); do
    # Check if server process is still alive
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "FATAL: Server process $SERVER_PID exited prematurely"
        echo "Server log:"
        cat "$LOGFILE"
        exit 1
    fi
    if curl -sf "$BASE/api/health" > /dev/null 2>&1; then
        READY=1
        break
    fi
    sleep 0.25
done

if [ "$READY" -eq 0 ]; then
    echo "FATAL: Server did not become ready within 10 seconds"
    echo "Server log:"
    cat "$LOGFILE"
    exit 1
fi
echo "  Server ready"
echo ""

# ============================================================================
# Tests
# ============================================================================
echo "=== Running E2E Tests ==="

# --- 1. Health check (no auth) ---
echo ""
echo "--- 1. Health Check ---"
RESP=$(curl -sf "$BASE/api/health")
STATUS=$(pyget "$RESP" "status")
assert_eq "GET /api/health returns ok" "ok" "$STATUS"

# --- 2. Unauthorized access ---
echo ""
echo "--- 2. Auth ---"
assert_http_code "GET /api/messages without token returns 401" "401" "$BASE/api/messages"
assert_http_code "GET /api/messages with wrong token returns 401" "401" "$BASE/api/messages" \
    -H "Authorization: Bearer wrong-token"
assert_http_code "POST /api/messages without token returns 401" "401" "$BASE/api/messages" \
    -X POST -H "Content-Type: application/json" -d '{"channel":"inbox","text":"test"}'

# --- 3. Create text message ---
echo ""
echo "--- 3. Create Text Message ---"
RESP=$(curl -sf -X POST "$BASE/api/messages" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"channel":"inbox","title":"E2E Test","text":"Hello from e2e test!"}')
MSG_ID=$(pyget "$RESP" "id")
MSG_KIND=$(pyget "$RESP" "kind")
assert_not_empty "Create message returns id" "$MSG_ID"
assert_eq "Create message kind is text" "text" "$MSG_KIND"

# --- 4. List inbox messages ---
echo ""
echo "--- 4. List Inbox Messages ---"
RESP=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/messages?channel=inbox")
TOTAL_MSGS=$(pyget "$RESP" "total")
FOUND=$(echo "$RESP" | python3 -c "
import sys, json
data = json.load(sys.stdin)
print('yes' if any(m['id'] == '$MSG_ID' for m in data['messages']) else 'no')
")
assert_eq "Inbox has at least 1 message" "yes" "$([ "${TOTAL_MSGS:-0}" -ge 1 ] && echo yes || echo no)"
assert_eq "Created message found in inbox list" "yes" "$FOUND"

# --- 5. Create 10K text message ---
echo ""
echo "--- 5. Create 10K Text Message ---"
LONG_10K=$(python3 -c "print('A' * 10240)")
RESP=$(curl -sf -X POST "$BASE/api/messages" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"channel\":\"inbox\",\"title\":\"10K Text\",\"text\":\"$LONG_10K\"}")
MSG_10K_ID=$(pyget "$RESP" "id")
assert_not_empty "10K text message created" "$MSG_10K_ID"

# --- 6. Create 100K text message ---
echo ""
echo "--- 6. Create 100K Text Message ---"
LONG_100K=$(python3 -c "print('B' * 102400)")
RESP=$(curl -sf -X POST "$BASE/api/messages" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"channel\":\"inbox\",\"title\":\"100K Text\",\"text\":\"$LONG_100K\"}")
MSG_100K_ID=$(pyget "$RESP" "id")
assert_not_empty "100K text message created" "$MSG_100K_ID"

# --- 7. Get message detail ---
echo ""
echo "--- 7. Get Message Detail ---"
RESP=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/messages/$MSG_ID")
DETAIL_TITLE=$(pyget "$RESP" "title")
DETAIL_TEXT=$(pyget "$RESP" "text")
assert_eq "Get message title matches" "E2E Test" "$DETAIL_TITLE"
assert_eq "Get message text matches" "Hello from e2e test!" "$DETAIL_TEXT"

# --- 8. Delete message ---
echo ""
echo "--- 8. Delete Message ---"
RESP=$(curl -sf -X DELETE -H "Authorization: Bearer $TOKEN" "$BASE/api/messages/$MSG_10K_ID")
DELETED=$(pyget "$RESP" "deleted")
assert_eq "Delete returns deleted=true" "True" "$DELETED"
assert_http_code "Deleted message returns 404" "404" "$BASE/api/messages/$MSG_10K_ID" \
    -H "Authorization: Bearer $TOKEN"

# --- 9. Upload file ---
echo ""
echo "--- 9. Upload File ---"
UPLOAD_CONTENT="This is the e2e test file content. Timestamp: $(date +%s)"
echo "$UPLOAD_CONTENT" > "$TMPDIR_DATA/upload.txt"
RESP=$(curl -sf -X POST "$BASE/api/files?channel=files" \
    -H "Authorization: Bearer $TOKEN" \
    -F "file=@$TMPDIR_DATA/upload.txt")
FILE_ID=$(pyget "$RESP" "id")
FILE_KIND=$(pyget "$RESP" "kind")
FILE_NAME=$(pyget "$RESP" "file_name")
assert_not_empty "Upload returns file id" "$FILE_ID"
assert_eq "Upload kind is file" "file" "$FILE_KIND"
assert_eq "Upload file_name is upload.txt" "upload.txt" "$FILE_NAME"

# --- 10. Download file and verify content + headers ---
echo ""
echo "--- 10. Download File ---"
# Download and capture headers
HEADERS=$(curl -s -D - -H "Authorization: Bearer $TOKEN" "$BASE/api/files/$FILE_ID" -o "$TMPDIR_DATA/downloaded.txt")
DOWNLOADED=$(cat "$TMPDIR_DATA/downloaded.txt")
if [ "$UPLOAD_CONTENT" = "$DOWNLOADED" ]; then
    log_pass "Downloaded file content matches uploaded content"
else
    log_fail "Downloaded file content mismatch"
fi
# Check Content-Disposition header
if echo "$HEADERS" | grep -qi 'content-disposition.*upload\.txt'; then
    log_pass "Download response has Content-Disposition with filename"
else
    log_fail "Download response missing Content-Disposition with filename"
fi

# --- 11. OpenAPI spec ---
echo ""
echo "--- 11. OpenAPI Spec ---"
RESP=$(curl -sf "$BASE/openapi.json")
HAS_CREATE=$(echo "$RESP" | python3 -c "import sys; print('yes' if 'createMessage' in sys.stdin.read() else 'no')")
HAS_LIST=$(echo "$RESP" | python3 -c "import sys; print('yes' if 'listMessages' in sys.stdin.read() else 'no')")
assert_contains "OpenAPI contains createMessage" "yes" "$HAS_CREATE"
assert_contains "OpenAPI contains listMessages" "yes" "$HAS_LIST"

# --- 12. Channels ---
echo ""
echo "--- 12. Channels ---"
RESP=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/channels")
HAS_INBOX=$(echo "$RESP" | python3 -c "import sys; print('yes' if 'inbox' in sys.stdin.read() else 'no')")
assert_contains "Channels list contains inbox" "yes" "$HAS_INBOX"

# --- 13. Web UI: Login page ---
echo ""
echo "--- 13. Web UI ---"
# Web UI is client-side rendered; pages return HTML shells with JS
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/login")
assert_eq "GET /login returns 200" "200" "$HTTP_CODE"
BODY=$(curl -sf "$BASE/login")
assert_contains "Login page references drop_token" "drop_token" "$BODY"
assert_contains "Login page redirects to /c/inbox" "/c/inbox" "$BODY"

# --- 14. Web UI: Home page ---
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/")
assert_eq "GET / returns 200" "200" "$HTTP_CODE"
BODY=$(curl -sf "$BASE/")
assert_contains "Home page references drop_token" "drop_token" "$BODY"
assert_contains "Home page references /c/inbox" "/c/inbox" "$BODY"

# --- 15. Web UI: Channel page ---
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/c/inbox")
assert_eq "GET /c/inbox returns 200" "200" "$HTTP_CODE"
BODY=$(curl -sf "$BASE/c/inbox")
assert_contains "Channel page calls /api/messages" "/api/messages" "$BODY"
assert_contains "Channel page uses Authorization" "Authorization" "$BODY"
assert_contains "Channel page uses Bearer" "Bearer" "$BODY"

HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/c/xline")
assert_eq "GET /c/xline returns 200" "200" "$HTTP_CODE"

# --- 16. Web UI: Message detail page ---
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/m/$MSG_ID")
assert_eq "GET /m/{id} returns 200" "200" "$HTTP_CODE"

# --- 17. Web UI: Send page ---
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/send")
assert_eq "GET /send returns 200" "200" "$HTTP_CODE"
BODY=$(curl -sf "$BASE/send")
assert_contains "Send page calls POST /api/messages" "/api/messages" "$BODY"
assert_contains "Send page uses Authorization" "Authorization" "$BODY"
assert_contains "Send page uses Bearer" "Bearer" "$BODY"

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "============================================"
echo "  E2E Test Results: $PASS passed, $FAIL failed, $TOTAL total"
echo "============================================"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "Server log (last 20 lines):"
    tail -20 "$LOGFILE" | sed 's/^/  /'
    exit 1
fi

echo "All tests passed!"
exit 0
