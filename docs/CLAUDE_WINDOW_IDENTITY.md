# Claude Code window identity

[简体中文](CLAUDE_WINDOW_IDENTITY.zh-CN.md)

The optional [`claude_window_headers.py`](../scripts/claude_window_headers.py)
helper supplies only `X-WebCodex-Client-Window` for Claude Code's HTTP MCP
`headersHelper`. It does not authenticate a request, select a Project or
Workflow Session, or change any Claude configuration. It requires a WebCodex
Server version that supports the explicit client-window header; an older Server
does not gain Window identity merely by installing the helper. See the
[Session model](agent/session-model.md).

## Configure the helper

Claude Code's [MCP documentation](https://code.claude.com/docs/en/mcp) defines
`headersHelper` as a shell command that prints a JSON object of HTTP headers.
Claude runs it when connecting or reconnecting, with a 10-second timeout.
Use an absolute interpreter and script path so shell working-directory changes
do not change the command. Python 3.9 or newer is required.

Add the helper to the intended server entry in your own Claude configuration,
using your existing server URL and authentication configuration. For example:

```json
{
  "mcpServers": {
    "webcodex": {
      "type": "http",
      "url": "https://YOUR-WEBCODEX-SERVER/mcp",
      "headersHelper": "/usr/bin/python3 /absolute/path/webcodex/scripts/claude_window_headers.py"
    }
  }
}
```

Quote absolute paths containing spaces according to the shell used by Claude.
The example adds only the custom window header. The helper never reads or
outputs an `Authorization` header. Claude's helper headers override static
headers with the same name; an `Authorization` value from a helper disables
Claude's OAuth fallback, so do not add one to this helper.

## Explicit identity on every platform

Set `WEBCODEX_CLIENT_WINDOW_ID` before starting the intended Claude process.
An explicit value must match ASCII `[A-Za-z0-9._:-]{1,256}` exactly. Spaces,
trimming, Unicode, empty values and longer values are rejected. If the variable
is present but invalid, the helper prints `{}` and does not try automatic
detection.

Generate a fresh, high-entropy value for each intended window. On a POSIX shell:

```sh
export WEBCODEX_CLIENT_WINDOW_ID="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
claude
```

On PowerShell:

```powershell
$env:WEBCODEX_CLIENT_WINDOW_ID = [guid]::NewGuid().ToString("N")
claude
```

Keep that value unchanged during reconnects. Generate another before starting
an independent window. Processes started from the same shell inherit the same
value until it is changed; a globally fixed value would merge their window
correlation. Treat the value as an opaque correlation label, never as a
credential, account name, working directory, or conversation title.

Windows and macOS use explicit identity only. Without it, the helper prints
`{}`; ordinary tools can still run without Window correlation.

## Automatic Linux process identity

With no explicit variable, the helper walks at most 32 ancestors using bounded
`/proc/<pid>/stat` reads and the candidate executable symlink. It accepts a
recognized native Claude process, including the native installer's versioned
binary layout. It deliberately rejects Node/Bun wrappers and ambiguous ancestry.
It reads the Linux boot ID and machine ID, then hashes a domain-separated tuple
of machine ID, boot ID, Claude PID and process start ticks. Raw machine IDs,
boot IDs and process facts are never emitted.

The helper rechecks the observed ancestry, start times, executable, boot ID and
machine ID before returning an identity. Missing, unreadable, oversized,
malformed or changing inputs yield `{}`. This is a bounded best-effort process
observation, not an atomic operating-system snapshot.

A reconnect from the same live Claude process keeps the same identity even
when the helper or shell process changes. A new Claude process, reused PID,
reboot or different machine changes it. **This identifies a Claude process,
not a conversation.** Multiple chats handled by one process share its identity;
a resumed conversation in a new process receives a new automatic identity.
Use explicit IDs when your desired boundary differs.

The helper never reads command lines, other process environments, Claude
private files, transcripts or authentication state. It emits one JSON object
and no stderr diagnostics on failure. If Python itself cannot start or Claude
cannot invoke the command, that failure happens outside the helper.

## Validation scope

Run the credential-free fixtures with:

```sh
python3 -m unittest scripts.tests.test_claude_window_headers -v
```

The tests simulate Linux process files and executable links, including
reconnects, process replacement, malformed data, permission failures and races.
The command-output fixture exercises the actual helper with an explicit test
ID. These fixtures do not establish real Claude/Linux integration; that needs
a separately observed integration run. No test reads Claude configuration or
credentials, and installing the helper does not deploy or restart WebCodex.
