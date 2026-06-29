# OAuth2 internals

This document describes the internal OAuth2 data model and storage layer in
WebCodex. For the user-facing authentication model, see
[AUTH_MODEL.md](AUTH_MODEL.md). For the overall auth architecture, see
[AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md).

## Current phase

**Phase 2c-1** enables `OAuth2Verifier` to validate opaque `wc_oat_*` access
tokens. OAuth2 access tokens are now accepted by `AuthMiddleware` on regular
HTTP endpoints (API, MCP). They are rejected on agent-transport paths and the
QUIC surface.

### Phase 2b-1: `POST /oauth/token`

The token endpoint implements RFC 6749 §4.1.3 (authorization code grant) with
PKCE (RFC 7636) support:

- **Grant type**: `authorization_code` only
- **Client authentication**: `client_id` + `client_secret` in form body
  (confidential client required)
- **PKCE**: S256 method required when `config.oauth2.require_pkce` is true
- **Form encoding**: `application/x-www-form-urlencoded`
- **Authorization codes**: atomically consumed via
  `consume_oauth_authorization_code_by_hash()` — single-use, short-lived
- **Tokens**: opaque (`wc_oat_*`, `wc_ort_*`), only SHA-256 hashes stored
- **Enable gate**: returns 503 when `config.oauth2.enabled` is false

Error responses follow RFC 6749 §5.2 format:

```json
{
  "error": "invalid_grant",
  "error_description": "..."
}
```

### Phase 2b-1.1: hardening

The token endpoint is hardened with the following changes:

1. **No-store headers**: all OAuth2 JSON responses (success and error) include
   `Cache-Control: no-store` and `Pragma: no-cache` to prevent intermediaries
   from caching sensitive tokens or error context.
2. **Content-Type enforcement**: only `application/x-www-form-urlencoded` is
   accepted (with optional `; charset=...`). Missing or wrong Content-Type
   returns 415 Unsupported Media Type.
3. **Body size limit**: request bodies are bounded at 16 KiB. Exceeding the
   limit returns 413 Payload Too Large (or 400 if Content-Length is absent).
4. **Transactional exchange**: authorization code consumption and token
   insertion happen in a single SQLite transaction via
   `exchange_oauth_authorization_code_for_tokens()`. If the refresh token
   INSERT fails, the entire exchange is rolled back — no partial writes.

Post-consume validation semantics are preserved:

- Wrong `client_secret` → code **not** consumed (rejected before exchange)
- `client_id` / `redirect_uri` / PKCE mismatch → code **consumed**
  (post-consume failures are intentional; the code cannot be retried)

### Phase 2b-1.2: no tokens on failed exchange

Fixes a bug where `POST /oauth/token` inserted access and refresh tokens
even when post-consume validation (client_id / redirect_uri / PKCE)
failed. The root cause was that `exchange_oauth_authorization_code_for_tokens()`
was called before validation, so tokens were persisted regardless of the
validation outcome.

**New handler flow**:

1. Client authentication (`client_secret` verified before code consumption)
2. Read authorization code metadata (`get_oauth_authorization_code_by_hash`,
   no consumption)
3. Validate `client_id` match → on failure: consume code, return
   `invalid_grant`, **no tokens**
4. Validate `redirect_uri` match → on failure: consume code, return
   `invalid_grant`, **no tokens**
5. Validate PKCE S256 → on failure: consume code, return `invalid_grant`,
   **no tokens**
6. All validations passed →
   `exchange_oauth_authorization_code_for_tokens()` atomically consumes
   code + inserts both tokens in a single transaction

**Semantic summary**:

| Scenario | Code consumed? | Tokens inserted? |
| --- | --- | --- |
| Wrong `client_secret` | No | No |
| Unknown / expired / revoked code | No | No |
| `client_id` mismatch | Yes | **No** |
| `redirect_uri` mismatch | Yes | **No** |
| PKCE mismatch | Yes | **No** |
| Valid exchange | Yes | Yes (transactional) |

