# Authentication architecture

[English](AUTH_ARCHITECTURE.md) | [简体中文](AUTH_ARCHITECTURE.zh-CN.md)

This document describes the internal authentication architecture of WebCodex.
For the user-facing credential model (which token to use where), see
[AUTH_MODEL.md](AUTH_MODEL.md).

## Overview

All protected API endpoints share a single authentication pipeline:

```
HTTP request
  └─ Bearer token extraction
      └─ AuthMiddleware (Salvo hoop)
          ├─ authenticate(config, db, token)
          │     ├─ PatVerifier: bootstrap → PAT → agent token → account credential
          │     └─ OAuth2Verifier: wc_oat_* access token validation
          ├─ enforce_token_surface(ctx, path)
          │     ├─ Agent token → only agent transport endpoints
          │     └─ Account credential → only account control endpoints
          ├─ enforce OAuth route scope policy when ctx.kind == OAuth2Token
          │     ├─ Simple route → require delegated scope
          │     ├─ Body-aware route → handler enforces after JSON parse
          │     └─ FirstPartyOnly / AgentSurface / Unknown → 403
          └─ Inject AuthContext into Depot
              └─ Handler reads AuthContext → dispatches tool
```

The QUIC agent transport uses `authenticate_bearer()`, which calls the same
verifier chain but rejects account credentials (they are only valid on HTTP
account-control endpoints):

```
QUIC agent transport
  └─ authenticate_bearer(config, db, token)
      ├─ authenticate(config, db, token)  // same verifier chain
      └─ Reject if AuthKind::AccountCredential
```

## OAuth2 authorize and client management

Two OAuth2 surfaces intentionally sit **outside** the shared `AuthMiddleware`
pipeline and do their own authentication:

- `GET /oauth/authorize` accepts either a first-party Bearer token
  (Bootstrap / PAT → direct authorization-code issuance, backward compatible)
  or a short-lived `webcodex_authorize_session` cookie (browser consent flow).
  No Bearer and no session → minimal HTML login page. OAuth2 access tokens,
  agent tokens, and account credentials presented as Bearer are rejected
  (403); the handler reuses `authenticate()` + `is_authorize_identity_allowed()`
  so the first-party identity boundary is identical to the pre-session era.
- `POST /oauth/authorize/login` and `POST /oauth/authorize/consent` do their
  own token/session validation (route policy `Public`). The login form
  authenticates a PAT/bootstrap token via the shared verifier chain and sets
  an opaque `HttpOnly; SameSite=Lax` session cookie; only the SHA-256 hash of
  the session id is kept in an in-process session store. The consent endpoint
  requires a valid session and revalidates client/redirect/scope/PKCE from
  scratch.

The first-party OAuth client management API
(`POST /api/oauth/clients/{create,list,revoke}`) **is** behind
`AuthMiddleware`. Its route policy is `FirstPartyOnly`, so `OAuth2Token`
callers are rejected even when they hold `account:manage`; agent tokens and
account credentials are rejected by the existing surface gates. The plaintext
`client_secret` is returned only once at creation; only its SHA-256 hash is
stored.

## Module structure

```
src/auth/
├── mod.rs          — AuthMiddleware, AuthContext, TokenVerifier, authenticate_bearer
├── principal.rs    — Principal, AuthMethod, AuthError
├── scopes.rs       — Scope constants, validation, require_scope
└── pat.rs          — Token generation, hashing, validation utilities
```

### Core types

| Type | Location | Purpose |
| --- | --- | --- |
| `AuthContext` | `auth/mod.rs` | Depot-injected struct carrying raw auth fields |
| `AuthKind` | `auth/mod.rs` | Enum: Bootstrap, ApiToken, AgentToken, AccountCredential, OAuth2Token |
| `Principal` | `auth/principal.rs` | Higher-level identity abstraction |
| `AuthMethod` | `auth/principal.rs` | Enum: Bootstrap, Pat, AgentToken, AccountCredential, OAuth2 |
| `AuthError` | `auth/principal.rs` | Error type for auth/authorization failures |
| `TokenVerifier` | `auth/mod.rs` | Trait for pluggable token verification |

### Relationship: AuthContext ↔ Principal

`AuthContext` is the low-level struct injected into the Salvo `Depot` by
`AuthMiddleware`. It carries the raw database fields (user_id, api_key_id,
scopes, etc.).

