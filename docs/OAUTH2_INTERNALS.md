# OAuth2 internals

This document describes the internal OAuth2 data model and storage layer in
WebCodex. For the user-facing authentication model, see
[AUTH_MODEL.md](AUTH_MODEL.md). For the overall auth architecture, see
[AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md).

## Current phase

**Phase 2a** implements only the internal infrastructure:

- OAuth2 configuration (`OAuth2Config`)
- Database tables and CRUD helpers
- Token/client generation and hashing utilities

No OAuth2 HTTP endpoints are exposed. No OAuth2 tokens are accepted by
`AuthMiddleware`. The GPT Actions / MCP / REST surface continues to accept
only the existing token formats (PAT, agent tokens, account credentials).

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
| `WEBCODEX_PUBLIC_URL` | — | Public issuer URL (also used for `/.well-known/*`) |
| `WEBCODEX_OAUTH2_ISSUER` | — | Fallback issuer if `PUBLIC_URL` is not set |
| `WEBCODEX_OAUTH2_ACCESS_TOKEN_TTL_SECS` | `3600` | Access token TTL |
| `WEBCODEX_OAUTH2_REFRESH_TOKEN_TTL_SECS` | `2592000` | Refresh token TTL (30 days) |
| `WEBCODEX_OAUTH2_AUTH_CODE_TTL_SECS` | `300` | Authorization code TTL (5 min) |
| `WEBCODEX_OAUTH2_REQUIRE_PKCE` | `true` | Require PKCE S256 |

## What is NOT implemented yet

- `/oauth/authorize` endpoint
- `/oauth/token` endpoint
- `/oauth/revoke` endpoint
- `/oauth/userinfo` endpoint
- `/.well-known/oauth-authorization-server` metadata
- `OAuth2Verifier` real validation (still a stub)
- OAuth2 tokens accepted by `AuthMiddleware`
- JWT/JWKS/OIDC
- Handler migration to `Principal`
