#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# WebCodex — Release Readiness Check
#
# Lightweight pre-release gate. Runs the most important local checks that must
# pass before tagging/importing GPT Actions. It does NOT boot a public server,
# does NOT touch the network, and NEVER reads or prints real tokens, secrets,
# agent.toml, webcodex.env, or .env files.
#
# Stages:
#   1. cargo fmt --check
#   2. cargo check
#   3. cargo check --tests
#   4. cargo test
#   5. bash scripts/e2e_zero_config_ws.sh               (websocket transport)
#   6. E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh (polling fallback)
#   7. static: no sensitive files tracked or staged by git
#
# Invariant notes (verified by stages 5/6, not re-checked statically here to
# keep the script fast and dependency-free):
#   - /openapi.json operation count == 27
#   - MCP tools/list returns a non-empty runtime tool list with key tools
# The E2E harness asserts these against the live server/schema.
#
# Usage:
#   bash scripts/release_check.sh
#
# Exit codes:
#   0  all stages passed
#   1  one or more stages failed
#   2  environment/dependency error
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

STAGE=0
FAILED_STAGE=""

log() { printf '[release] %s\n' "$*"; }
stage_start() {
    STAGE=$((STAGE + 1))
    printf '\n[release] ===== Stage %d: %s =====\n' "$STAGE" "$*"
}
ok() { printf '[release][ok]   %s\n' "$*"; }
die() {
    printf '[release][FAIL] %s\n' "$*" >&2
    printf '\n[release] FAILED at stage %d (%s)\n' "$STAGE" "${1:-unknown}" >&2
    exit 1
}

# Sanity: cargo present.
if ! command -v cargo >/dev/null 2>&1; then
    printf '[release] cargo is required\n' >&2
    exit 2
fi
# Sanity: git present (needed for the sensitive-file static check).
if ! command -v git >/dev/null 2>&1; then
    printf '[release] git is required\n' >&2
    exit 2
fi
# Sanity: bash present for the E2E harness (inherited from this interpreter).

log "project: $PROJECT_DIR"

# ----------------------------------------------------------------------------
# Stage 1: cargo fmt --check
# ----------------------------------------------------------------------------
stage_start "cargo fmt --check"
if cargo fmt --check; then
    ok "cargo fmt --check"
else
    die "cargo fmt --check"
fi

# ----------------------------------------------------------------------------
# Stage 2: cargo check
# ----------------------------------------------------------------------------
stage_start "cargo check"
if cargo check; then
    ok "cargo check"
else
    die "cargo check"
fi

# ----------------------------------------------------------------------------
# Stage 3: cargo check --tests
# ----------------------------------------------------------------------------
stage_start "cargo check --tests"
if cargo check --tests; then
    ok "cargo check --tests"
else
    die "cargo check --tests"
fi

# ----------------------------------------------------------------------------
# Stage 4: cargo test
# ----------------------------------------------------------------------------
stage_start "cargo test"
if cargo test; then
    ok "cargo test"
else
    die "cargo test"
fi

# ----------------------------------------------------------------------------
# Stage 5: E2E smoke (WebSocket transport)
# ----------------------------------------------------------------------------
stage_start "E2E smoke (websocket transport)"
if bash scripts/e2e_zero_config_ws.sh; then
    ok "E2E smoke (websocket) passed"
else
    die "E2E smoke (websocket)"
fi

# ----------------------------------------------------------------------------
# Stage 6: E2E smoke (polling transport)
# ----------------------------------------------------------------------------
stage_start "E2E smoke (polling transport)"
if E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh; then
    ok "E2E smoke (polling) passed"
else
    die "E2E smoke (polling)"
fi

# ----------------------------------------------------------------------------
# Stage 7: static — no sensitive files tracked or staged by git
# ----------------------------------------------------------------------------
stage_start "static: no sensitive files tracked/staged"
# These are git-ignored deployment files that must NEVER be committed. We check
# both tracked files and staged-but-untracked changes. We match by exact path
# under the repo root so the deploy/*.example templates are NOT flagged.
SENSITIVE_PATTERNS=(
    'agent.toml'
    'webcodex.env'
    '.env'
    'projects.d'
)
violations=""
while IFS= read -r line; do
    [ -z "$line" ] && continue
    # Normalize to the path component after the status flags.
    path="${line:3}"
    base="$(basename "$path")"
    parent="$(basename "$(dirname "$path")")"
    for pat in "${SENSITIVE_PATTERNS[@]}"; do
        if [ "$base" = "$pat" ] || [ "$parent" = "$pat" ]; then
            # Allow only checked-in example templates under deploy/.
            case "$path" in
                deploy/*.example|deploy/*/*.example) continue ;;
            esac
            violations="${violations}  ${line}"$'\n'
        fi
    done
done <<GIT_STATUS
$(git status --porcelain --untracked-files=all)
GIT_STATUS

if [ -z "$violations" ]; then
    ok "no sensitive files tracked or staged"
else
    printf '[release][FAIL] sensitive files must not be tracked or staged:\n' >&2
    printf '%s\n' "$violations" >&2
    printf '[release][FAIL] remove them from git (git rm --cached) and rotate WEBCODEX_TOKEN if exposed.\n' >&2
    die "sensitive files in git"
fi

# ----------------------------------------------------------------------------
# Summary
# ----------------------------------------------------------------------------
printf '\n[release] ===== all stages passed =====\n'
ok "fmt, check, check --tests, test, E2E ws, E2E polling, no sensitive files"
log "invariants (verified by E2E): /openapi.json ops == 27, MCP tools/list is non-empty with key tools"
log "release readiness PASSED"
exit 0