`Principal` is a higher-level abstraction derived from `AuthContext`. It
unifies the identity representation regardless of whether the caller used a
PAT, agent token, account credential, or (future) OAuth2 bearer token.

During this refactoring phase both types coexist:

- **`AuthContext`** remains the depot-injected type — all existing handlers
  continue to use `depot.obtain::<AuthContext>()`.
- **`Principal`** is available via `auth_context.to_principal()` for code
  that wants the cleaner abstraction.
- A future phase can migrate handlers to read `Principal` directly from the
  depot.

## PAT compatibility

Existing `Authorization: Bearer <PAT>` requests continue to work unchanged.
The verification is performed by `PatVerifier` (the primary verifier in the
chain), which handles:

1. Auth-disabled mode → bootstrap context
2. Bootstrap token match (constant-time compare) → bootstrap context
3. SHA-256 hash → `api_keys` table lookup → `ApiToken` or `AgentToken`
4. SHA-256 hash → `account_credentials` table lookup → `AccountCredential`
5. User disabled / token expired → error (mapped to 401)

After verification, `enforce_token_surface()` applies the path gate:
- Agent tokens → only agent transport endpoints
- Account credentials → only account control endpoints

Both the HTTP `AuthMiddleware` and the QUIC `authenticate_bearer()` call
the same `authenticate()` function, which runs the verifier chain
(`PatVerifier` → `OAuth2Verifier`). This eliminates the previous
duplication between the two authentication paths.

## TokenVerifier trait

```rust
#[async_trait]
pub(crate) trait TokenVerifier: Send + Sync {
    async fn verify(
        &self,
        config: &Config,
        db: Option<&Arc<Database>>,
        token: &str,
    ) -> Result<Option<AuthContext>, String>;
}
```

The trait returns:
- `Ok(Some(ctx))` — token verified, here's the auth context
- `Ok(None)` — token not recognized by this verifier (try next)
- `Err(msg)` — token recognized but invalid (reject immediately)

Current implementations:
- **`PatVerifier`** — handles bootstrap, PAT, agent tokens, and account
  credentials via the existing database lookup logic.
- **`OAuth2Verifier`** — validates opaque `wc_oat_*` access tokens via
  SHA-256 hash lookup in `oauth_access_tokens`. Rejects expired, revoked,
  and client-revoked tokens. Returns `Ok(None)` for non-`wc_oat_*` tokens,
  allowing `PatVerifier` to handle them.

## OAuth2 extension points (future phase)

The following items are reserved for the OAuth2 implementation:

| Item | Type | Status |
| --- | --- | --- |
| `AuthMethod::OAuth2` | Enum variant | Defined, not yet constructed |
| `OAuth2Verifier` | Struct | Stub — always returns "not recognized" |
| `Principal::from_oauth2_claims_stub()` | Method | Returns `Err` (placeholder) |

When OAuth2 is implemented, the pipeline will be:

1. Extract bearer token
2. Try `PatVerifier` first (existing PAT/agent tokens)
3. If `PatVerifier` returns `None`, try `OAuth2Verifier`
4. `OAuth2Verifier` validates the token and maps claims to `AuthContext`

`OAuth2Verifier` will validate WebCodex-issued OAuth2 access tokens. The
initial implementation may use opaque DB-backed access tokens.
JWT/JWKS/OIDC metadata can be added later where required by MCP clients.

No OAuth2 endpoints will be exposed in this phase. The GPT Actions / MCP /
REST surface continues to accept only the existing token formats.

## Scopes

Scopes are string-based permissions stored space-separated in the database.
Bootstrap auth is treated as holding every scope (`admin` is a wildcard).

| Scope | Purpose |
| --- | --- |
| `runtime:read` | Read runtime status, list tools |
| `project:read` | List and read projects |
| `project:write` | Create projects, write files, apply patches |
| `job:run` | Run jobs and shell commands |
| `agent:register` | Register an agent connection |
| `agent:poll` | Poll for agent work |
| `agent:result` | Submit agent results |
| `agent:job_update` | Send agent job updates |
| `admin` | Full access (wildcard) |
| `account:manage` | Manage own account credentials |

The `require_scope` and `scopes_include` helpers in `scopes.rs` treat `admin`
as satisfying any requirement.

## OAuth2 delegated scope enforcement

Delegated OAuth scopes are enforced only for `AuthKind::OAuth2Token`.
First-party `Bootstrap` and `ApiToken` callers keep their existing behavior and
are not limited by delegated OAuth scopes. `AgentToken` and
`AccountCredential` callers keep using the existing surface gates.

