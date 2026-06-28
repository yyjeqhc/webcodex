# sg4 manual smoke test: account credential, PAT, agent token, and project call

[English](smoke-test-sg4.md) | [简体中文](smoke-test-sg4.zh-CN.md)

This document records the manually verified onboarding loop for `https://sg4.yyjeqhc.cn`. Replace the domain, username, `client_id`, project id, and paths for another deployment.

Verified example values:

```text
username = alice
client_id = ubuntu-client
agent project id = webcodex
runtime project id = agent:ubuntu-client:webcodex
project path = /root/git/private-drop
```

## Credential model

- `WEBCODEX_TOKEN`: server bootstrap/root/admin credential. Use it for first user creation and emergency management. Do not put it in GPT Actions, MCP, or `webcodex-agent` for daily use.
- `wc_acct_xxx`: account credential. The admin issues it once while creating a user. The user uses it locally to create a `wc_pat_xxx` and `wc_agent_xxx`. Do not put it in GPT Actions/MCP and do not give it to `webcodex-agent`.
- `wc_pat_xxx`: personal API token. It is generated locally; the server stores only the hash. Use it for GPT Actions, MCP, `/api/tools/list`, and `/api/tools/call`.
- `wc_agent_xxx`: agent token. It is generated locally; the server stores only the hash and binds it to `allowed_client_id`. Use it only for `webcodex-agent`; it cannot call runtime/project/tool/MCP/account endpoints.
- `client_id`: agent client instance id such as `ubuntu-client` or `alice-macbook`. Runtime project ids are `agent:<client_id>:<project_id>`.

## 1. Server env

```bash
cd /opt/webcodex
chmod +x ./webcodex ./webcodex-cli

BOOTSTRAP_TOKEN="wc_bootstrap_$(openssl rand -hex 32)"
mkdir -p /opt/webcodex/data

cat > /opt/webcodex/webcodex.env <<EOF_ENV
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/opt/webcodex/data
WEBCODEX_TOKEN=$BOOTSTRAP_TOKEN
EOF_ENV

chmod 600 /opt/webcodex/webcodex.env
WEBCODEX_ENV_FILE=/opt/webcodex/webcodex.env ./webcodex
```

Keep `WEBCODEX_TOKEN` on the server. It is not the GPT Action/MCP token and it is not the agent token.

## 2. Create user and issue account credential

```bash
./webcodex-cli users create \
  --server-url https://sg4.yyjeqhc.cn \
  --token "$WEBCODEX_TOKEN" \
  --username alice \
  --display-name "Alice" \
  --role user \
  --issue-credential
```

Save the returned `wc_acct_xxx` as the user's account credential. In the examples below it is passed through `WEBCODEX_ACCOUNT_CREDENTIAL`.

## 3. User creates a PAT locally

```bash
./webcodex-cli token create-local \
  --server https://sg4.yyjeqhc.cn \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

The output includes a `wc_pat_xxx`. Store it as `WEBCODEX_PAT` on the user's client and use it for GPT Actions/MCP/runtime API calls. The server stores only the token hash.

## 4. User creates an agent token locally

```bash
./webcodex-cli agent-token create-local \
  --server https://sg4.yyjeqhc.cn \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --client-id ubuntu-client \
  --name ubuntu-client
```

The output includes a `wc_agent_xxx`. Store it as `WEBCODEX_AGENT_TOKEN` on the agent host. The server stores only the token hash and binds it to `ubuntu-client`.

## 5. Agent init

```bash
./webcodex-agent init \
  --server-url https://sg4.yyjeqhc.cn \
  --token "$WEBCODEX_AGENT_TOKEN" \
  --client-id ubuntu-client \
  --owner alice \
  --display-name "Ubuntu Client" \
  --transport websocket \
  --projects-dir /root/.config/webcodex/projects.d \
  --allowed-root /root \
  --output /opt/webcodex/agent.toml \
  --overwrite
```

## 6. Agent project config

This is the agent `projects.d/*.toml` format, not the server `projects.toml` format.

Correct `/root/.config/webcodex/projects.d/webcodex.toml`:

```toml
id = "webcodex"
path = "/root/git/private-drop"
name = "WebCodex"
kind = "repo"
description = "WebCodex repository"
allow_patch = true

[hooks]
status = ["git status --short"]
fmt = ["cargo fmt"]
check = ["cargo check --all-targets"]
test = ["cargo test"]
```

Wrong agent project file:

```toml
[projects.webcodex]
path = "/root/git/private-drop"
```

That nested table is for server-side `projects.toml`. In agent `projects.d/*.toml` it causes `missing field id`; use top-level `id` and `path`.

## 7. Start agent

```bash
./webcodex-agent --config /opt/webcodex/agent.toml
```

## 8. runtime_status check

```bash
curl -sS https://sg4.yyjeqhc.cn/api/runtime/status \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  -d '{}' | jq '.output.agents.clients[] | {client_id, owner, connected, projects_count}'
```

Expected: an online `ubuntu-client` owned by `alice` with at least one project.

## 9. projects/list check

```bash
curl -sS https://sg4.yyjeqhc.cn/api/projects/list \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  -d '{}' | jq .
```

Expected: `agent:ubuntu-client:webcodex` appears in the output.

## 10. tools/call git_status check

```bash
curl -sS https://sg4.yyjeqhc.cn/api/tools/call \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  -d '{
    "tool": "git_status",
    "project": "agent:ubuntu-client:webcodex"
  }' | jq .
```

Expected: the server routes the request to the connected agent and returns git status for `/root/git/private-drop`.

## Common errors

### old binary: `unknown admin command user create`

The deployed `webcodex-cli` is older than the docs. Replace both server and CLI binaries, then rerun `webcodex-cli --help` and confirm it lists `user create`, `token create-local`, `token register-hash`, `agent-token create-local`, and `agent-token register-hash`.

### wrong `projects.d` format: `missing field id`

The agent project file probably uses server-side `[projects.<id>]` syntax. Rewrite it with top-level fields:

```toml
id = "webcodex"
path = "/root/git/private-drop"
```

Newer agents print a warning that this looks like a server `projects.toml` entry and that agent `projects.d` files require top-level `id` and `path`.

### websocket nginx proxy issue

If the server is reachable but the agent does not connect, verify the reverse proxy passes WebSocket upgrade headers for `/api/agents/ws`, uses HTTP/1.1 upstream, and does not strip `Authorization` or query-token compatibility data during the handshake.