### Phase 2b-2: refresh_token grant

`POST /oauth/token` now supports `grant_type=refresh_token` with refresh
token rotation (RFC 6749 §6).

**Request parameters**:

- `grant_type=refresh_token`
- `refresh_token` — plaintext refresh token
- `client_id` + `client_secret` — confidential client authentication

The `scope` parameter is **not yet supported**; including it returns
`invalid_request`.

**Handler flow**:

1. Client authentication (`client_secret` verified before any token
   operations).
2. Hash the plaintext refresh token and look up the record.
3. Validate: token exists, not revoked, not expired, `client_id` matches.
4. Call `rotate_oauth_refresh_token()` — a single SQLite transaction that:
   - Revokes the old refresh token (`revoked_at = now`, `last_used_at = now`)
   - Inserts a new access token
   - Inserts a new refresh token (`rotated_from_id = old.id`)
   - Commits
5. Return the new access token and new refresh token.

**Security invariants**:

| Scenario | Old RT revoked? | New tokens inserted? |
| --- | --- | --- |
| Wrong `client_secret` | No | No |
| Unknown refresh token | No | No |
| Expired refresh token | No | No |
| Revoked refresh token | No | No |
| `client_id` mismatch | No | No |
| Valid rotation | Yes | Yes (transactional) |

- Refresh token plaintext is never stored; only SHA-256 hashes are persisted.
- Old refresh tokens can only be used once (rotation revokes them).
- New tokens inherit `user_id`, `scopes`, `resource`, and `client_id` from
  the old refresh token.

### Phase 2b-3: `POST /oauth/revoke`

The token revocation endpoint implements RFC 7009. Clients can revoke access
tokens and refresh tokens.

**Request parameters** (form body):

- `token` — the plaintext token to revoke (required)
- `token_type_hint` — `access_token` or `refresh_token` (optional)
- `client_id` — OAuth2 client ID (required)
- `client_secret` — OAuth2 client secret (required)

**Handler flow**:

1. Config check, OAuth2 enable gate.
2. Content-Type enforcement (same as `/oauth/token`).
3. Body size limit (16 KiB).
4. Parse form body.
5. Validate `token`, `client_id`, `client_secret`.
6. Client authentication (`verify_oauth_client_secret` +
   `get_oauth_client_by_client_id`).
7. Hash the plaintext token.
8. Dispatch by `token_type_hint`:
   - `access_token` → try `revoke_oauth_access_token_by_hash_for_client`
   - `refresh_token` → try `revoke_oauth_refresh_token_by_hash_for_client`
   - missing / unknown → try both
9. Return HTTP 200 with `{}`.

**Security invariants**:

| Scenario | Token revoked? | HTTP status | Response |
| --- | --- | --- | --- |
| Token belongs to this client | Yes (idempotent) | 200 | `{}` |
| Token does not exist | No-op | 200 | `{}` |
| Token belongs to other client | No-op | 200 | `{}` |
| Token already revoked | No-op (COALESCE) | 200 | `{}` |
| Wrong `client_secret` | No | 401 | `invalid_client` |
| Unknown client | No | 401 | `invalid_client` |
| Revoked client | No | 401 | `invalid_client` |

**Design choices**:

- `token_type_hint` is advisory per RFC 7009 §2.1. Unknown hints are treated
  as "no hint" (try both token types) rather than returning an error.
- The SQL uses `COALESCE(revoked_at, ?now)` so repeated revocations are
  idempotent — `revoked_at` is only set once.
- `last_used_at` is **not** updated on revocation; revocation is not a "use".
- Token records are never deleted; only `revoked_at` is set.
- The response `{}` does not disclose whether the token existed, which client
  it belongs to, or what type it is — preventing token enumeration.

### Phase 2c-1: OAuth2 access token verification

`OAuth2Verifier` now validates opaque `wc_oat_*` access tokens so that
OAuth2-issued tokens can be used as bearer tokens on HTTP endpoints protected
by `AuthMiddleware`.