`src/auth/scopes.rs` defines an explicit route policy enum:

| Variant | Meaning |
| --- | --- |
| `Public` | Public OAuth/discovery endpoint |
| `FirstPartyOnly` | Authenticated first-party route such as `/oauth/authorize` |
| `AgentSurface` | Agent/account transport surface not OAuth-delegable |
| `Require(scope)` | Simple path-level delegated scope requirement |
| `BodyAware(policy)` | Handler must inspect the parsed request body |
| `Unknown` | Fail closed for OAuth2 tokens |

`AuthMiddleware` applies the path-level policy after successful
authentication and before handler dispatch. `Require(scope)` checks the
OAuth2 token's stored scopes. `Unknown`, `FirstPartyOnly`, and `AgentSurface`
return HTTP 403. `BodyAware` routes are allowed through the middleware so the
handler can parse JSON without the middleware consuming the body.

The body-aware routes are:

- `POST /api/tools/call`: parses the runtime tool name and applies
  `oauth_scope_policy_for_runtime_tool(name)`.
- `POST /mcp`: requires `runtime:read` for `initialize`, `ping`,
  `tools/list`, and `notifications/initialized`; for `tools/call`, it parses
  `params.name` and applies the same runtime tool policy.

Runtime tool policy maps read-only runtime operations to `runtime:read`,
project inspection to `project:read`, file/patch/artifact/project mutation to
`project:write`, and shell/Codex/cargo/job execution to `job:run`. Unknown
routes, unknown MCP methods, and unknown runtime tools fail closed for OAuth2
tokens.

OAuth2 scope failures return HTTP 403:

```json
{
  "error": "insufficient_scope",
  "error_description": "missing required scope: project:read"
}
```

Where a concrete scope is missing, the response includes
`WWW-Authenticate: Bearer error="insufficient_scope", scope="<scope>"`.
Resource/audience binding and route-level project-resource authorization are
not implemented.

## Authorization flow

```
Handler receives request
  └─ Extract AuthContext from Depot
      └─ ctx.has_scope("project:write")  // or ctx.to_principal().require_scope(...)
          ├─ true → proceed
          └─ false → 403 Forbidden
```

Agent tokens and account credentials are additionally gated by path at the
middleware level (before any handler runs):
- Agent tokens → only `/api/shell/agent/*` and `/api/agents/ws`
- Account credentials → only `/api/users/me`, `/api/tokens/*`,
  `/api/agent-tokens/register_hash`

## Client-agent pairing

The client-agent pairing mechanism is **not affected** by this refactoring.
Agent tokens continue to be bound to an `allowed_client_id` and validated
per-endpoint via `can_use_agent_endpoint()`.

## What changed in this refactoring

### Phase 1 — module restructure and new types

1. **Module restructure**: `src/auth.rs` → `src/auth/` directory with
   `mod.rs`, `principal.rs`, `scopes.rs`, `pat.rs`.
2. **New types**: `Principal`, `AuthMethod`, `AuthError`, `TokenVerifier`,
   `PatVerifier`, `OAuth2Verifier`.
3. **`AuthContext::to_principal()`**: bridge from the low-level context to
   the `Principal` abstraction.
4. **Scope helpers**: `scopes_include()` and `require_scope()` functions in
   `scopes.rs` for use outside the `AuthContext` type.
5. **`PatVerifier`**: the existing PAT validation logic wrapped in the
   `TokenVerifier` trait for composability.
6. **`OAuth2Verifier`**: stub for future OAuth2 validation (implemented in
   Phase 2c-1).

### Phase 1b — verifier chain integration

1. **`authenticate()`**: shared async function that runs the verifier chain
   (`PatVerifier` → `OAuth2Verifier`). This is the single token verification
   path used by both the HTTP `AuthMiddleware` and the QUIC
   `authenticate_bearer()`.
2. **`enforce_token_surface()`**: extracted the token-kind path gate into a
   reusable function. Applied after verification, before handler dispatch.
3. **`AuthMiddleware` rewritten**: the middleware now calls `authenticate()`
   and `enforce_token_surface()` instead of inline bootstrap/DB lookup logic.
4. **`authenticate_bearer()` made async**: the QUIC transport function now
   calls the same `authenticate()` verifier chain, eliminating the previous
   code duplication.
5. **`PatVerifier` is the actual primary verifier**: it handles bootstrap,
   PAT, agent tokens, and account credentials. The inline logic in
   `AuthMiddleware` was removed.

