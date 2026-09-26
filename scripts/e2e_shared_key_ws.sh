#!/usr/bin/env bash
set -euo pipefail

# WebPi product-boundary smoke for the retired shared-key server mode.
# The current product server must fail closed before opening its HTTP listener
# whenever WEBPI_SHARED_KEY_ENABLED=true. This keeps historical shared-key
# compatibility code from silently becoming a supported public auth mode.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_DIR"

CARGO_BIN="${CARGO_BIN:-cargo}"
TMP_ROOT=""
SERVER_PID=""

log() { printf '[shared-key-e2e] %s\n' "$*"; }
die() { printf '[shared-key-e2e][FAIL] %s\n' "$*" >&2; exit 1; }

cleanup() {
    trap - EXIT INT TERM
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    if [ -n "$TMP_ROOT" ]; then
        case "$TMP_ROOT" in
            /tmp/webpi-shared-key-e2e.*) rm -rf -- "$TMP_ROOT" ;;
            *) printf '[shared-key-e2e][WARN] refusing unexpected cleanup path: %s\n' "$TMP_ROOT" >&2 ;;
        esac
    fi
}
trap cleanup EXIT INT TERM

free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

if ! command -v python3 >/dev/null; then
    die "python3 is required"
fi

SERVER_BIN="${E2E_SERVER_BIN:-}"
if [ -z "$SERVER_BIN" ]; then
    command -v "$CARGO_BIN" >/dev/null || die "cargo is required when E2E_SERVER_BIN is not set"
    log "building WebPi Server"
    "$CARGO_BIN" build --quiet -p webcodex --bin webpi-server
    SERVER_BIN="$REPO_DIR/target/debug/webpi-server"
fi
[ -x "$SERVER_BIN" ] || die "WebPi Server binary is not executable: $SERVER_BIN"

PORT="$(free_port)"
TMP_ROOT="$(mktemp -d /tmp/webpi-shared-key-e2e.XXXXXX)"
DATA_DIR="$TMP_ROOT/data"
LOG_FILE="$TMP_ROOT/server.log"
mkdir -p "$DATA_DIR"

log "verifying shared-key mode is rejected before listener startup"
set +e
WEBPI_ADDR="127.0.0.1:${PORT}" \
WEBPI_DATA="$DATA_DIR" \
WEBPI_TOKEN="bootstrap-e2e-only" \
WEBPI_SHARED_KEY_ENABLED=true \
WEBPI_ALLOW_ANONYMOUS=false \
WEBPI_OAUTH2_SHARED_KEY_BRIDGE=false \
WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED=false \
RUST_LOG=warn \
"$SERVER_BIN" >"$LOG_FILE" 2>&1 &
SERVER_PID=$!
wait "$SERVER_PID"
status=$?
SERVER_PID=""
set -e

[ "$status" -ne 0 ] || die "shared-key-enabled WebPi Server unexpectedly stayed running"

expected='WebPi requires a non-empty bootstrap credential and disabled anonymous/shared-key/query-token modes'
grep -Fq "$expected" "$LOG_FILE" \
    || die "shared-key rejection did not preserve the fail-closed diagnostic"

if python3 - "$PORT" <<'PY'
import socket, sys
port = int(sys.argv[1])
s = socket.socket()
s.settimeout(0.3)
try:
    rc = s.connect_ex(("127.0.0.1", port))
finally:
    s.close()
raise SystemExit(0 if rc != 0 else 1)
PY
then
    log "listener remained closed as required"
else
    die "shared-key rejection occurred after a listener became reachable"
fi

log "PASS: retired shared-key mode fails closed before listener startup"
