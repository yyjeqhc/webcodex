# Model API ACP adapter

`adapter.py` is a Python 3.10+ standard-library ACP v1 stdio provider for bounded
text reasoning and review through a configured model API. It supports OpenAI
Responses and OpenAI-compatible chat completions, including SSE and JSON
responses. It never executes model tools, reads project files, fetches resource
links, or changes code. The Runner supplies the complete text context in
`session/prompt`; resource links become textual references. This adapter does
not discover, reconstruct, or persist Workflow Sessions or conversation history.

## Runner configuration

Configure one logical provider per fixed API/model/credential audience. Multiple
entries allow the caller to select a backend with `coding_agent_start`'s existing
`provider_id`; use `coding_agent_observe` and `coding_agent_cancel` for its existing
run lifecycle. Runner authorization, project identity, concurrency, durable
dispatch certainty, and cancellation remain authoritative. Changing models or
API endpoints requires operator configuration and Runner restart.

The MCP client needs the existing `coding_agent:run` and `project:write` grants
to initiate work. Configuring a provider does not grant those scopes or widen
existing OAuth clients; use the normal operator provisioning/consent path.

Use `coding_agent_start.context_session_id` to explicitly supply an authorized,
Project-matching bounded Session handoff. It requires `runtime:read` in addition
to Run authority. Context is supplied as quoted reports in the prompt; the
adapter never selects or resumes that Session. Current files/Git are not fetched,
and changed context under the same initiation key conflicts. See the
[continuity guide](../../docs/SESSION_CONTINUITY.md) for saving context, discovery,
recovery, and observing the exact Run after uncertain dispatch.

```toml
[acp]
max_concurrent_runs = 2

[[acp.agents]]
id = "model-review"
name = "Configured review model"
executable = "/usr/bin/python3"
args = ["/runner/owned/webcodex/integrations/model_gateway/adapter.py",
        "--api", "responses", "--base-url", "https://api.openai.com/v1",
        "--model", "YOUR_RESPONSES_MODEL", "--api-key-env", "MODEL_API_KEY"]

[acp.agents.env_from_env]
MODEL_API_KEY = "WEBCODEX_REVIEW_API_KEY"

[[acp.agents]]
id = "model-alternate"
name = "Configured compatible model"
executable = "/usr/bin/python3"
args = ["/runner/owned/webcodex/integrations/model_gateway/adapter.py",
        "--api", "chat-completions", "--base-url", "https://YOUR_API_HOST/v1",
        "--model", "YOUR_COMPATIBLE_MODEL", "--api-key-env", "MODEL_API_KEY"]

[acp.agents.env_from_env]
MODEL_API_KEY = "WEBCODEX_ALTERNATE_API_KEY"
```

The Runner clears child environment and injects these explicit mappings. Keep
tokens in the operator's environment, never argv, URLs, repository files, or
prompts. The configured variable is sent as a Bearer credential. An official
OAuth access token can be configured only when its issuer, audience, entitlement,
and endpoint support that API; a ChatGPT/Codex account does not by itself establish
standard API access. This adapter does not inspect browser cookies, perform login,
refresh tokens, or modify ChatGPT/Codex settings. Credentials are not persisted.
Responses requests use `stream: true`, `store: false`, and an `input` array. Chat
requests use a single user message and `stream: true`. Neither supplies tools.

`--base-url` is an API root; the adapter appends `/responses` or
`/chat/completions`. HTTPS is required except for loopback HTTP used by local
services/tests. URLs with embedded credentials, queries, or fragments are rejected.
Redirects and environment proxy discovery are unsupported. Requests are never
automatically retried, including after cancellation or an ambiguous transport
failure. Sending the prompt to the configured endpoint is an intentional external
data transfer; choose that endpoint and context according to the operator's policy.

## Protocol and bounds

Stdout contains only newline-delimited JSON-RPC 2.0. ACP initialization negotiates
version 1, advertises no filesystem/terminal/tool capabilities, and supports
`initialize`, `session/new`, `session/prompt`, `session/update`, and `session/cancel`.
One adapter process accepts exactly one session and one admitted prompt; create a
fresh run for subsequent work. This avoids implicit unbounded conversation state.
`session/new` requires an empty `mcpServers` array. Embedded images, resources, and
audio are rejected. Baseline resource links are passed as references without
retrieval. The project `cwd` is accepted as ACP metadata and never accessed.

Defaults: 120 seconds absolute request deadline, 256 KiB UTF-8 prompt, 2 MiB HTTP
response body, and 256 KiB UTF-8 model output. Configure `--timeout`,
`--max-prompt-bytes`, `--max-response-bytes`, and `--max-output-bytes` as needed;
hard ceilings are 600 seconds and 16 MiB per byte bound. Input frames are bounded
to prompt limit + 64 KiB, including JSON overhead. Individual output chunks contain
at most 2,048 characters. Only one HTTP request can be active per process.

Model text becomes `agent_message_chunk` updates. A correlated `end_turn` is sent
only after nonempty text and confirmed normal API completion. Partial output alone
is not success. Truncated, empty, failed, tool-call, timeout, or over-limit responses
return a correlated JSON-RPC error (`code: -32000`, safe `data.kind`), which the
Runner records as failure. HTTP errors expose only `http_<status>`; response bodies,
exception details, tokens and headers are never reported. Invalid ACP requests use
standard JSON-RPC error codes and safe fixed messages.

Cancellation promptly answers the active prompt with `stopReason: cancelled`,
blocks further output and attempts to interrupt the connection. A network request
already accepted by a provider may still consume resources; cancellation does not
claim it was never dispatched. The Runner owns process cleanup. No automatic retry
or follow-up task is created. Model output is untrusted text, not execution authority.

## Validation

```sh
python3 -m unittest discover -s integrations/model_gateway/tests -v
```

Tests launch the actual stdio adapter and a local mock HTTP server. They cover both
API payloads, JSON/SSE text including split UTF-8, failures without credential/body
leakage, cancellation before response headers, deadline, input/response/output
limits, invalid requests, single-use sessions, incomplete/empty results, and rejected
model tools. They do not call a paid model or use real credentials.

Protocol references: [official ACP v1 schema](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/schema/v1/schema.json),
[ACP prompt lifecycle](https://agentclientprotocol.com/protocol/prompt-turn),
[OpenAI Responses streaming reference](https://platform.openai.com/docs/api-reference/responses-streaming).