### Phase 1c — cleanup

1. **`authenticate_bearer()` rejects account credentials**: account
   credentials (`wc_acct_*`) are only valid on HTTP account-control
   endpoints. The QUIC/agent transport has no use for them, and accepting
   them would silently update `last_used_at` before the caller rejects the
   connection. `authenticate_bearer()` now filters them out explicitly.
2. **`OAuth2Verifier` doc updated**: comments no longer lock the first
   implementation to JWT/JWKS. The initial implementation may use opaque
   DB-backed access tokens; JWT/JWKS/OIDC metadata can be added later
   where MCP or OIDC clients require it.
3. **Unused re-exports removed**: `AuthMethod`, `KNOWN_SCOPES`,
   `require_scope`, `scopes_include` are no longer re-exported from
   `auth/mod.rs`. They remain available within the `auth` module but are
   not part of the public API surface.
4. **Warning cleanup**: added `#[allow(dead_code)]` to future-use items
   (`AuthMethod::OAuth2`, `AuthError` variants, `Principal` fields/methods,
   `scopes_include`, `require_scope`) so the warning surface reflects
   genuinely unused code rather than reserved extension points.

All existing behavior is preserved. No handler signatures changed. No
external API surface changed. No database schema changes.

### Phase 2a — OAuth2 internal infrastructure

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full data model and
configuration reference.

1. **`OAuth2Config`**: new config struct loaded from `WEBCODEX_OAUTH2_*` env
   vars. Disabled by default. Added to `Config` as a nested field.
2. **Database tables**: `oauth_clients`, `oauth_authorization_codes`,
   `oauth_access_tokens`, `oauth_refresh_tokens` with appropriate indexes.
3. **Data models**: `OAuthClientRecord`, `OAuthAuthorizationCodeRecord`,
   `OAuthAccessTokenRecord`, `OAuthRefreshTokenRecord` in `models.rs`.
4. **Token generation**: `generate_oauth_client_id`,
   `generate_oauth_client_secret`, `generate_oauth_authorization_code`,
   `generate_oauth_access_token`, `generate_oauth_refresh_token` in
   `auth::pat`. All use 256-bit random hex, matching PAT entropy.
5. **Database CRUD**: insert, get-by-hash, mark-used, revoke, and
   verify-client-secret helpers for all four OAuth2 tables.
6. **`AuthMethod` re-export**: restored `pub use principal::AuthMethod` so
   downstream modules can match on the method enum.

No OAuth2 endpoints are exposed. No OAuth2 tokens are accepted by
`AuthMiddleware`. The `OAuth2Verifier` remains a stub (implemented in Phase
2c-1). Existing PAT, agent token, and account credential behavior is
unchanged.

### Phase 2a.1 — tighten storage helpers

1. **Issuer precedence**: `WEBCODEX_OAUTH2_ISSUER` now takes priority over
   `WEBCODEX_PUBLIC_URL`. The OAuth2-specific setting overrides the generic
   public URL.
2. **Single hash source**: `insert_oauth_client()` no longer accepts a
   separate `client_secret_hash` parameter. The hash is read exclusively from
   `OAuthClientRecord.client_secret_hash`, eliminating the dual-source risk.
3. **Atomic code consumption**: new `consume_oauth_authorization_code_by_hash()`
   helper atomically sets `used_at` only when the code is valid (not revoked,
   not used, not expired). Preferred for `/oauth/token` exchange.
4. **Constant-time secret verification**: `verify_oauth_client_secret()` now
   uses constant-time comparison for the hash, preventing timing side-channels
   in client authentication.
5. **`OAuth2Config` doc fix**: comment no longer claims `Config { ... }`
   literals are untouched (they now include the `oauth2` field).

No endpoints, no AuthMiddleware changes, no handler migration.

### Phase 2b-1 — authorization code token exchange

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full endpoint reference.

1. **`POST /oauth/token`**: first OAuth2 HTTP endpoint. Supports
   `grant_type=authorization_code` with confidential client authentication
   (`client_id` + `client_secret` in form body).
2. **PKCE S256**: enforced when `config.oauth2.require_pkce` is true. The
   `code_verifier` is SHA-256 hashed and compared against the stored
   `code_challenge` using constant-time equality.
3. **Atomic code consumption**: authorization codes are consumed via
   `consume_oauth_authorization_code_by_hash()` — single-use, short-lived,
   revoked codes rejected.
