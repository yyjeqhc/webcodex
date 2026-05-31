#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/diagnose_tailscale_ssh.sh --target USER@HOST [options]

Purpose:
  Run read-only SSH/Tailscale connectivity sampling from this machine to a target
  host. It records latency, exit status, and stderr snippets so intermittent
  Tailscale/SSH failures around e2e/job runs can be correlated with connection
  patterns.

Required:
  --target USER@HOST              SSH target, for example root@100.x.y.z

Options:
  --iterations N                  Sequential samples before/after trigger (default: 20)
  --interval SECONDS              Sleep between sequential samples (default: 1)
  --concurrency N                 Parallel SSH probes per burst (default: 0, disabled)
  --bursts N                      Number of parallel bursts when concurrency > 0 (default: 3)
  --connect-timeout SECONDS       SSH ConnectTimeout (default: 5)
  --remote-command COMMAND        Read-only remote command (default: printf ok...)
  --out-dir DIR                   Output directory (default: .codex/diagnostics/tailscale-ssh-<timestamp>)
  --trigger COMMAND               Optional local command to run between pre/post samples
  --tailscale-target HOST         Optional host/IP for `tailscale ping` samples
  --ssh-option OPTION             Extra ssh option; may be repeated, e.g. --ssh-option StrictHostKeyChecking=no
  --no-controlmaster              Disable SSH ControlMaster for probes
  --help                          Show this help

Examples:
  scripts/diagnose_tailscale_ssh.sh --target root@100.92.192.51 --iterations 30

  scripts/diagnose_tailscale_ssh.sh \
    --target root@100.92.192.51 \
    --iterations 20 \
    --concurrency 8 \
    --bursts 5 \
    --trigger 'bash scripts/e2e_test.sh'

Outputs:
  samples.csv       Per-probe timing and result rows
  summary.txt       Aggregated success/failure counts and latency summary
  trigger.log       Optional trigger command output
  environment.txt   Local environment and tool versions

Safety:
  The default remote command is read-only. The script does not restart services,
  edit remote files, or change Tailscale/SSH configuration.
EOF
}

target=""
iterations=20
interval=1
concurrency=0
bursts=3
connect_timeout=5
remote_command='printf "ok host=%s now=%s\n" "$(hostname 2>/dev/null || uname -n)" "$(date +%s)"'
out_dir=""
trigger=""
tailscale_target=""
no_controlmaster=0
ssh_options=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:-}"; shift 2 ;;
    --iterations)
      iterations="${2:-}"; shift 2 ;;
    --interval)
      interval="${2:-}"; shift 2 ;;
    --concurrency)
      concurrency="${2:-}"; shift 2 ;;
    --bursts)
      bursts="${2:-}"; shift 2 ;;
    --connect-timeout)
      connect_timeout="${2:-}"; shift 2 ;;
    --remote-command)
      remote_command="${2:-}"; shift 2 ;;
    --out-dir)
      out_dir="${2:-}"; shift 2 ;;
    --trigger)
      trigger="${2:-}"; shift 2 ;;
    --tailscale-target)
      tailscale_target="${2:-}"; shift 2 ;;
    --ssh-option)
      ssh_options+=("-o" "${2:-}"); shift 2 ;;
    --no-controlmaster)
      no_controlmaster=1; shift ;;
    --help|-h)
      usage; exit 0 ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2 ;;
  esac
done

if [[ -z "$target" ]]; then
  echo "--target is required" >&2
  usage >&2
  exit 2
fi

case "$target" in
  *[[:space:]]*|*';'*|*'|'*|*'&'*|*'`'*|*'$('*|*'<'*|*'>'*)
    echo "target contains unsafe shell metacharacters" >&2
    exit 2 ;;
esac

for numeric in iterations concurrency bursts connect_timeout; do
  value="${!numeric}"
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    echo "$numeric must be a non-negative integer" >&2
    exit 2
  fi
done

