# OAuth2 Shared-Key Bridge Threat Model

## Status

The OAuth subject model substrate exists, but no public bridge issuance endpoint
or UI exists yet. OAuth code, access-token, and refresh-token rows now
distinguish `managed_user` and `shared_key` subjects.

The current internal chain is:

```text
oauth_authorization_codes.subject_kind / subject_id / shared_key_hash
-> authorization_code token exchange
-> oauth_access_tokens.subject_kind / subject_id / shared_key_hash
-> oauth_refresh_tokens.subject_kind / subject_id / shared_key_hash
-> refresh rotation
-> OAuth2Verifier managed-user dispatch
```

Important current design facts:

- `OAuthAuthorizationCodeRecord`, `OAuthAccessTokenRecord`, and
  `OAuthRefreshTokenRecord` have explicit `subject_kind` and `subject_id`
  fields. `managed_user` subjects carry `user_id`; `shared_key` subjects carry
  `shared_key_hash` and no `user_id`.
- `OAuth2Verifier` still dispatches only managed-user OAuth subjects. Shared-key
  OAuth subjects are explicitly rejected until the next implementation phase.
- Managed-user OAuth records may still carry bridge metadata when explicitly
  seeded, but `shared_key_hash` does not change managed-user identity.
- A bridge OAuth token is still an `OAuth2Token`, not `SharedKey`.
- Agent transport endpoints still reject `OAuth2Token`.
- Current-session identity is still keyed by OAuth token/user/client semantics
  for managed-user OAuth tokens. Shared-key OAuth current-session dispatch is
  not implemented yet.

## Non-goals

This design does not add or permit:

- Blank OAuth field fallback.
- No-auth fallback.
- Open anonymous bridge.
- OAuth token access to agent transport endpoints.
- Plaintext shared key storage.
- Fake managed user identity.
- A public bridge endpoint in this commit.

## Identity Problem

OAuth tokens are still backed by `user_id`. A shared-key-only caller has no
managed `user_id`. Therefore a public shared-key OAuth bridge cannot be safely
implemented by only adding a route; it must choose an explicit subject model.

The central design question is:

```text
Where does user_id come from for a shared-key bridge OAuth token?
Is the token a managed user token, or is it a shared-key principal token?
```

Until that answer is explicit, public bridge issuance must remain unimplemented.

## Threat Model

The bridge threat model assumes a public HTTPS WebCodex server that may be used
by OAuth-only hosts which cannot configure a static Bearer/API-key header.
Attackers may try to:

- Turn blank OAuth client fields into no-auth or shared-key fallback.
- Convert open anonymous access into a reusable OAuth token.
- Use a shared key to mint account-management or admin OAuth scopes.
- Reuse an OAuth2 access token on agent transport endpoints.
- Cause different shared-key groups to see each other's projects or jobs.
- Persist or leak the plaintext shared key through logs, DB rows, redirects, or
  session state.
- Bind a bridge token to a synthetic managed user that bypasses normal audit,
  disablement, or revocation semantics.

The bridge must preserve the existing boundaries: OAuth tokens remain OAuth
tokens, scope enforcement still applies, agent transport remains unavailable to
OAuth2 tokens, and shared-key grouping is represented only by a stored
SHA-256-derived `shared_key_hash`.

## Candidate Identity Models

### A. Managed-account-bound bridge

The user first authenticates with a managed PAT or account credential, then
enters a shared key. The issued OAuth token stores:

```text
user_id = managed user
shared_key_hash = hash(shared key)
```

Advantages:

- Clear audit and user lifecycle.
- Existing `OAuth2Verifier` user checks still apply.
- Disabled users can be invalidated immediately.
- No fake user is needed.

Disadvantages:

- Does not solve pure shared-key-only OAuth onboarding.
- Still requires a managed account or PAT.

### B. Shared-key principal OAuth token

Add an explicit principal model, for example:

```text
subject_kind = managed_user | shared_key
subject_id = user_id or shared_key_hash
```

An equivalent schema is acceptable if it makes the subject kind explicit and
does not overload `user_id`.

Advantages:

- Semantically accurate.
- Supports pure shared-key OAuth onboarding.
- Does not forge managed user identity.

Disadvantages:

- Requires larger changes across DB schema, `AuthContext`, `OAuth2Verifier`,
  account APIs, scope enforcement, and tests.
- Must define which account-control endpoints are rejected for shared-key
  principals.
- Must define revocation, rotation, audit, and current-session semantics for
  non-user OAuth subjects.

### C. Synthetic managed user

Create a managed-looking user for each shared key, such as:

```text
user_id = shared-key:<hash>
```

or automatically insert a row in `users`.

This is not recommended unless a future design adds strict constraints that
make it auditable and reversible.

Reasons:

- It pollutes the managed user model.
- Audit and account lifecycle become ambiguous.
- Disabled and revocation semantics become unclear.
- It makes a shared key look like a real user.

## Recommended v1

Recommended v1: do not implement a public bridge endpoint until the subject
model is explicit.

For the first safe implementation, prefer the managed-account-bound bridge if
the goal is production OAuth security. Prefer a shared-key principal OAuth token
only if the product requirement is pure quick-start OAuth onboarding. Do not use
synthetic managed users.