4. **Token issuance**: opaque access tokens (`wc_oat_*`) and refresh tokens
   (`wc_ort_*`) are generated, hashed, and stored. Plaintext is returned only
   once in the JSON response.
5. **Enable gate**: `POST /oauth/token` returns 503 when
   `config.oauth2.enabled` is false.
6. **Route**: mounted at `/oauth/token` (public, no `AuthMiddleware`).

Not implemented: `/oauth/authorize`, `refresh_token` grant,
`client_credentials` grant, `/oauth/revoke`, `/.well-known/*`,
`OAuth2Verifier` real validation, MCP OAuth.

### Phase 2b-1.1 — token endpoint hardening

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full reference.

1. **No-store headers**: all OAuth2 responses include `Cache-Control: no-store`
   and `Pragma: no-cache` (RFC 6749 §5.1, §5.2).
2. **Content-Type enforcement**: only `application/x-www-form-urlencoded` is
   accepted. Missing or wrong Content-Type returns 415.
3. **Body size limit**: request bodies bounded at 16 KiB (413 on overflow).
4. **Transactional exchange**: `exchange_oauth_authorization_code_for_tokens()`
   atomically consumes the authorization code and inserts both tokens in a
   single SQLite transaction. No partial writes on failure.

Not implemented: `/oauth/authorize`, `refresh_token` grant,
`client_credentials` grant, `/oauth/revoke`, `/.well-known/*`,
`OAuth2Verifier` real validation, MCP OAuth.

### Phase 2b-1.2 — no tokens on failed exchange

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full reference.

1. **Bug fix**: failed post-consume validation (client_id / redirect_uri /
   PKCE mismatch) no longer inserts access or refresh tokens into the
   database. Previously, `exchange_oauth_authorization_code_for_tokens()`
   was called before validation, inserting tokens regardless of outcome.
2. **New flow**: validation runs against code metadata before code
   consumption. Only when all checks pass does the transactional exchange
   (consume code + insert tokens) execute.
3. **Preserved semantics**: mismatched validation still consumes the
   authorization code (preventing replay), but no tokens are minted.
   Wrong `client_secret` is still rejected before code consumption.

### Phase 2b-2 — refresh_token grant

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full reference.

1. **`grant_type=refresh_token`**: `POST /oauth/token` now supports refresh
   token rotation (RFC 6749 §6). Old refresh tokens are revoked on use; new
   access + refresh token pairs are issued in a single transaction.
2. **Transactional rotation**: `rotate_oauth_refresh_token()` atomically
   revokes the old token, inserts a new access token, and inserts a new
   refresh token (with `rotated_from_id` linking to the old one).
3. **Security**: invalid / expired / revoked / client-mismatch refresh tokens
   return `invalid_grant` without inserting any tokens. Wrong `client_secret`
   returns `invalid_client` without touching the refresh token.
4. **Scope parameter**: including `scope` in a `refresh_token` request is
   rejected with `invalid_request` (not yet supported).
5. **Handler refactor**: `oauth_token` handler is split into
   `handle_authorization_code_grant()` and `handle_refresh_token_grant()`.

Not implemented: `/oauth/authorize`, `client_credentials` grant,
`/.well-known/*`, `OAuth2Verifier` real validation, MCP OAuth.

### Phase 2b-3 — token revocation endpoint

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full reference.

1. **`POST /oauth/revoke`**: implements RFC 7009 token revocation. Clients
   can revoke access tokens and refresh tokens by submitting the plaintext
   token along with `client_id` + `client_secret`.
2. **`token_type_hint`**: advisory per RFC 7009 §2.1. `access_token` tries
   only the access token table; `refresh_token` tries only the refresh token
   table; missing or unknown hints try both.
3. **Idempotent**: revoking an already-revoked, nonexistent, or other-client
   token returns HTTP 200 with `{}` — no token state is disclosed.
4. **Client ownership**: tokens can only be revoked by the client that owns
   them. The SQL `WHERE client_id = ?` clause ensures this.
5. **`last_used_at` not updated**: revocation is not a "use"; only
   `revoked_at` is set (via `COALESCE` for idempotency).
6. **Token records not deleted**: only `revoked_at` is set; the row remains.

Not implemented: `/oauth/authorize`, `client_credentials` grant,
`/.well-known/*`, route-level OAuth scope enforcement, MCP OAuth.

### Phase 2c-1 — OAuth2 access token verification

See [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) for the full reference.

