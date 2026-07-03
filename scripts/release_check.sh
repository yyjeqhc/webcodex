#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# WebCodex — Release Readiness Check
#
# Lightweight release-readiness gate. Runs focused local checks that must pass
# before final acceptance. It does NOT run the full suite, E2E smoke, eval
# harness, boot a public server, touch the network, or read/print real tokens,
# secrets, agent.toml, webcodex.env, or .env files.
#
# Stages:
#   1. cargo fmt --check
#   2. cargo check --all-targets
#   3. cargo test --bin webcodex metadata -- --nocapture
#   4. cargo test --bin webcodex schema -- --nocapture
#   5. cargo test --bin webcodex openapi -- --nocapture
#   6. cargo test --bin webcodex mcp -- --nocapture
#   7. bash syntax checks for scripts/*.sh
#   8. static: no python runtime helper regressions
#   9. static: no sensitive files tracked or staged by git
#
# Manual final acceptance steps live in docs/RELEASE_CHECKLIST.md:
#   - cargo test --bin webcodex -- --nocapture
#   - bash scripts/e2e_zero_config_ws.sh
#   - E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
#   - EVAL_MODE=compare bash scripts/eval_coding_loop.sh
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
# Stage 2: cargo check --all-targets
# ----------------------------------------------------------------------------
stage_start "cargo check --all-targets"
if cargo check --all-targets; then
    ok "cargo check --all-targets"
else
    die "cargo check --all-targets"
fi

# ----------------------------------------------------------------------------
# Stage 3: focused metadata tests
# ----------------------------------------------------------------------------
stage_start "cargo test --bin webcodex metadata -- --nocapture"
if cargo test --bin webcodex metadata -- --nocapture; then
    ok "metadata tests"
else
    die "metadata tests"
fi

# ----------------------------------------------------------------------------
# Stage 4: focused schema tests
# ----------------------------------------------------------------------------
stage_start "cargo test --bin webcodex schema -- --nocapture"
if cargo test --bin webcodex schema -- --nocapture; then
    ok "schema tests"
else
    die "schema tests"
fi

# ----------------------------------------------------------------------------
# Stage 5: focused OpenAPI tests
# ----------------------------------------------------------------------------
stage_start "cargo test --bin webcodex openapi -- --nocapture"
if cargo test --bin webcodex openapi -- --nocapture; then
    ok "openapi tests"
else
    die "openapi tests"
fi

# ----------------------------------------------------------------------------
# Stage 6: focused MCP tests
# ----------------------------------------------------------------------------
stage_start "cargo test --bin webcodex mcp -- --nocapture"
if cargo test --bin webcodex mcp -- --nocapture; then
    ok "mcp tests"
else
    die "mcp tests"
fi

# ----------------------------------------------------------------------------
# Stage 7: bash syntax checks
# ----------------------------------------------------------------------------
stage_start "bash syntax checks"
for script in scripts/*.sh; do
    if bash -n "$script"; then
        ok "bash -n $script"
    else
        die "bash syntax: $script"
    fi
done

# ----------------------------------------------------------------------------
# Stage 8: static — no python runtime helper regressions
# ----------------------------------------------------------------------------
stage_start "static: no python runtime helper regressions"
if grep -R "python3 -c" -n src/tool_runtime src/bin src/shell_client; then
    die "python3 -c in runtime paths"
else
    ok "no python3 -c in runtime paths"
fi
if grep -R "run_agent_helper" -n src/tool_runtime src/bin src/shell_client; then
    die "run_agent_helper in runtime paths"
else
    ok "no run_agent_helper in runtime paths"
fi

# ----------------------------------------------------------------------------
# Stage 9: static — no sensitive files tracked or staged by git
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
ok "fmt, check --all-targets, focused metadata/schema/openapi/mcp tests, bash syntax, static checks"
log "manual final acceptance: full suite, E2E websocket/polling, and eval compare (see docs/RELEASE_CHECKLIST.md)"
log "release readiness gate PASSED"
exit 0
