# Shell Profiles (Prepared Environment Snapshots)

WebCodex does **not** keep a persistent shell session. It prepares an
environment snapshot **once per project/profile** and then runs each command as
an independent process with that snapshot. This page describes how shell
profiles work, how to configure them, and the safety boundaries.

> Applies to `webcodex-agent` (the host agent that executes shell commands).
> The server never reads or stores shell env values, init_script bodies, or
> tokens.

---

## 1. What is a prepared shell env snapshot

When a command runs for a project, the agent:

1. Resolves the effective profile (`project.shell_profile`, else
   `shell.default_profile`, else plain shell config).
2. Prepares a one-time environment snapshot for that `project/cwd + profile`
   pair: it starts the profile program with `env_clear`, applies the profile
   `env`, runs the profile `init_script` (if any), and captures the resulting
   environment via `env -0` behind a unique marker.
3. Caches the snapshot keyed by `project/cwd + profile name`.
4. Runs every subsequent command for that pair as a fresh process with the
   cached snapshot applied — there is no long-lived shell, no `source` per
   command, and no `.bashrc`/`.profile` loaded by default.

Because the snapshot is keyed by project path + profile name, the same profile
can be reused by multiple projects, and a project-relative `init_script` (e.g.
`source .venv/bin/activate`) is resolved from each project's own root.

## 2. Why `.bashrc` / `.profile` are not sourced by default

WebCodex intentionally does **not** source `~/.bashrc` or `~/.profile` when
preparing a snapshot:

- `.bashrc` can be slow (network calls, prompts, completions).
- It may contain interactive-only commands (`stty`, `echo`, prompts) that
  break non-interactive capture or hang the prepare step.
- It can leak or pollute the environment (secrets, aliases, functions).
- It is non-reproducible across hosts and users.

Use an **explicit** shell profile instead. Explicit profiles are auditable,
fast, and do not depend on the interactive shell setup of the agent's user.

## 3. Rust / Cargo example

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
program = "sh"
args = ["-c"]

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"
```

This profile needs no `init_script`: the env block already sets `PATH`,
`CARGO_HOME`, and `RUSTUP_HOME`, so `cargo` / `rustc` are directly available.

## 4. Python venv example

```toml
[shell.profiles.py-venv]
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
```

The `init_script` is **project-relative**: `.venv/bin/activate` is resolved
from the project root, so each project activates its own venv. The snapshot is
captured once per project/profile pair.

## 5. Conda example

```toml
[shell.profiles.conda-ml]
program = "bash"
args = ["-lc"]
init_script = '''
source /opt/miniconda3/etc/profile.d/conda.sh
conda activate ml
'''
```

## 6. Binding a project to a profile

A project TOML (`projects.d/<id>.toml`) can pin a profile:

```toml
id = "paper-exp"
path = "/root/git/paper-exp"
shell_profile = "conda-ml"
```

## 7. Resolution rules

The effective profile for a command is chosen as:

1. `project.shell_profile` (if set), **else**
2. `shell.default_profile` (if set), **else**
3. fallback to the plain shell config (no prepared snapshot).

Notes:

- The snapshot cache key is `project/cwd + profile name`.
- The same profile can be reused by multiple projects.
- `.venv` (and any project-relative `init_script`) is prepared in the
  **project root**.
- `listProjects` exposes `shell_profile` (the project's setting),
  `resolved_shell_profile` (the actually-used name), and
  `shell_profile_status` (`configured` / `missing` / `not_configured` /
  `unknown`).

## 8. Changing config requires restarting the agent

There is **no reload API** in this phase. Changing a shell profile config
requires restarting the agent so the in-memory snapshot cache is rebuilt:

> Changing shell profile config requires restarting the agent so the
> in-memory snapshot cache is rebuilt.

After editing `agent.toml` or a project TOML, restart the `webcodex-agent`
service. Existing snapshots are dropped on restart and re-prepared lazily on
the next command.

## 9. Security notes

- **Never** put tokens in `init_script`.
- **Never** `echo`/`printf` secrets in `init_script` — anything the script
  writes to stdout is parsed as part of the env snapshot capture.
- Status / `runtime_status` / `listAgents` / `listProjects` expose **only**
  sanitized metadata: profile name, `has_init_script` (boolean),
  `env_keys_count` (count), `program`, and `args_count`. They never expose
  `init_script` bodies, env values, tokens, the Authorization header, the full
  `agent.toml`, or the full env snapshot.
- A prepare failure may surface a **safe** error summary (e.g. "failed to
  prepare shell profile 'x' at <path>"), but never the `init_script` body or
  stderr tail.
- The WebCodex agent token (`WEBCODEX_TOKEN` / `WEBCODEX_AGENT_TOKEN` /
  `WEBCODEX_USER_TOKEN` / `AUTHORIZATION`) is stripped from the child process
  environment and is never passed to commands.
- `prepare` runs with `env_clear` + an explicit allowlist of inherited keys;
  profiles must declare the env they need.

## 10. Troubleshooting

Run the local agent-config doctor to validate shell profiles and project
binding without contacting the server:

```bash
webcodex-cli doctor --agent-config /etc/webcodex/agent.toml
```

Add `--strict` to exit non-zero on any failure, and `--project <id>` to also
run a remote shell roundtrip (`printf webcodex-doctor-ok`) against a specific
project:

```bash
webcodex-cli doctor --agent-config /etc/webcodex/agent.toml --strict
webcodex-cli doctor --server-url https://example.test \
  --user-token-file ~/.config/webcodex/user.token \
  --agent-config /etc/webcodex/agent.toml \
  --project agent:oe:webcodex --strict
```

The doctor checks, locally:

- `agent.toml` parses.
- `shell.default_profile` references an existing `shell.profiles` entry.
- `projects_dir` exists and each project TOML parses.
- each project `path` exists.
- each project `shell_profile` (or the resolved default) is present in
  `shell.profiles`.

Remotely (when `--server-url`, a token, and `--project` are given), it runs a
minimal `printf webcodex-doctor-ok` roundtrip through `run_shell` and verifies
the marker.

See also:

- [AGENT_PROJECTS.md](AGENT_PROJECTS.md) — agent project registry.
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) — operational checklist.
- [BUILD_INSTALL.md](BUILD_INSTALL.md) — install and status reference.