1. **`OAuth2Verifier`**: now validates opaque `wc_oat_*` access tokens
   (previously a stub returning `Ok(None)`). The verifier hashes the
   plaintext token and looks it up in `oauth_access_tokens`.
2. **Validation**: revoked (`revoked_at IS NULL`), expired (`expires_at`),
   revoked client, and disabled user are all rejected.
3. **`last_used_at`**: updated only on successful verification.
4. **`AuthKind::OAuth2Token`**: new variant; mapped to `AuthMethod::OAuth2`
   in `Principal`.
5. **Surface restrictions**: OAuth2 tokens are accepted on all regular HTTP
   paths (API, MCP) via `AuthMiddleware`. They are rejected on agent
   transport paths and the QUIC surface (`authenticate_bearer()`).
6. **Verifier chain**: `PatVerifier` → `OAuth2Verifier`. Non-`wc_oat_*`
   tokens return `Ok(None)` from `OAuth2Verifier`, falling through to
   `PatVerifier` unchanged.
7. **Refresh tokens** (`wc_ort_*`), authorization codes (`wc_oac_*`), client
   secrets (`wc_csec_*`), and client IDs (`wc_client_*`) are never accepted
   as bearer tokens.

Not implemented: `/oauth/authorize`, `client_credentials` grant,
`/.well-known/*`, route-level OAuth scope enforcement, MCP OAuth.

### Phase 2c-1.1 — forbid `last_used_at` updates on rejected surfaces

1. **Pre-rejection**: `wc_oat_*` tokens are now rejected *before*
   `OAuth2Verifier` runs on forbidden surfaces, preventing `last_used_at`
   from being updated on tokens that were never actually used.
2. **`authenticate_bearer()`**: checks `is_oauth2_access_token(token)`
   before calling `authenticate()`; returns `None` immediately.
3. **`AuthMiddleware`**: checks `is_agent_transport_path(path) &&
   is_oauth2_access_token(token)` before calling `authenticate()`; returns
   403 immediately.
4. **`enforce_token_surface()`**: retained as defense-in-depth.

### Phase 2d-1 — protected resource metadata

1. **`GET /.well-known/oauth-protected-resource`**: public endpoint returning
   OAuth Protected Resource Metadata (RFC 9728). No authentication required.
   Returns 404 when OAuth2 is disabled.
2. **Metadata fields**: `resource`, `authorization_servers`,
   `bearer_methods_supported`, `scopes_supported`, `resource_name`.
3. **`resource`**: derived from `config.oauth2.issuer`
   (`WEBCODEX_OAUTH2_ISSUER` → `WEBCODEX_PUBLIC_URL`).
4. **`scopes_supported`**: non-agent scopes only (`runtime:read`,
   `project:read`, `project:write`, `job:run`, `account:manage`). Agent
   scopes and `admin` are excluded.
5. **`WWW-Authenticate` challenge**: `AuthMiddleware` 401 responses include
   `Bearer resource_metadata="<issuer>/.well-known/oauth-protected-resource"`
   when OAuth2 is enabled with an issuer. 403 responses do not include it.
6. **Authorization server metadata** (`/.well-known/oauth-authorization-server`)
   is implemented in Phase 2e-2 and points at the same issuer.

### Phase 2e-0 — authorization endpoint design contract

See [OAUTH2_AUTHORIZE_DESIGN.md](OAUTH2_AUTHORIZE_DESIGN.md) for the full
contract.

1. **Design only**: `/oauth/authorize` is still not implemented in this
   phase. No route, UI, authorization code issuance, token endpoint change, or
   verifier behavior change is introduced.
2. **Identity source**: Phase 2e-1 should protect `GET /oauth/authorize` with
   the existing `AuthMiddleware`. The authorizing user is
   `AuthContext.user_id`; no standalone login page, cookie session, or consent
   UI is introduced yet.
3. **Redirect trust boundary**: unknown clients, revoked clients, missing
   `redirect_uri`, and redirect URI mismatches return direct 400 errors.
   Errors after a registered redirect URI is validated may redirect with
   `error` and the decoded `state` value URL-encoded again.
4. **PKCE**: browser authorization code issuance must always require
   `code_challenge_method=S256` and a `code_challenge`, regardless of the
   legacy token-exchange `require_pkce` config.
5. **Scopes**: requested scopes must be a subset of both
   `client.allowed_scopes` and the global OAuth scope registry. Empty
   requests default to the normalized intersection. `agent:*` and `admin` are
   not valid OAuth delegation scopes.