**Validation flow**:

1. Only tokens starting with `wc_oat_` are handled. Non-matching tokens
   return `Ok(None)` (not recognized), allowing `PatVerifier` to handle them.
2. If OAuth2 is disabled in config, returns `Ok(None)` — the subsystem is
   dormant, not rejecting.
3. Hash the plaintext token and look up `oauth_access_tokens` (the query
   enforces `revoked_at IS NULL`).
4. Check `expires_at > now` — expired tokens are rejected.
5. Verify the owning client is not revoked (`get_oauth_client_by_client_id`
   enforces `revoked_at IS NULL`).
6. Verify the owning user is not disabled (consistent with `PatVerifier`).
7. On success, update `last_used_at` and return an `AuthContext` with
   `AuthKind::OAuth2Token`.

**Surface restrictions**:

- **HTTP `AuthMiddleware`** (API, MCP): OAuth2 tokens are accepted on all
  regular paths.
- **Agent transport paths** (`/api/shell/agent/*`, `/api/agents/ws`): OAuth2
  tokens are **rejected** — these endpoints require agent tokens or bootstrap
  auth.
- **QUIC / `authenticate_bearer()`**: OAuth2 tokens are **rejected** — the
  QUIC surface is agent-only.

**What is NOT covered**:

- Route-level OAuth scope enforcement is not implemented. The `scopes` field
  from the access token is populated in `AuthContext` but no handler checks
  it yet.
- `resource` (audience) binding is not enforced.

## Design decisions

### Opaque DB-backed tokens (not JWT)

The first OAuth2 implementation uses **opaque tokens** stored in SQLite with
SHA-256 hashes. This is the simplest approach that satisfies the security
requirements:

- Token plaintext is returned to the client **once** at creation time.
- Only the SHA-256 hash is stored in the database.
- Token validation is a hash lookup, not a cryptographic signature
  verification.
- No JWT, JWKS, or OIDC metadata is required in this phase.

JWT/JWKS/OIDC can be added later as an extension where MCP or OIDC clients
require it. The verifier chain (`TokenVerifier` trait) already supports
plugging in additional verification strategies.

### Token formats

| Token type | Prefix | Example |
| --- | --- | --- |
| Client ID | `wc_client_` | `wc_client_a1b2c3...` |
| Client secret | `wc_csec_` | `wc_csec_d4e5f6...` |
| Authorization code | `wc_oac_` | `wc_oac_789abc...` |
| Access token | `wc_oat_` | `wc_oat_def012...` |
| Refresh token | `wc_ort_` | `wc_ort_345678...` |

All tokens use 256 bits of hex-encoded randomness (64 hex characters after
the prefix), matching the entropy level of PAT and agent tokens.

### Coexistence with PAT

PAT and OAuth2 coexist. The verifier chain tries `PatVerifier` first, then
`OAuth2Verifier`. Existing `Authorization: Bearer <PAT>` requests continue
to work unchanged. Client-agent pairing remains unchanged.

## Database schema

### `oauth_clients`

Registered OAuth2 clients (applications).

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | Internal UUID |
| `client_id` | TEXT UNIQUE | Public identifier (`wc_client_*`) |
| `client_secret_hash` | TEXT | SHA-256 of the client secret |
| `name` | TEXT | Human-readable application name |
| `owner_user_id` | TEXT FK | User who registered the client |
| `redirect_uris` | TEXT | Newline-separated allowed redirect URIs |
| `allowed_scopes` | TEXT | Space-separated scope list |
| `created_at` | INTEGER | Unix timestamp |
| `revoked_at` | INTEGER nullable | Set when the client is revoked |

### `oauth_authorization_codes`

