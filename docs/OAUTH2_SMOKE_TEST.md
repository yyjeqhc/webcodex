# OAuth2 smoke test

[English](OAUTH2_SMOKE_TEST.md)

This is a manual end-to-end smoke test for the OAuth2 onboarding closed loop:
enable OAuth2, create a client via the first-party API, drive the browser
authorize login + consent flow, exchange the code for a `wc_oat_*` access
token, call a protected runtime endpoint, and revoke the client.

It assumes a running WebCodex server reachable at `http://127.0.0.1:8080`
(adjust the host/port as needed). For HTTPS production deployments, set
`WEBCODEX_OAUTH2_ISSUER` / `WEBCODEX_PUBLIC_URL` to the public HTTPS domain.

## 0. Prerequisites

- A WebCodex server with a configured `WEBCODEX_TOKEN` (bootstrap admin token).
- `curl` and a browser for the authorize step.
- jq (optional, for pretty-printing JSON).

```bash
export BASE=http://127.0.0.1:8080
export BOOTSTRAP=<your WEBCODEX_TOKEN>
```

## 1. Enable OAuth2

OAuth2 is disabled by default. Restart the server with:

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=http://127.0.0.1:8080
WEBCODEX_PUBLIC_URL=http://127.0.0.1:8080
```

For local HTTP testing the loopback issuer is fine; the session cookie will
not be marked `Secure`. In production use `https://your-domain.example`.

Verify discovery is published (public, no auth):

```bash
curl -fsS $BASE/.well-known/oauth-authorization-server | jq .
```

The metadata must advertise `/oauth/authorize`, `/oauth/token`,
`/oauth/revoke`, `response_types_supported: ["code"]`,
`code_challenge_methods_supported: ["S256"]`, and
`scopes_supported` containing `runtime:read project:read project:write job:run account:manage`.

## 2. Create a user + PAT (if you do not already have one)

The authorize login flow accepts a PAT or the bootstrap token. Create a user
and a PAT via the first-party API:

```bash
# Create a user (bootstrap admin).
curl -fsS -X POST $BASE/api/users/create \
  -H "Authorization: Bearer $BOOTSTRAP" \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","role":"user"}'

# Issue a PAT for alice.
curl -fsS -X POST $BASE/api/tokens/create \
  -H "Authorization: Bearer $BOOTSTRAP" \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","name":"oauth-owner","scopes":["runtime:read","project:read","project:write","job:run","account:manage"]}'
```

Save the returned `token` (a `wc_pat_*` value):

```bash
export PAT=<wc_pat_...>
```

## 3. Create an OAuth client

```bash
curl -fsS -X POST $BASE/api/oauth/clients/create \
  -H "Authorization: Bearer $PAT" \
  -H "Content-Type: application/json" \
  -d '{
    "name":"Smoke Client",
    "redirect_uris":["http://127.0.0.1:3918/callback"],
    "allowed_scopes":["runtime:read","project:read"]
  }' | jq .
```

Save `client_id` and `client_secret` from the response. The secret is returned
**only once**; only its SHA-256 hash is stored.

```bash
export CLIENT_ID=<wc_client_...>
export CLIENT_SECRET=<wc_csec_...>
```

Verify `list` never returns the secret/hash:

```bash
curl -fsS -X POST $BASE/api/oauth/clients/list \
  -H "Authorization: Bearer $PAT" \
  -H "Content-Type: application/json" -d '{}' | jq .
```

## 4. Browser authorize flow (login + consent)

You need a PKCE S256 code verifier/challenge. Generate them:

```bash
export VERIFIER=$(openssl rand -base64 32 | tr -d '=+/' | cut -c1-43)
export CHALLENGE=$(printf '%s' "$VERIFIER" | openssl dgst -sha256 -binary | base64 | tr -d '=+/' | cut -c1-43)
export STATE=smoke-state-123
```

Open this URL in a browser (no Bearer header, no cookie):