6. **Code storage**: successful authorization stores only a hash of the
   `wc_oac_*` code in `oauth_authorization_codes` with user, client, redirect,
   scope, resource, PKCE, creation, expiry, unused, and unrevoked metadata.
   The plaintext code appears only in the success redirect.
7. **Metadata gate**: `/.well-known/oauth-authorization-server` remains
   unexposed until the next metadata phase.

### Phase 2e-1a — authorization helper groundwork

Phase 2e-1a adds only pure internal helpers for the future authorization
endpoint. It does not mount `/oauth/authorize`, issue authorization codes,
insert `oauth_authorization_codes`, expose authorization server metadata, or
change `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, `AuthMiddleware`,
MCP security schemes, or GPT Action configuration.

1. **Request parsing type**: `OAuthAuthorizeRequest` represents the future
   authorize query fields: `response_type`, `client_id`, `redirect_uri`,
   `scope`, `state`, `code_challenge`, `code_challenge_method`, and
   `resource`.
2. **Error type**: `OAuthAuthorizeError` distinguishes direct
   pre-redirect-trust errors from errors that can later be mapped to OAuth
   redirect errors after client and redirect URI validation.
3. **Global OAuth scopes**: `oauth_scopes_supported()` exposes the canonical
   non-agent, non-admin OAuth scope registry already advertised by protected
   resource metadata.
4. **Scope normalization**: `normalize_oauth_scopes()` validates explicit
   requested scopes against both `client.allowed_scopes` and the global OAuth
   registry, deduplicates, and orders output canonically. Empty or
   whitespace-only requested scope defaults to the client/global intersection.
5. **Delegation boundary**: `agent:*` and `admin` are not OAuth delegation
   scopes; they are rejected when explicitly requested and filtered out of the
   default intersection.

### Phase 2e-1b — validation-only authorize route

Phase 2e-1b mounts `GET /oauth/authorize` behind `AuthMiddleware`, but the
handler is still validation-only. It does not generate authorization codes,
insert `oauth_authorization_codes`, expose authorization server metadata, or
change `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, MCP security
schemes, or GPT Action configuration.

1. **Protected route**: `/oauth/authorize` is a root OAuth route, not an
   `/api/*` route, and is not included in the GPT Action OpenAPI schema.
2. **Authenticated user**: the handler requires an authenticated
   `AuthContext.user_id`; bootstrap/no-user contexts are rejected.
3. **Redirect trust boundary**: missing, empty, unknown, or revoked
   `client_id`; missing or empty `redirect_uri`; and redirect URI mismatches
   are direct 400 errors with no redirect.
4. **Exact redirect match**: `redirect_uri` must match one registered URI
   exactly after the existing `OAuthClientRecord::redirect_uris_vec()`
   trimming. There is no prefix, host-only, query-subset, wildcard, or
   normalization match.
5. **Redirectable validation**: only after client and redirect URI validation,
   unsupported or empty `response_type`, missing or invalid PKCE, invalid
   scope, and unsupported `resource` redirect to the trusted redirect URI with
   an OAuth `error` parameter.
6. **PKCE and scope**: PKCE S256 is always required, independent of the token
   endpoint compatibility flag, and `normalize_oauth_scopes()` is used for
   authorize-time scope validation.
7. **State**: `state` is opaque. WebCodex does not interpret or trust it. The
   decoded value is preserved semantically and URL-encoded again on redirect
   errors.
8. **Validation success**: Phase 2e-1b returned HTTP 501 without creating a
   code. Phase 2e-1c replaces that path with authorization code issuance.
9. **Metadata gate**: `/.well-known/oauth-authorization-server` remains
   unexposed until the next metadata phase.

### Phase 2e-1c — authorization code issuance

Phase 2e-1c keeps the Phase 2e-1b validation boundary and implements the
success path. It does not add login UI, consent UI, browser sessions,
authorization server metadata, OpenID metadata, MCP security scheme changes,
GPT Action configuration changes, or changes to `/oauth/token`,
`/oauth/revoke`, `OAuth2Verifier`, or `AuthMiddleware` semantics.

1. **Plaintext code**: successful `GET /oauth/authorize` generates a
   `wc_oac_*` authorization code.
2. **Hash-only storage**: only `hash_token(plaintext_code)` is stored in
   `oauth_authorization_codes`, along with client, user, exact redirect URI,
   normalized scopes, `resource = None`, PKCE S256 metadata, creation/expiry,
   `used_at = None`, and `revoked_at = None`.
