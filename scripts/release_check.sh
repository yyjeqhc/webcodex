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
#   1. workspace boundary check
#   2. cargo fmt --all -- --check
#   3. cargo check --workspace --all-targets
#   4. cargo test -p webcodex --lib metadata -- --nocapture
#   5. cargo test -p webcodex --lib schema -- --nocapture
#   6. cargo test -p webcodex --lib openapi -- --nocapture
#   7. cargo test -p webcodex --lib mcp -- --nocapture
#   8. bash syntax checks for scripts/*.sh
#   9. release verification tooling self-tests
#  10. static: test harnesses use current runtime contracts
#  11. static: no python runtime helper regressions
#  12. static: no sensitive files tracked or staged by git
#
# Final pre-tag acceptance is orchestrated by .github/workflows/release-readiness.yml:
#   Stage 1 (all test gates in parallel, before any release-profile/image build):
#   - this canonical release_check.sh
#   - complete locked Rust workspace tests, package-sharded without test-name filters
#   - frontend typecheck/test/committed-build check
#   - WebSocket + polling zero-config E2E
#   - EVAL_MODE=compare bash scripts/eval_coding_loop.sh with prebuilt debug fixtures
#   Stage 2 (parallel fanout only after every Stage-1 test gate succeeds):
#   - five native release-profile surfaces + two disposable Server-image architectures
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
# Stage 1: workspace boundary check
# ----------------------------------------------------------------------------
stage_start "workspace boundary check"
if bash scripts/workspace_boundary_check.sh; then
    ok "workspace boundary check"
else
    die "workspace boundary check"
fi

# ----------------------------------------------------------------------------
# Stage 2: cargo fmt --all -- --check
# ----------------------------------------------------------------------------
stage_start "cargo fmt --all -- --check"
if cargo fmt --all -- --check; then
    ok "cargo fmt --all -- --check"
else
    die "cargo fmt --all -- --check"
fi

# ----------------------------------------------------------------------------
# Stage 3: cargo check --workspace --all-targets
# ----------------------------------------------------------------------------
stage_start "cargo check --workspace --all-targets"
if cargo check --workspace --all-targets; then
    ok "cargo check --workspace --all-targets"
else
    die "cargo check --workspace --all-targets"
fi

# ----------------------------------------------------------------------------
# Stage 4: focused metadata tests
# ----------------------------------------------------------------------------
stage_start "cargo test -p webcodex --lib metadata -- --nocapture"
if cargo test -p webcodex --lib metadata -- --nocapture; then
    ok "metadata tests"
else
    die "metadata tests"
fi

# ----------------------------------------------------------------------------
# Stage 5: focused schema tests
# ----------------------------------------------------------------------------
stage_start "cargo test -p webcodex --lib schema -- --nocapture"
if cargo test -p webcodex --lib schema -- --nocapture; then
    ok "schema tests"
else
    die "schema tests"
fi

# ----------------------------------------------------------------------------
# Stage 6: focused OpenAPI tests
# ----------------------------------------------------------------------------
stage_start "cargo test -p webcodex --lib openapi -- --nocapture"
if cargo test -p webcodex --lib openapi -- --nocapture; then
    ok "openapi tests"
else
    die "openapi tests"
fi

# ----------------------------------------------------------------------------
# Stage 7: focused MCP tests
# ----------------------------------------------------------------------------
stage_start "cargo test -p webcodex --lib mcp -- --nocapture"
if cargo test -p webcodex --lib mcp -- --nocapture; then
    ok "mcp tests"
else
    die "mcp tests"
fi

# ----------------------------------------------------------------------------
# Stage 8: bash syntax checks
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
# Stage 9: release verification tooling self-tests
# ----------------------------------------------------------------------------
stage_start "release verification tooling self-tests"
if bash scripts/test_python_tooling.sh \
    && python3 scripts/release_operator.py --help >/dev/null \
    && python3 scripts/release_operator.py preflight --help >/dev/null \
    && python3 scripts/release_operator.py reclaim-tag --help >/dev/null \
    && python3 scripts/release_operator.py readiness-start --help >/dev/null \
    && python3 scripts/release_operator.py readiness-status --help >/dev/null \
    && python3 scripts/release_operator.py build-start --help >/dev/null \
    && python3 scripts/release_operator.py build-status --help >/dev/null \
    && python3 scripts/release_operator.py collect --help >/dev/null \
    && python3 scripts/release_operator.py stage-npm --help >/dev/null \
    && python3 scripts/release_operator.py verify-draft --help >/dev/null \
    && python3 scripts/check_markdown_links.py \
    && bash scripts/tests/test_npm_package_smoke_existing_binaries.sh; then
    ok "release verification tooling self-tests"
else
    die "release verification tooling self-tests"
fi

# ----------------------------------------------------------------------------
# Stage 10: static — test harnesses use current runtime contracts
# ----------------------------------------------------------------------------
stage_start "static: current test harness contracts"
if grep -En -- '--bin webcodex([[:space:]]|`|$)|target/debug/webcodex([^/-]|$)|include_runtime_status|include_git|include_recent_commits|include_rules|process_local_in_memory|output\.content|numbered_text' \
    scripts/e2e_zero_config_ws.sh \
    scripts/e2e_reconnect_ws.sh \
    scripts/eval_coding_loop.sh \
    scripts/test-agent-config-reload-e2e.sh \
    scripts/test-claude-provider-e2e.sh \
    docs/TESTING.md \
    docs/RELEASE_CHECKLIST.md; then
    die "stale runtime target or startup field in test harness guidance"
else
    ok "test harnesses use current server target and startup fields"
fi
if grep -En -- 'binding loss|lost process-local binding|lost_after_restart=true|binding_is_process_local_and_principal_scoped' \
    scripts/e2e_reconnect_ws.sh \
    docs/ARCHITECTURE.md \
    docs/RUNNER.md \
    docs/TESTING.md \
    docs/agent/architecture-decisions.md; then
    die "stale current-session binding durability guidance"
else
    ok "binding guidance reflects durable exact restoration"
fi

# ----------------------------------------------------------------------------
# Stage 11: static — no python runtime helper regressions
# ----------------------------------------------------------------------------
stage_start "static: no python runtime helper regressions"
if grep -R "python3 -c" -n src/tool_runtime src/shell_client crates/webcodex-runner/src; then
    die "python3 -c in runtime paths"
else
    ok "no python3 -c in runtime paths"
fi
if grep -R "run_agent_helper" -n src/tool_runtime src/shell_client crates/webcodex-runner/src; then
    die "run_agent_helper in runtime paths"
else
    ok "no run_agent_helper in runtime paths"
fi

# ----------------------------------------------------------------------------
# Stage 12: static — no sensitive files tracked or staged by git
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
ok "workspace boundaries, fmt, check --all-targets, focused metadata/schema/openapi/mcp tests, bash syntax, release tooling self-tests, harness contracts, static checks"
log "final pre-tag acceptance: dispatch the exact-source release-readiness workflow (see docs/RELEASE_CHECKLIST.md)"
log "release readiness gate PASSED"
exit 0
