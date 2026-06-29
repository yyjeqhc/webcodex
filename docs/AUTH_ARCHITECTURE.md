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
          │     └─ OAuth2Verifier: stub (returns "not recognized")
          ├─ enforce_token_surface(ctx, path)
          │     ├─ Agent token → only agent transport endpoints
          │     └─ Account credential → only account control endpoints
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
| `AuthKind` | `auth/mod.rs` | Enum: Bootstrap, ApiToken, AgentToken, AccountCredential |
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
- **`OAuth2Verifier`** — stub that always returns `Ok(None)`. Will be
  implemented in a future phase to validate WebCodex-issued OAuth2 access
  tokens (initially opaque DB-backed; JWT/JWKS optional later).

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
6. **`OAuth2Verifier`**: stub for future OAuth2 validation.

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