3. **Success redirect**: the handler redirects to the validated
   `redirect_uri` with `code` and optional decoded/re-encoded `state`. Existing
   redirect URI query parameters are preserved and new parameters are appended
   with URL query-pair encoding.
4. **No token issuance**: `/oauth/authorize` does not return access or refresh
   tokens. `/oauth/token` remains responsible for consuming the authorization
   code and issuing `wc_oat_*` and `wc_ort_*` tokens.
5. **Metadata**: Phase 2e-2 exposes
   `/.well-known/oauth-authorization-server` with only implemented OAuth
   capabilities.

### Phase 2e-2 — authorization server metadata and authorize identity hardening

Phase 2e-2 keeps the existing `/oauth/token`, `/oauth/revoke`,
`OAuth2Verifier`, AuthMiddleware, MCP security scheme, GPT Action
configuration, and artifact/import/action behavior unchanged.

1. **Authorize identity sources**: `/oauth/authorize` remains behind
   `AuthMiddleware`, but the handler explicitly allows only
   `AuthKind::Bootstrap` and `AuthKind::ApiToken` as first-party WebCodex
   identity sources. OAuth2 access tokens cannot be used to obtain new
   authorization codes. Agent tokens and account credentials are rejected by
   existing surface gates or by the authorize handler if they reach it.
   Rejected identities do not insert `oauth_authorization_codes`.
2. **Authorization server metadata**:
   `GET /.well-known/oauth-authorization-server` is public, requires no Bearer
   token, and returns 404 with `{"error":"OAuth2 is not enabled"}` when OAuth2
   is disabled.
3. **Metadata fields**: the response includes `issuer`,
   `authorization_endpoint`, `token_endpoint`, `revocation_endpoint`,
   `response_types_supported = ["code"]`,
   `grant_types_supported = ["authorization_code", "refresh_token"]`,
   `code_challenge_methods_supported = ["S256"]`,
   `token_endpoint_auth_methods_supported = ["client_secret_post", "none"]`,
   and `scopes_supported`.
4. **Issuer-derived URLs**: `issuer` comes from `config.oauth2.issuer` with
   the existing `http://localhost` fallback. Endpoint URLs trim a trailing
   issuer slash before appending `/oauth/authorize`, `/oauth/token`, and
   `/oauth/revoke`.
5. **Scope registry reuse**: `scopes_supported` reuses
   `oauth_scopes_supported()`, the same global OAuth scope registry used by
   protected resource metadata and authorize-time scope normalization.
6. **Still not implemented**: `/.well-known/openid-configuration`, JWKS,
   JWT/OIDC, route-level OAuth scope enforcement, and MCP resource/audience
   binding remain out of scope.

### Phase 2f-0 — route-level OAuth scope policy definition

Phase 2f-0 adds only a policy helper and tests. It does not change
`AuthMiddleware`, `OAuth2Verifier`, OAuth token issuance/revocation,
`/oauth/authorize`, MCP tool execution, GPT Action OpenAPI output, or any HTTP
request behavior.

The helper `required_oauth_scope_for_path_method(method, path)` lives in
`src/auth/scopes.rs`. It maps authenticated route categories onto the existing
OAuth scope constants: runtime reads use `runtime:read`; project reads use
`project:read`; project mutations and artifact import use `project:write`;
execution/job-like operations use `job:run`; and user/token/account,
agent-token management, pairing-create, and audit/admin surfaces use
`account:manage`.

The policy deliberately returns no required OAuth scope for public OAuth
metadata and token endpoints, including
`/.well-known/oauth-protected-resource`,
`/.well-known/oauth-authorization-server`, `/oauth/token`, and `/oauth/revoke`.
It also returns no delegated OAuth scope for `/oauth/authorize`; that route is a
first-party identity boundary controlled by the existing Bootstrap / ApiToken
allowlist, not by OAuth delegated scopes.

When Phase 2f-1 enables enforcement, the check must run only for
`AuthKind::OAuth2Token`. Bootstrap and ApiToken authentication are first-party
WebCodex credentials and must remain unrestricted by delegated OAuth scopes.
AgentToken and AccountCredential surfaces remain governed by the existing
surface gates. Unknown routes currently return `None`; Phase 2f-1 must audit all
authenticated routes before treating `None` as an enforcement bypass. Resource
or audience binding remains unimplemented.
