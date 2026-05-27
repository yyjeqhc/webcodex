#!/usr/bin/env bash
set -uo pipefail

# ============================================================================
# Private Drop E2E Smoke Test
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TOKEN="e2e-test-token"
PORT=18080
BASE="http://127.0.0.1:$PORT"
TMPDIR_DATA=$(mktemp -d)
LOGFILE="$TMPDIR_DATA/server.log"
PIDFILE="$TMPDIR_DATA/server.pid"
PASS=0
FAIL=0
TOTAL=0

cleanup() {
    if [ -f "$PIDFILE" ]; then
        local pid
        pid=$(cat "$PIDFILE")
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    fi
    rm -rf "$TMPDIR_DATA"
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
    if echo "$haystack" | grep -q "$needle"; then
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
# Start server
# ============================================================================
echo "=== Starting server ==="
echo "  Data dir: $TMPDIR_DATA"
echo "  Log file: $LOGFILE"

DROP_TOKEN="$TOKEN" \
DROP_ADDR="127.0.0.1:$PORT" \
DROP_DATA="$TMPDIR_DATA" \
    ./target/release/private-drop > "$LOGFILE" 2>&1 &
echo $! > "$PIDFILE"

echo "  Waiting for server..."
READY=0
for i in $(seq 1 20); do
    if curl -sf "$BASE/api/health" > /dev/null 2>&1; then
        READY=1
        break
    fi
    sleep 0.5
done

if [ "$READY" -eq 0 ]; then
    echo "FATAL: Server did not start within 10 seconds"
    echo "Server log:"
    cat "$LOGFILE"
    exit 1
fi
echo "  Server ready (PID=$(cat "$PIDFILE"))"
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
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/messages")
assert_eq "GET /api/messages without token returns 401" "401" "$HTTP_CODE"

HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer wrong-token" "$BASE/api/messages")
assert_eq "GET /api/messages with wrong token returns 401" "401" "$HTTP_CODE"

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
assert_eq "Inbox has at least 1 message" "yes" "$([ "$TOTAL_MSGS" -ge 1 ] && echo yes || echo no)"
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

HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $TOKEN" "$BASE/api/messages/$MSG_10K_ID")
assert_eq "Deleted message returns 404" "404" "$HTTP_CODE"

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
assert_not_empty "Upload returns file id" "$FILE_ID"
assert_eq "Upload kind is file" "file" "$FILE_KIND"

# --- 10. Download file and verify content ---
echo ""
echo "--- 10. Download File ---"
curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/files/$FILE_ID" -o "$TMPDIR_DATA/downloaded.txt"
DOWNLOADED=$(cat "$TMPDIR_DATA/downloaded.txt")
if [ "$UPLOAD_CONTENT" = "$DOWNLOADED" ]; then
    log_pass "Downloaded file content matches uploaded content"
else
    log_fail "Downloaded file content mismatch"
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