One-time authorization codes for the authorization code flow.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | Internal UUID |
| `code_hash` | TEXT UNIQUE | SHA-256 of the code |
| `client_id` | TEXT FK | Client that requested the code |
| `user_id` | TEXT FK | User who authorized |
| `redirect_uri` | TEXT | Redirect URI used |
| `scopes` | TEXT | Space-separated granted scopes |
| `code_challenge` | TEXT nullable | PKCE S256 challenge |
| `code_challenge_method` | TEXT nullable | Only `"S256"` supported |
| `resource` | TEXT nullable | MCP audience / resource indicator |
| `created_at` | INTEGER | Unix timestamp |
| `expires_at` | INTEGER | Default: created_at + 300s |
| `used_at` | INTEGER nullable | Set when exchanged |
| `revoked_at` | INTEGER nullable | Set when revoked |

### `oauth_access_tokens`

Short-lived access tokens.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | Internal UUID |
| `token_hash` | TEXT UNIQUE | SHA-256 of the token |
| `client_id` | TEXT FK | Client that requested the token |
| `user_id` | TEXT FK | Authorized user |
| `scopes` | TEXT | Space-separated granted scopes |
| `resource` | TEXT nullable | MCP audience / resource indicator |
| `created_at` | INTEGER | Unix timestamp |
| `expires_at` | INTEGER | Default: created_at + 3600s |
| `revoked_at` | INTEGER nullable | Set when revoked |
| `last_used_at` | INTEGER nullable | Updated on each use |

### `oauth_refresh_tokens`

Long-lived refresh tokens with rotation support.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | Internal UUID |
| `token_hash` | TEXT UNIQUE | SHA-256 of the token |
| `client_id` | TEXT FK | Client that requested the token |
| `user_id` | TEXT FK | Authorized user |
| `scopes` | TEXT | Space-separated granted scopes |
| `resource` | TEXT nullable | MCP audience / resource indicator |
| `created_at` | INTEGER | Unix timestamp |
| `expires_at` | INTEGER | Default: created_at + 2592000s (30d) |
| `revoked_at` | INTEGER nullable | Set when revoked |
| `last_used_at` | INTEGER nullable | Updated on each use |
| `rotated_from_id` | TEXT nullable | Previous refresh token (rotation) |

## Configuration

OAuth2 is configured via `WEBCODEX_OAUTH2_*` environment variables. All
settings have sensible defaults; OAuth2 is **disabled by default**.

| Env var | Default | Description |
| --- | --- | --- |
| `WEBCODEX_OAUTH2_ENABLED` | `false` | Enable the OAuth2 subsystem |
| `WEBCODEX_OAUTH2_ISSUER` | — | OAuth2-specific issuer URL (takes precedence) |
| `WEBCODEX_PUBLIC_URL` | — | Fallback issuer URL if `OAUTH2_ISSUER` is not set |
| `WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS` | `3600` | Access token TTL |
| `WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS` | `2592000` | Refresh token TTL (30 days) |
| `WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS` | `300` | Authorization code TTL (5 min) |
| `WEBCODEX_OAUTH2_REQUIRE_PKCE` | `true` | Require PKCE S256 |

## Security notes

- **Single hash source**: `insert_oauth_client()` reads the client secret hash
  exclusively from `OAuthClientRecord.client_secret_hash`. There is no
  separate parameter — the caller must hash the secret before constructing the
  record.
- **Constant-time secret verification**: `verify_oauth_client_secret()` compares
  the computed hash against the stored hash using constant-time comparison to
  avoid timing side-channels.
- **Atomic code consumption**: `consume_oauth_authorization_code_by_hash()` uses
  a single conditional UPDATE to guarantee that an authorization code can only
  be exchanged once. The older `mark_oauth_authorization_code_used()` is
  retained for backward compatibility but does not enforce expiry or revocation
  checks.

## What is NOT implemented yet

- `/oauth/authorize` endpoint
- `/oauth/userinfo` endpoint
- `/.well-known/oauth-authorization-server` metadata
- `client_credentials` grant
- Route-level OAuth scope enforcement
- MCP OAuth (resource indicator / audience binding)
- JWT/JWKS/OIDC
- Handler migration to `Principal`