if ! [[ "$interval" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "interval must be a number" >&2
  exit 2
fi

if [[ -z "$out_dir" ]]; then
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  safe_target="${target//@/_}"
  safe_target="${safe_target//[^A-Za-z0-9_.-]/_}"
  out_dir=".codex/diagnostics/tailscale-ssh-${safe_target}-${stamp}"
fi

mkdir -p "$out_dir"
samples_csv="$out_dir/samples.csv"
summary_txt="$out_dir/summary.txt"
env_txt="$out_dir/environment.txt"
trigger_log="$out_dir/trigger.log"

printf 'phase,seq,worker,start_ms,duration_ms,exit_code,stdout_head,stderr_head\n' > "$samples_csv"

now_ms() {
  python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

csv_escape() {
  local s="${1//$'\r'/ }"
  s="${s//$'\n'/ }"
  s="${s//\"/\"\"}"
  printf '"%s"' "$s"
}

ssh_base=(ssh
  -o BatchMode=yes
  -o ConnectTimeout="$connect_timeout"
  -o ServerAliveInterval=2
  -o ServerAliveCountMax=2
)

if [[ "$no_controlmaster" -eq 1 ]]; then
  ssh_base+=(
    -o ControlMaster=no
    -o ControlPath=none
  )
fi

if [[ "${#ssh_options[@]}" -gt 0 ]]; then
  ssh_base+=("${ssh_options[@]}")
fi

write_environment() {
  {
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "hostname=$(hostname 2>/dev/null || true)"
    echo "target=$target"
    echo "iterations=$iterations"
    echo "interval=$interval"
    echo "concurrency=$concurrency"
    echo "bursts=$bursts"
    echo "connect_timeout=$connect_timeout"
    echo "no_controlmaster=$no_controlmaster"
    echo "remote_command=$remote_command"
    echo "trigger=$trigger"
    echo "tailscale_target=$tailscale_target"
    echo
    echo "ssh_version=$({ ssh -V; } 2>&1 || true)"
    if command -v tailscale >/dev/null 2>&1; then
      echo "tailscale_version=$(tailscale version 2>&1 | head -n 1 || true)"
      echo
      echo "tailscale_status:"
      tailscale status 2>&1 || true
    else
      echo "tailscale_version=not-found"
    fi
  } > "$env_txt"
}

record_probe() {
  local phase="$1"
  local seq="$2"
  local worker="$3"
  local start end duration code stdout stderr stdout_head stderr_head stderr_file
  stderr_file="$(mktemp "${TMPDIR:-/tmp}/diagnose_tailscale_ssh_stderr.XXXXXX")"
  start="$(now_ms)"
  set +e
  stdout=$("${ssh_base[@]}" "$target" "$remote_command" 2>"$stderr_file")
  code=$?
  stderr="$(cat "$stderr_file" 2>/dev/null || true)"
  rm -f "$stderr_file"
  set -e
  end="$(now_ms)"
  duration=$((end - start))
  stdout_head="$(printf '%s' "$stdout" | head -c 300)"
  stderr_head="$(printf '%s' "$stderr" | head -c 300)"
  {
    printf '%s,%s,%s,%s,%s,%s,' "$phase" "$seq" "$worker" "$start" "$duration" "$code"
    csv_escape "$stdout_head"
    printf ','
    csv_escape "$stderr_head"
    printf '\n'
  } >> "$samples_csv"
}

record_tailscale_ping() {
  local phase="$1"
  local seq="$2"
  if [[ -z "$tailscale_target" ]] || ! command -v tailscale >/dev/null 2>&1; then
    return 0
  fi
  local start end duration code stdout stderr stdout_head stderr_head
  start="$(now_ms)"
  set +e
  stdout="$(tailscale ping --timeout "${connect_timeout}s" "$tailscale_target" 2>"$out_dir/.tailscale_ping.err")"
  code=$?
  stderr="$(cat "$out_dir/.tailscale_ping.err" 2>/dev/null || true)"
  rm -f "$out_dir/.tailscale_ping.err"
  set -e
  end="$(now_ms)"
  duration=$((end - start))
  stdout_head="$(printf '%s' "$stdout" | head -c 300)"
  stderr_head="$(printf '%s' "$stderr" | head -c 300)"
  {
    printf '%s,%s,%s,%s,%s,%s,' "${phase}_tailscale_ping" "$seq" 0 "$start" "$duration" "$code"
    csv_escape "$stdout_head"
    printf ','
    csv_escape "$stderr_head"
    printf '\n'
  } >> "$samples_csv"
}

run_sequential_phase() {
  local phase="$1"
  local n="$2"
  local i
  for ((i = 1; i <= n; i++)); do
    record_tailscale_ping "$phase" "$i"
    record_probe "$phase" "$i" 0
    sleep "$interval"
  done
}

run_parallel_phase() {
  local phase="$1"
  local burst_count="$2"
  local workers="$3"
  local burst worker
  if [[ "$workers" -le 0 ]]; then
    return 0
  fi
  for ((burst = 1; burst <= burst_count; burst++)); do
    for ((worker = 1; worker <= workers; worker++)); do
      record_probe "$phase" "$burst" "$worker" &
    done
    wait
    sleep "$interval"
  done
}

write_summary() {
  python3 - "$samples_csv" > "$summary_txt" <<'PY'
import csv
import statistics
import sys
from collections import Counter, defaultdict

path = sys.argv[1]
rows = list(csv.DictReader(open(path, newline='')))
print(f"samples={len(rows)}")
by_phase = defaultdict(list)
for row in rows:
    by_phase[row['phase']].append(row)
for phase, items in sorted(by_phase.items()):
    codes = Counter(row['exit_code'] for row in items)
    durations = [int(row['duration_ms']) for row in items if row['duration_ms'].isdigit()]
    ok = sum(1 for row in items if row['exit_code'] == '0')
    fail = len(items) - ok
    print(f"\n[{phase}]")
    print(f"total={len(items)} ok={ok} fail={fail} codes={dict(codes)}")
    if durations:
        print(f"latency_ms min={min(durations)} p50={statistics.median(durations):.1f} max={max(durations)}")
    errors = Counter(row['stderr_head'] for row in items if row['stderr_head'])
    for err, count in errors.most_common(5):
        print(f"stderr[{count}]={err[:240]}")
PY
}

write_environment

echo "diagnostic output: $out_dir"
echo "pre sequential samples..."
run_sequential_phase pre "$iterations"

echo "parallel burst samples..."
run_parallel_phase parallel "$bursts" "$concurrency"

if [[ -n "$trigger" ]]; then
  echo "running trigger command; output: $trigger_log"
  set +e
  bash -lc "$trigger" > "$trigger_log" 2>&1
  trigger_code=$?
  set -e
  echo "trigger_exit_code=$trigger_code" >> "$env_txt"
else
  echo "trigger_exit_code=not-run" >> "$env_txt"
fi

echo "post sequential samples..."
run_sequential_phase post "$iterations"

write_summary
cat "$summary_txt"
echo
echo "Wrote:"
echo "  $samples_csv"
echo "  $summary_txt"
echo "  $env_txt"
if [[ -n "$trigger" ]]; then
  echo "  $trigger_log"
fi
