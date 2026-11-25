# WebCodex live web preview — ChatGPT MCP Apps

WebCodex Workbench can start a temporary HTTPS preview for a dev server running on the selected Runner and show the page interactively in the ChatGPT MCP App.

## How it works

1. Start the application's local development server on the selected Runner (e.g. Vite on port 5173).
2. In WebCodex Workbench, choose the matching Project, open **Web preview**, enter the local port, and select **Start preview**.
3. The `start_web_preview` action creates a 30-minute gateway session and dispatches an authenticated Node.js agent as a supervisor-owned detached Runner Job.
4. The Runner agent connects outwards by WSS, relays HTTP and WebSocket/HMR, and exposes the site at `https://wcprev-<session-id>.zetx.tech/` inside the Workbench iframe. No inbound Runner port is needed.

## Gateway setup (one-time)

The existing Traefik configuration for `codex.zetx.tech` routes `/agent/` and `/admin/sessions` to the preview gateway. The preview gateway's router also accepts `wcprev-<32-hex-id>.zetx.tech`. The control router must use `PathPrefix` for `/admin/sessions` so session-status and expiry endpoints are reachable. Both names need working DNS, HTTPS and WebSocket upgrades. A TLS certificate for `*.zetx.tech` covers the temporary preview hosts.

From `integrations/web-preview`, create a private `.env.preview` with:

```env
PREVIEW_ADMIN_TOKEN=<strong-random-secret>
PREVIEW_DOMAIN=zetx.tech
PREVIEW_PUBLIC_SCHEME=https
PREVIEW_PORT=8093
```

Generate a secret with `openssl rand -hex 32` and keep the file outside version control. Start the gateway:

```sh
docker compose --env-file .env.preview -f docker-compose.preview.yml up -d --build
```

Configure the WebCodex Server with:

```env
WEBCODEX_PREVIEW_GATEWAY_URL=https://codex.zetx.tech
WEBCODEX_PREVIEW_ADMIN_TOKEN=<same-secret-as-PREVIEW_ADMIN_TOKEN>
WEBCODEX_PREVIEW_BASE_DOMAIN=zetx.tech
```

Deploy the Server version containing Workbench v11. Runner hosts need Node.js 22+ with outgoing WSS access to the gateway and the `detached_process_jobs` capability. The operator needs Project read, Job run and detached Job permission.

## Security

Every session has a random 32-character hex id and independent agent token. The Server holds the admin secret; the Runner only receives its own agent token through Job stdin. Agent authentication uses an unreflected WebSocket subprotocol. Browser iframes receive only expiring HTTPS URLs with isolated origins; Host CSP constrains the preview domain. HTTP request and response sizes, WebSocket payloads, session capacity and concurrent requests are bounded. Sessions expire after 30 minutes and can be explicitly closed.

## Tests

```sh
npm ci
npm test
```

Gateway tests cover admin authentication, deterministic session creation, session status/close, HTTP proxying and bidirectional WebSockets. Rust tests validate temporary URL origins, gateway URLs and idempotency namespacing.
