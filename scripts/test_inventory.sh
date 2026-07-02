#!/usr/bin/env bash
set -euo pipefail

# Heuristic, read-only test inventory for WebCodex.
#
# Scope:
#   - scans only src, docs, and tests when those directories exist
#   - does not access the network
#   - does not modify the repository
#   - avoids printing matched source lines so token-looking fixture values are
#     not echoed by this script

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

DETAILS=0
if [ "$#" -gt 0 ]; then
    case "$1" in
        --details)
            DETAILS=1
            ;;
        -h|--help)
            printf 'usage: bash scripts/test_inventory.sh [--details]\n'
            exit 0
            ;;
        *)
            printf '[inventory] unknown argument: %s\n' "$1" >&2
            printf 'usage: bash scripts/test_inventory.sh [--details]\n' >&2
            exit 2
            ;;
    esac
fi

ROOTS=()
for dir in src docs tests; do
    if [ -d "$dir" ]; then
        ROOTS+=("$dir")
    fi
done

if [ "${#ROOTS[@]}" -eq 0 ]; then
    printf '[inventory] no scan roots found\n' >&2
    exit 1
fi

rg_count() {
    local pattern="$1"
    local output status
    set +e
    output="$(rg --count-matches "$pattern" "${ROOTS[@]}" 2>/dev/null)"
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

rg_locations() {
    local label="$1"
    local pattern="$2"
    local status
    set +e
    rg --line-number --no-heading "$pattern" "${ROOTS[@]}" 2>/dev/null \
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
    rg --line-number --no-heading "$pattern" "${ROOTS[@]}" 2>/dev/null \
        | awk -F: -v label="$label" '{ count[$1]++ } END { for (file in count) print count[file] "\t" file "\t" label }' \
        | sort -nr \
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

rust_files=()
while IFS= read -r file; do
    rust_files+=("$file")
done < <(find "${ROOTS[@]}" -type f -name '*.rs' 2>/dev/null | sort)

print_ignored_tests() {
    if [ "${#rust_files[@]}" -eq 0 ]; then
        return 0
    fi
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

printf '[inventory] roots:'
printf ' %s' "${ROOTS[@]}"
printf '\n\n'

printf '[inventory] test attributes\n'
printf '  rust files: %s\n' "${#rust_files[@]}"
printf '  #[test]: %s\n' "$(rg_count '^[[:space:]]*#\[test')"
printf '  #[tokio::test]: %s\n' "$(rg_count '^[[:space:]]*#\[tokio::test')"
printf '  #[ignore]: %s\n' "$(rg_count '^[[:space:]]*#\[ignore')"
printf '\n'

printf '[inventory] risk clue counts\n'
printf '  sleep calls: %s\n' "$(rg_count 'sleep[[:space:]]*\(')"
printf '  timeout calls: %s\n' "$(rg_count 'timeout[[:space:]]*\(')"
printf '  loopback strings or TcpListener: %s\n' "$(rg_count 'localhost|127\.0\.0\.1|TcpListener')"
printf '  env set/remove calls: %s\n' "$(rg_count '(std::)?env::(set_var|remove_var)')"
printf '  TEST_ENV_LOCK mentions: %s\n' "$(rg_count 'TEST_ENV_LOCK')"
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
        rg_locations sleep 'sleep[[:space:]]*\('
        rg_locations timeout 'timeout[[:space:]]*\('
        rg_locations loopback_or_listener 'localhost|127\.0\.0\.1|TcpListener'
        rg_locations env_mutation '(std::)?env::(set_var|remove_var)'
        rg_locations test_env_lock 'TEST_ENV_LOCK'
    } | sort | sed 's/^/  /'
else
    printf '[inventory] top risk files by clue type\n'
    {
        rg_file_counts sleep 'sleep[[:space:]]*\('
        rg_file_counts timeout 'timeout[[:space:]]*\('
        rg_file_counts loopback_or_listener 'localhost|127\.0\.0\.1|TcpListener'
        rg_file_counts env_mutation '(std::)?env::(set_var|remove_var)'
        rg_file_counts test_env_lock 'TEST_ENV_LOCK'
    }
    printf '\n[inventory] rerun with --details for sanitized file:line locations\n'
fi
