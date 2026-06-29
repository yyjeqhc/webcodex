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
          ├─ Bootstrap check (WEBCODEX_TOKEN)
          ├─ PAT / Agent token lookup (api_keys table, SHA-256 hash)
          ├─ Account credential lookup (account_credentials table)
          └─ Inject AuthContext into Depot
              └─ Handler reads AuthContext → dispatches tool
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
The PAT validation flow is:

1. Extract bearer token from the `Authorization` header
2. Check if it matches the bootstrap `WEBCODEX_TOKEN` (constant-time compare)
3. SHA-256 hash the token → look up in `api_keys` table
4. Validate user exists, is not disabled, token is not expired
5. Distinguish `ApiToken` vs `AgentToken` by the `kind` column
6. For agent tokens: enforce the agent-transport-path gate

This flow is wrapped in the `PatVerifier` struct implementing the
`TokenVerifier` trait. It is functionally identical to the pre-refactoring
logic.

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
  implemented in a future phase to validate OAuth2 JWT tokens.

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
4. `OAuth2Verifier` decodes the JWT, validates against OIDC JWKS
5. Maps claims to `AuthContext` with `AuthMethod::OAuth2`

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
6. **`OAuth2Verifier`**: stub for future OAuth2 JWT validation.

All existing behavior is preserved. No handler signatures changed. No
external API surface changed. No database schema changes.
