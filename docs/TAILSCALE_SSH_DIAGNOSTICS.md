# Tailscale / SSH diagnostics

Use `scripts/diagnose_tailscale_ssh.sh` from the sg4/project host to collect read-only SSH connectivity samples against an OE/Tailscale target. The script is intended to answer questions like:

- Does plain SSH fail before, during, or after an e2e/job run?
- Are failures caused by sequential probes, parallel bursts, or the trigger workload?
- Are failures real network/Tailscale timeouts, SSH host-key issues, authentication issues, or command errors?

The script does not restart services, edit remote files, or change Tailscale/SSH configuration. With `--remote-health`, it also captures read-only remote `systemctl`, `tailscale status/netcheck`, `journalctl`, and port-22 socket snapshots so a real `tailscaled` service outage can be correlated with the probe timeline.

## Quick smoke test

```bash
scripts/diagnose_tailscale_ssh.sh \
  --target root@100.92.192.51 \
  --iterations 3 \
  --interval 0.2 \
  --concurrency 2 \
  --bursts 2 \
  --connect-timeout 5 \
  --ssh-option StrictHostKeyChecking=no \
  --ssh-option UserKnownHostsFile=/dev/null \
  --ssh-option LogLevel=ERROR \
  --out-dir /tmp/private-drop-v4-tailscale-diagnose-smoke
```

Expected outputs:

- `samples.csv`: one row per SSH/Tailscale probe
- `summary.txt`: success/failure count, exit codes, latency summary, common stderr snippets
- `environment.txt`: local environment and tool versions

## Around an e2e trigger

```bash
scripts/diagnose_tailscale_ssh.sh \
  --target root@100.92.192.51 \
  --iterations 20 \
  --interval 0.5 \
  --concurrency 4 \
  --bursts 5 \
  --connect-timeout 5 \
  --ssh-option StrictHostKeyChecking=no \
  --ssh-option UserKnownHostsFile=/dev/null \
  --ssh-option LogLevel=ERROR \
  --remote-health \
  --remote-health-since '20 minutes ago' \
  --trigger 'bash scripts/e2e_test.sh' \
  --out-dir /tmp/private-drop-v4-tailscale-diagnose-e2e
```

This records:

1. `pre` sequential probes
2. `parallel` burst probes
3. optional `trigger` command output in `trigger.log`
4. `remote_health_after_trigger.log`, when enabled
5. `post` sequential probes
6. `remote_health_after_post.log`, when enabled

If Tailscale drops during or after the trigger, the phase-level summary helps identify whether the issue is tied to connection bursts, the long-running trigger, or recovery after the trigger exits.

## Interpreting common failures

| stderr / exit code | Likely meaning | Suggested next check |
|---|---|---|
| `Host key verification failed`, exit `255` | Known-hosts mismatch or no host key entry for the raw target | Use explicit `--ssh-option StrictHostKeyChecking=no --ssh-option UserKnownHostsFile=/dev/null` for diagnostics, or pre-seed known_hosts |
| `Connection timed out`, exit `255` | Tailscale path, tailscaled, firewall, or target sshd unreachable | Check `tailscale status`, `tailscale ping`, `journalctl -u tailscaled`, and target `sshd` logs |
| `Connection reset`, exit `255` | Target sshd or network path closed connection mid-handshake | Compare sequential vs parallel phases; check sshd rate limits/logs |
| High latency but exit `0` | Path is reachable but slow or serialized | Compare `--concurrency 0` vs burst runs; check ControlMaster behavior |
| Trigger fails but probes remain healthy | Workload/test failure, not Tailscale failure | Inspect `trigger.log` |

## Notes

- Keep the default remote command read-only unless you intentionally need a custom probe.
- Use `/tmp/...` for `--out-dir` when you do not want diagnostic outputs in the repository.
- Add `--tailscale-target 100.x.y.z` if the sg4 host has the `tailscale` CLI and you want `tailscale ping` samples in the CSV.
- Add `--remote-health` when you suspect the target `tailscaled` or `sshd` service actually stopped or was restarted during the trigger window.
- For ControlMaster isolation, compare a run with and without `--no-controlmaster`.
