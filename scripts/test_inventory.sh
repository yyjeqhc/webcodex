#!/usr/bin/env bash
set -euo pipefail

# Heuristic, read-only test inventory for WebCodex.
#
# Scope:
#   - scans Git-tracked Rust files across the whole workspace
#   - groups the root package and crates/* sources for later lane work
#   - does not access the network
#   - does not modify the repository
#   - avoids printing matched source lines so token-looking fixture values are
#     not echoed by this script

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    printf 'usage: bash scripts/test_inventory.sh [--details|--self-test]\n'
}

if [ "${1:-}" = "--self-test" ]; then
    if [ "$#" -ne 1 ]; then
        usage >&2
        exit 2
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        printf '[inventory][FAIL] python3 is required for --self-test\n' >&2
        exit 2
    fi
    cd "$PROJECT_DIR"
    PYTHONDONTWRITEBYTECODE=1 \
        exec python3 -m unittest scripts.tests.test_test_inventory_script
fi

DETAILS=0
if [ "$#" -gt 0 ]; then
    case "$1" in
        --details)
            if [ "$#" -ne 1 ]; then
                usage >&2
                exit 2
            fi
            DETAILS=1
            ;;
        -h|--help)
            if [ "$#" -ne 1 ]; then
                usage >&2
                exit 2
            fi
            usage
            exit 0
            ;;
        *)
            printf '[inventory] unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
fi

for required in git rg; do
    if ! command -v "$required" >/dev/null 2>&1; then
        printf '[inventory][FAIL] %s is required\n' "$required" >&2
        exit 2
    fi
done

cd "$PROJECT_DIR"
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    printf '[inventory][FAIL] project root is not a Git worktree\n' >&2
    exit 2
fi

rust_files=()
while IFS= read -r -d '' file; do
    rust_files+=("$file")
done < <(git ls-files -z -- '*.rs')

if [ "${#rust_files[@]}" -eq 0 ]; then
    printf '[inventory] no Git-tracked Rust files found\n' >&2
    exit 1
fi

TEST_PATTERN='^[[:space:]]*#\[test'
TOKIO_TEST_PATTERN='^[[:space:]]*#\[tokio::test'
IGNORE_PATTERN='^[[:space:]]*#\[ignore'
SLEEP_PATTERN='sleep[[:space:]]*\('
TIMEOUT_PATTERN='timeout[[:space:]]*\('
LOOPBACK_PATTERN='localhost|127\.0\.0\.1|TcpListener'
ENV_MUTATION_PATTERN='(std::)?env::(set_var|remove_var)'
TEST_ENV_LOCK_PATTERN='TEST_ENV_LOCK'

rg_count_files() {
    local pattern="$1"
    shift
    if [ "$#" -eq 0 ]; then
        printf '0\n'
        return 0
    fi

    local output status
    set +e
    output="$(rg --count-matches "$pattern" -- "$@" 2>/dev/null)"
    status=$?
    set -e
    if [ "$status" -eq 1 ]; then
        printf '0\n'
        return 0
    fi
    if [ "$status" -ne 0 ]; then
        printf '[inventory] rg failed for pattern: %s\n' "$pattern" >&2
        return "$status"
    fi
    printf '%s\n' "$output" | awk -F: '{ sum += $NF } END { print sum + 0 }'
}

rg_count() {
    local pattern="$1"
    rg_count_files "$pattern" "${rust_files[@]}"
}

rg_locations() {
    local label="$1"
    local pattern="$2"
    local status
    set +e
    rg --line-number --no-heading "$pattern" -- "${rust_files[@]}" 2>/dev/null \
        | awk -F: -v label="$label" '{ print $1 ":" $2 ":" label }'
    status=${PIPESTATUS[0]}
    set -e
    if [ "$status" -eq 1 ]; then
        return 0
    fi
    if [ "$status" -ne 0 ]; then
        printf '[inventory] rg failed for pattern: %s\n' "$pattern" >&2
        return "$status"
    fi
}

rg_file_counts() {
    local label="$1"
    local pattern="$2"
    local status
    set +e
    rg --line-number --no-heading "$pattern" -- "${rust_files[@]}" 2>/dev/null \
        | awk -F: -v label="$label" '{ count[$1]++ } END { for (file in count) print count[file] "\t" file "\t" label }' \
        | sort -t $'\t' -k1,1nr -k2,2 \
        | head -n 10 \
        | awk -F'\t' '{ print "  " $3 " " $2 ": " $1 }'
    status=${PIPESTATUS[0]}
    set -e
    if [ "$status" -eq 1 ]; then
        return 0
    fi
    if [ "$status" -ne 0 ]; then
        printf '[inventory] rg failed for pattern: %s\n' "$pattern" >&2
        return "$status"
    fi
}

