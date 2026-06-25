# Tailscale / SSH Diagnostics (Deprecated)

> **This document is deprecated.** It documents
> `scripts/diagnose_tailscale_ssh.sh` in the context of the **removed SSH
> executor**. The current runtime does not have an SSH executor; remote
> execution is handled by the polling `private-drop-agent` (protocol
> `polling-v1`).

## Current remote execution

Remote execution goes through the polling agent protocol, not SSH. See
[AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) for the current register / poll / result
/ job_update protocol and capability model.

## About the diagnostic script

`scripts/diagnose_tailscale_ssh.sh` is a generic, read-only SSH/Tailscale
connectivity sampling tool. It is unrelated to the runtime's execution path and
is kept only as a standalone network diagnostic utility. It does not restart
services, edit remote files, or change Tailscale/SSH configuration. Run it
directly with `--help` for current usage; do not treat it as part of the
runtime's agent transport.