```text
$BASE/oauth/authorize?response_type=code&client_id=$CLIENT_ID&redirect_uri=http://127.0.0.1:3918/callback&scope=runtime:read&state=$STATE&code_challenge=$CHALLENGE&code_challenge_method=S256
```

You should see the minimal **login page**. Enter `$PAT` and submit. On success
you are redirected back to `/oauth/authorize?...` with a
`webcodex_authorize_session` `HttpOnly; SameSite=Lax` cookie and shown the
**consent page** listing the client name, redirect URI, and requested scopes.

Click **Allow**. The browser is redirected to
`http://127.0.0.1:3918/callback?code=wc_oac_...&state=smoke-state-123`.
(There is no server listening on `:3918`, so the browser will show a
connection error — that is expected. Copy the `code=` query parameter from the
address bar.)

```bash
export CODE=<wc_oac_...>
```

Clicking **Deny** instead would redirect with `?error=access_denied&state=...`.

The Bearer direct-issuance path still works for non-browser clients:

```bash
curl -fsS -G "$BASE/oauth/authorize" \
  -H "Authorization: Bearer $PAT" \
  --data-urlencode "response_type=code" \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "redirect_uri=http://127.0.0.1:3918/callback" \
  --data-urlencode "scope=runtime:read" \
  --data-urlencode "state=$STATE" \
  --data-urlencode "code_challenge=$CHALLENGE" \
  --data-urlencode "code_challenge_method=S256" \
  -o /dev/null -w '%{redirect_url}\n'
```

## 5. Exchange the code for tokens

```bash
curl -fsS -X POST $BASE/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "grant_type=authorization_code" \
  --data-urlencode "code=$CODE" \
  --data-urlencode "redirect_uri=http://127.0.0.1:3918/callback" \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "client_secret=$CLIENT_SECRET" \
  --data-urlencode "code_verifier=$VERIFIER" | jq .
```

Save the `access_token` (a `wc_oat_*` value) and `refresh_token`:

```bash
export OAT=<wc_oat_...>
export ORT=<wc_ort_...>
```

## 6. Use the OAuth2 access token

The token is subject to delegated scope enforcement. `runtime:read` is enough
for status:

```bash
curl -fsS -X POST $BASE/api/runtime/status \
  -H "Authorization: Bearer $OAT" \
  -H "Content-Type: application/json" -d '{}' | jq .
```

It must NOT be able to call first-party-only routes (e.g. client management):

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST $BASE/api/oauth/clients/list \
  -H "Authorization: Bearer $OAT" \
  -H "Content-Type: application/json" -d '{}'
# expected: 403
```

## 7. Refresh token rotation

```bash
curl -fsS -X POST $BASE/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "grant_type=refresh_token" \
  --data-urlencode "refresh_token=$ORT" \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "client_secret=$CLIENT_SECRET" | jq .
```

A new `wc_oat_*` + `wc_ort_*` pair is returned; the old refresh token is
revoked.

## 8. Revoke the client

```bash
curl -fsS -X POST $BASE/api/oauth/clients/revoke \
  -H "Authorization: Bearer $PAT" \
  -H "Content-Type: application/json" \
  -d "{\"client_id\":\"$CLIENT_ID\"}" | jq .
```

After this, every `wc_oat_*` / `wc_ort_*` / `wc_oac_*` belonging to the
client is revoked. Confirm the access token no longer works:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST $BASE/api/runtime/status \
  -H "Authorization: Bearer $OAT" \
  -H "Content-Type: application/json" -d '{}'
# expected: 401
```

Revoking again is idempotent and still returns `{"success":true}`.

## What is intentionally not tested here

- Dynamic client registration (not implemented).
- OIDC / `/.well-known/openid-configuration`, JWKS, JWT, `userinfo_endpoint`
  (not implemented).
- `client_credentials` grant, device code flow (not implemented).
- MCP resource/audience binding (not implemented).
- DB-backed session storage — the authorize session is in-process memory and
  resets on server restart; that is acceptable for this phase.