area_for_file() {
    local file="$1"
    local rest crate
    case "$file" in
        crates/*/*)
            rest="${file#crates/}"
            crate="${rest%%/*}"
            printf 'crates/%s\n' "$crate"
            ;;
        src/*|tests/*)
            printf 'webcodex\n'
            ;;
        *)
            printf 'other\n'
            ;;
    esac
}

areas=()
while IFS= read -r area; do
    areas+=("$area")
done < <(
    for file in "${rust_files[@]}"; do
        area_for_file "$file"
    done | sort -u
)

print_ignored_tests() {
    awk '
        /^[[:space:]]*#\[ignore/ {
            pending = 1
            ignore_line = FNR
            next
        }
        pending && /^[[:space:]]*#\[/ {
            next
        }
        pending && /^[[:space:]]*(async[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]+/ {
            name = $0
            sub(/^[[:space:]]*/, "", name)
            sub(/^async[[:space:]]+/, "", name)
            sub(/^fn[[:space:]]+/, "", name)
            sub(/\(.*/, "", name)
            print FILENAME ":" ignore_line ":" name
            pending = 0
            next
        }
        pending && FNR > ignore_line + 8 {
            pending = 0
        }
    ' "${rust_files[@]}"
}

printf '[inventory] source\n'
printf '  scope: Git-tracked Rust files across the workspace\n'
printf '  rust files: %s\n' "${#rust_files[@]}"
printf '\n'

printf '[inventory] test attributes\n'
printf '  #[test]: %s\n' "$(rg_count "$TEST_PATTERN")"
printf '  #[tokio::test]: %s\n' "$(rg_count "$TOKIO_TEST_PATTERN")"
printf '  #[ignore]: %s\n' "$(rg_count "$IGNORE_PATTERN")"
printf '\n'

printf '[inventory] risk clue counts\n'
printf '  sleep calls: %s\n' "$(rg_count "$SLEEP_PATTERN")"
printf '  timeout calls: %s\n' "$(rg_count "$TIMEOUT_PATTERN")"
printf '  loopback strings or TcpListener: %s\n' "$(rg_count "$LOOPBACK_PATTERN")"
printf '  env set/remove calls: %s\n' "$(rg_count "$ENV_MUTATION_PATTERN")"
printf '  TEST_ENV_LOCK mentions: %s\n' "$(rg_count "$TEST_ENV_LOCK_PATTERN")"
printf '\n'

printf '[inventory] area summary (tab-separated)\n'
printf 'area\trust_files\ttest\ttokio_test\tignore\tsleep\ttimeout\tloopback_or_listener\tenv_mutation\ttest_env_lock\n'
for area in "${areas[@]}"; do
    area_files=()
    for file in "${rust_files[@]}"; do
        if [ "$(area_for_file "$file")" = "$area" ]; then
            area_files+=("$file")
        fi
    done
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$area" \
        "${#area_files[@]}" \
        "$(rg_count_files "$TEST_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$TOKIO_TEST_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$IGNORE_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$SLEEP_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$TIMEOUT_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$LOOPBACK_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$ENV_MUTATION_PATTERN" "${area_files[@]}")" \
        "$(rg_count_files "$TEST_ENV_LOCK_PATTERN" "${area_files[@]}")"
done
printf '\n'

printf '[inventory] ignored tests\n'
ignored_tests="$(print_ignored_tests)"
if [ -n "$ignored_tests" ]; then
    printf '%s\n' "$ignored_tests" | sed 's/^/  /'
else
    printf '  none found\n'
fi
printf '\n'

if [ "$DETAILS" -eq 1 ]; then
    printf '[inventory] sanitized risk locations\n'
    {
        rg_locations sleep "$SLEEP_PATTERN"
        rg_locations timeout "$TIMEOUT_PATTERN"
        rg_locations loopback_or_listener "$LOOPBACK_PATTERN"
        rg_locations env_mutation "$ENV_MUTATION_PATTERN"
        rg_locations test_env_lock "$TEST_ENV_LOCK_PATTERN"
    } | sort | sed 's/^/  /'
else
    printf '[inventory] top risk files by clue type\n'
    {
        rg_file_counts sleep "$SLEEP_PATTERN"
        rg_file_counts timeout "$TIMEOUT_PATTERN"
        rg_file_counts loopback_or_listener "$LOOPBACK_PATTERN"
        rg_file_counts env_mutation "$ENV_MUTATION_PATTERN"
        rg_file_counts test_env_lock "$TEST_ENV_LOCK_PATTERN"
    }
    printf '\n[inventory] rerun with --details for sanitized file:line locations\n'
fi