Suggested staged roadmap:

1. Phase A: document the threat model and endpoint contract.
2. Phase B: implement a managed-account-bound bridge only, behind an explicit
   config flag.
3. Phase C: if pure shared-key OAuth is still required, design a shared-key
   principal schema separately.

## Endpoint Contract Draft

This is a draft contract only. It does not imply that these routes exist.

Proposed route shape:

```text
GET /oauth/authorize?bridge=shared_key
POST /oauth/authorize/bridge
```

`GET /oauth/authorize?bridge=shared_key` would validate the OAuth request and
render a bridge form only after the OAuth boundary is trustworthy.
`POST /oauth/authorize/bridge` would verify the bridge subject requirements and
issue an authorization code with `shared_key_hash`.

Required contract:

- Disabled by default behind an explicit config flag.
- Validate `client_id` and exact `redirect_uri` before rendering any bridge
  form or redirecting.
- Require `response_type=code`.
- Require PKCE S256.
- Preserve `state` exactly according to the existing authorize endpoint
  semantics.
- Never accept blank OAuth client fields as shared-key fallback.
- Never accept no-auth fallback.
- Never accept open anonymous as a bridge subject.
- Store only SHA-256 `shared_key_hash`; never store plaintext shared keys.
- Issue an authorization code with `subject_kind = shared_key`,
  `subject_id = shared_key_hash`, `user_id = NULL`, and `shared_key_hash`.
- Access and refresh tokens inherit the OAuth subject fields and
  `shared_key_hash` through the existing token exchange substrate.
- Refresh rotation preserves the OAuth subject fields and `shared_key_hash`.
- Normalize requested scopes against the OAuth client's `allowed_scopes` and
  the global OAuth scope registry.
- Do not grant dangerous scopes such as `account:manage` or `admin` to a
  shared-key bridge unless explicitly justified and tested.
- OAuth2 tokens remain rejected on agent transport endpoints.

The contract must also define the subject model before implementation:

- Managed-account-bound v1: require an authenticated managed user before
  accepting the shared key, and set `user_id` to that managed user.
- Shared-key-principal v1: do not reuse `user_id`; first add an explicit
  non-user OAuth subject model.

## Scope Policy

Current OAuth delegation supports these non-agent scopes:

- `runtime:read`
- `project:read`
- `project:write`
- `job:run`
- `account:manage`

Current OAuth delegation excludes these scopes:

- `agent:register`
- `agent:poll`
- `agent:result`
- `agent:job_update`
- `admin`

Shared-key bridge OAuth tokens should default to runtime, project, and job
scopes only. They must not receive account-management or admin scopes by
default.

Recommended bridge policy:

- Allow by default: `runtime:read`, `project:read`.
- Allow only after explicit product/security review: `project:write`,
  `job:run`.
- Deny by default: `account:manage`.
- Always deny for OAuth bridge tokens: `admin`, `agent:register`,
  `agent:poll`, `agent:result`, `agent:job_update`.

`account:manage` is globally delegable for normal OAuth clients today, but a
shared-key bridge token is not a normal managed-user delegation unless the
managed-account-bound model is selected and explicitly justified. Even then,
bridge issuance should not grant `account:manage` by default.

## Current Session Decision

`shared_key_hash` affects project and job visibility, not current-session
principal identity.

Bridge OAuth tokens keep OAuth current-session identity semantics unless a
future design explicitly changes it. In the current implementation,
current-session identity for an OAuth2 token follows OAuth token/user/client
fields first; `shared_key_hash` is only a fallback stable id and is not used to
aggregate bridge sessions across OAuth subjects.

## Acceptance Tests for Future Implementation

Before any public bridge endpoint is implemented, tests must cover:

- Disabled by default.
- Invalid `client_id` returns a direct error with no redirect.
- `redirect_uri` exact match is required before any redirect or form render.
- PKCE S256 is required.
- `state` is preserved.
- Blank OAuth client fields do not fallback to shared key, no-auth, or static
  Bearer behavior.
- Open anonymous cannot bridge.
- Invalid shared key is rejected without issuing a code.
- Valid shared key issues an authorization code with `subject_kind =
  shared_key`, `subject_id = shared_key_hash`, `user_id = NULL`, and
  `shared_key_hash`.
- Token exchange propagates the OAuth subject fields and `shared_key_hash`.
- Refresh rotation preserves the OAuth subject fields and `shared_key_hash`.
- OAuth scope enforcement still applies.
- `account:manage` and admin-like scopes are denied or explicitly gated.
- Agent transport endpoints reject a bridge OAuth token.
- Different `shared_key_hash` groups remain isolated for project and job
  visibility.
- Plaintext shared key is never stored.
- Missing or disabled managed users invalidate managed-account-bound bridge
  tokens.
- Current-session behavior remains OAuth-token/user/client based unless a
  future design intentionally changes it.

## Open Questions

1. Is pure shared-key OAuth onboarding a product requirement, or is a
   managed-account-bound bridge enough?
2. Should shared-key bridge support `project:write` or `job:run` by default?
3. Should bridge tokens be revocable by shared-key hash?
4. Should changing or rotating a shared key revoke bridge-issued OAuth tokens?
5. Does the server need a bridge-specific client registration mode?
