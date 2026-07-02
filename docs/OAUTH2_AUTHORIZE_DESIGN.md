# OAuth2 Authorization Endpoint Design

## Status

Phase 2e-3 client management and minimal authorize UX. `/oauth/authorize`
accepts either a first-party Bearer PAT for direct authorization-code issuance
or a short-lived first-party authorize-session cookie for the browser login and
consent path. After successful validation it issues an authorization code and
redirects back to the validated client redirect URI.

This document is the implementation contract for Phase 2e. It describes the
request contract, state machine, security invariants, storage shape, and test
plan for the authorization endpoint.

Implemented in Phase 2e-1a:

- Internal `OAuthAuthorizeRequest` and `OAuthAuthorizeError` helpers.
- Pure `parse_authorize_query()` helper with required-parameter and duplicate
  known-parameter checks.
- Reusable global OAuth scope registry helper used by protected resource
  metadata.
- `normalize_oauth_scopes()` with canonical ordering, deduplication, default
  client/global intersection, and explicit exclusion of `agent:*` and `admin`
  from OAuth delegation.

Implemented in Phase 2e-1b:

- `GET /oauth/authorize` mounted behind `AuthMiddleware` for the initial
  validation phase; Phase 2e-3 replaced this with handler self-validation for
  Bearer PAT and authorize-session identity.
- OAuth2 enable gate and authenticated `AuthContext.user_id` requirement.
- Client lookup and exact registered `redirect_uri` validation.
- Direct 400 errors before `client_id` and `redirect_uri` are trusted.
- Redirect errors only after client and redirect URI validation.
- `response_type=code`, PKCE S256, normalized scope, and unsupported
  `resource` validation.
- Validation-only success path, replaced by issuance in Phase 2e-1c.

Implemented in Phase 2e-1c:

- Successful validation generates a plaintext `wc_oac_*` authorization code.
- Only `hash_token(plaintext_code)` and authorization metadata are stored in
  `oauth_authorization_codes`.
- Success redirects to the exact validated `redirect_uri` with `code` and
  optional decoded/re-encoded `state`.
- `/oauth/authorize` does not return access tokens or refresh tokens.
- `/oauth/token` remains responsible for code consumption and token issuance.

Implemented in Phase 2e-2:

- `/oauth/authorize` only accepts first-party WebCodex identity sources:
  `AuthKind::Bootstrap` and `AuthKind::ApiToken`. OAuth2 access tokens,
  agent tokens, account credentials, and other non-first-party identities are
  rejected before authorization code issuance.
- `GET /.well-known/oauth-authorization-server` is a public endpoint.
- Authorization server metadata advertises only implemented OAuth
  capabilities: real authorization/token/revocation endpoints,
  `response_types_supported = ["code"]`,
  `grant_types_supported = ["authorization_code", "refresh_token"]`,
  `code_challenge_methods_supported = ["S256"]`,
  `token_endpoint_auth_methods_supported = ["client_secret_post", "none"]`,
  and `scopes_supported` from the global OAuth scope registry.

Implemented in Phase 2e-3:

- First-party OAuth client management API:
  `/api/oauth/clients/{create,list,revoke}`.
- Minimal browser login + consent UX on `/oauth/authorize`, backed by a
  short-lived in-memory first-party session cookie.
- Bearer PAT direct authorization-code issuance remains supported; OAuth2
  access tokens, agent tokens, account credentials, and bootstrap are rejected
  as authorize identities.

Still not implemented:

- OpenID Connect metadata (`/.well-known/openid-configuration`).
- MCP resource/audience binding.
- Bearer-like OAuth bridge for OAuth-only hosts.

## Goals

- Define `GET /oauth/authorize` for the OAuth2 authorization code flow.
- Use the existing opaque authorization code storage model:
  `oauth_authorization_codes`.
- Preserve existing `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, MCP,
  agent transport, and protected resource metadata semantics.
- Publish authorization-server discovery only for implemented capabilities.

## Non-goals

- No full username/password account system; the current login page accepts a
  first-party PAT and creates only a short-lived authorize-session cookie.
- No third-party cookie session or persistent DB-backed browser session.
- No changes to `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, or existing
  PAT behavior.
- No `/.well-known/openid-configuration`.
- No MCP `securitySchemes` changes, GPT Action configuration changes,
  `client_credentials` grant, device code flow, JWT, JWKS, or broad auth
  architecture refactor.

## Bearer-like OAuth bridge

A bearer-like OAuth bridge product flow is future work. It may let a user enter
a shared key on a WebCodex-hosted OAuth authorization page and receive an OAuth
access token for OAuth-only hosts.

The internal token substrate now supports an optional `shared_key_hash` on
authorization codes, access tokens, and refresh tokens so bridge-issued tokens
can reuse shared-key group isolation. Public bridge issuance endpoint/UI remains
future work; no shared-key-to-OAuth exchange route is exposed yet.

Bridge OAuth tokens currently keep OAuth current-session identity semantics;
`shared_key_hash` affects shared-key project/job visibility, not managed-user
identity.

That bridge would preserve host OAuth semantics. It would not make blank OAuth
client fields behave like no-auth, shared-key quick start, or a static Bearer
header.

## OAuth bridge implementation constraints

OAuth bridge public issuance remains future work. Its implementation must
preserve OAuth semantics: blank OAuth client fields are never bearer/no-auth
fallback, open anonymous mode must not be bridgeable into OAuth tokens, and
bridge-issued access tokens must still enforce OAuth scopes. A shared-key bridge
token must preserve shared-key group isolation semantics without storing
plaintext shared keys. Do not model shared-key bridge users as managed users
unless there is an explicit account binding. Do not use a fake `user_id` hack
such as `shared-key:<hash>` without documenting and testing the isolation model.
Agent transport endpoints must remain unavailable to OAuth2 tokens.

## Identity Source

The current authorize endpoint accepts first-party WebCodex identity through
two paths:

- `GET /oauth/authorize` with a first-party Bearer PAT whose `AuthContext`
  includes a concrete `user_id` may issue an authorization code directly.
- Browser login posts a PAT to `/oauth/authorize/login`, stores only an opaque
  short-lived authorize-session cookie, and then `/oauth/authorize/consent`
  revalidates client, redirect URI, scopes, and PKCE before issuing a code.
- OAuth2 access tokens cannot be used to obtain new authorization codes.
- Agent tokens, account credentials, and bootstrap are not valid authorize
  identities. Bootstrap is rejected because it has no `user_id` to bind an
  authorization code to.
- Requests with no authenticated user show the minimal login page or fail
  validation; they must not create a code.
- Approval is first-party and based on a registered client plus allowed scopes.

## Request Parameters

`GET /oauth/authorize` accepts query parameters:

| Parameter | Required | Contract |
| --- | --- | --- |
| `response_type` | Yes | Must be exactly `code`. |
| `client_id` | Yes | Must identify an existing, non-revoked OAuth client. |
| `redirect_uri` | Yes | Must exactly match one registered redirect URI for the client. |
| `scope` | No | Whitespace-separated OAuth scopes. See scope validation below. |
| `state` | No | Opaque client value. WebCodex does not interpret or trust it. The decoded state value is preserved semantically and URL-encoded again when redirecting. |
| `code_challenge` | Yes | Required for PKCE. |
| `code_challenge_method` | Yes | Must be exactly `S256`. |
| `resource` | No | Currently unsupported. If present, reject instead of silently ignoring. |

Unsupported duplicate or ambiguous parameters should be rejected with
`invalid_request`. The handler must not interpret or trust `state`; it
preserves the decoded value semantically and URL-encodes it again when adding
it to redirect responses.

## State Machine

The handler follows this order:

1. Require OAuth2 enabled. If disabled, return a direct error and create no
   code.
2. Parse query parameters enough to validate the OAuth request and client
   redirect boundary.
3. Validate `client_id` and load a non-revoked client.
4. Validate `redirect_uri` by exact registered match before redirecting
   anywhere.
5. Validate `response_type`, PKCE, `scope`, and unsupported `resource`.
6. Resolve first-party authorize identity:

   - Bearer PAT with a concrete `user_id` may directly issue a code.
   - Browser flow may use `/oauth/authorize/login` to create a short-lived
     authorize-session cookie, then `/oauth/authorize/consent` revalidates the
     request, client, scope, and PKCE before code issuance.
   - OAuth2 access tokens, agent tokens, account credentials, bootstrap, and
     unauthenticated requests must not issue codes.

7. On any validation failure, create no authorization code.
8. Generate one plaintext `wc_oac_*` code, store only its SHA-256 hash and
   metadata, and redirect once to the registered redirect URI.

## Client And Redirect URI Validation

`client_id` and `redirect_uri` establish whether the server can safely redirect
the user agent. Errors before this trust boundary must be direct responses,
not redirects to a request-controlled URI.

Direct 400 errors:

- Unknown `client_id`.
- Revoked client.
- Empty `client_id`.
- Missing `redirect_uri`.
- Empty `redirect_uri`.
- `redirect_uri` mismatch.

`redirect_uri` matching must be exact against the newline-separated
`oauth_clients.redirect_uris` list after applying the same trimming already
used by `OAuthClientRecord::redirect_uris_vec()`. No prefix, host-only, query
subset, wildcard, or normalization match is allowed.

## Scope Validation

Authorize-time scope validation must intersect client registration with the
global OAuth scope registry. The global OAuth scopes are the same non-agent,
non-admin scopes currently advertised by protected resource metadata:

- `runtime:read`
- `project:read`
- `project:write`
- `job:run`
- `account:manage`

Rules:

- If `scope` is absent or empty, default to
  `client.allowed_scopes` intersected with the global OAuth scopes.
- If the default result is empty, return `invalid_scope`.
- If `scope` is present, every requested scope must be in both
  `client.allowed_scopes` and the global OAuth scope set.
- `agent:*` scopes and `admin` are never valid for OAuth delegation.
- Unknown scopes are invalid.
- Duplicate scopes are ignored after validation.
- The stored and returned scope string is normalized in canonical global order.

Recommended Phase 2e-1 helper:

```rust
normalize_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError>
```

## PKCE Policy

`/oauth/authorize` must always require PKCE S256:

- `code_challenge` is required.
- `code_challenge_method` must be exactly `S256`.
- `plain` and omitted methods are rejected.
- This stricter browser authorization policy applies even if
  `config.oauth2.require_pkce` is set to `false`.

The existing config flag remains relevant for legacy or manually inserted
authorization codes exchanged at `/oauth/token`, but browser authorization
code issuance must not create non-PKCE codes.

## Authorization Code Issuance

On success, generate one plaintext authorization code:

```text
wc_oac_<256-bit-random-hex>
```

Only the SHA-256 hash is stored. The plaintext code is returned exactly once
in the success redirect.

The row inserted into `oauth_authorization_codes` must include:

| Field | Value |
| --- | --- |
| `id` | New UUID. |
| `code_hash` | SHA-256 of the plaintext `wc_oac_*` code. |
| `client_id` | Validated client ID. |
| `user_id` | Authenticated `AuthContext.user_id`. |
| `redirect_uri` | Exact validated redirect URI. |
| `scopes` | Normalized granted scope string. |
| `resource` | `None` while resource indicators are unsupported. |
| `code_challenge` | Request `code_challenge`. |
| `code_challenge_method` | `S256`. |
| `created_at` | Current Unix timestamp. |
| `expires_at` | `created_at + config.oauth2.authorization_code_ttl_secs`. |
| `used_at` | `None`. |
| `revoked_at` | `None`. |

## Success Redirect

After storing the code, redirect to the validated registered `redirect_uri`
with:

- `code=<plaintext wc_oac_*>`
- `state=<decoded state re-encoded for the redirect>` when `state` was
  provided

The authorize response must never include an access token or refresh token.
If the registered redirect URI already has a query string, append `code` and
`state` with `&` by using URL query-pair encoding rather than manual string
concatenation.

## Error Handling

Errors before `client_id` and `redirect_uri` are both validated are direct
400 responses. The response may be JSON or minimal HTML, but it must not
redirect to an untrusted URI.

Redirect errors are allowed only after the client and redirect URI have been
validated. The redirect should include:

- `error=<oauth error code>`
- `state=<decoded state re-encoded for the redirect>` when supplied

Examples that may redirect after client and redirect validation:

- Unsupported `response_type`.
- Missing or invalid PKCE.
- Invalid scope.
- Unsupported `resource`.

Do not expose internal error details to the redirect target. Internal failures
after redirect validation may use a generic `server_error`, but they must not
create a code.

## Security Invariants

- Authorization codes have a short TTL.
- Authorization codes are single-use; `/oauth/token` consumes them.
- `redirect_uri` is validated at authorize time and token exchange time.
- The PKCE challenge is stored at authorize time and the verifier is checked
  at token exchange.
- Invalid client and redirect mismatch never generate a code.
- Invalid scope, invalid PKCE, unsupported `response_type`, and unsupported
  `resource` never generate a code.
- `state` is opaque. WebCodex does not interpret or trust it. The decoded value
  is preserved semantically and URL-encoded again when redirecting.
- No access token or refresh token is ever returned from `/oauth/authorize`.
- Plaintext authorization codes are not stored.
- Plaintext authorization codes are not logged.
- Only hashes are persisted for authorization codes, access tokens, refresh
  tokens, and client secrets.

## Test Plan For Phase 2e-1

The implementation phase must add tests covering at least:

- `authorize_requires_authenticated_user`
- `authorize_rejects_unknown_client_without_redirect`
- `authorize_rejects_revoked_client_without_redirect`
- `authorize_rejects_missing_redirect_uri_without_redirect`
- `authorize_rejects_empty_redirect_uri_without_redirect`
- `authorize_rejects_redirect_uri_mismatch_without_redirect`
- `authorize_rejects_empty_client_id_without_redirect`
- `authorize_rejects_unsupported_response_type_with_redirect_after_client_validation`
- `authorize_rejects_empty_response_type_with_redirect_after_client_validation`
- `authorize_requires_pkce_s256`
- `authorize_rejects_plain_pkce_method`
- `authorize_rejects_missing_code_challenge`
- `authorize_rejects_empty_code_challenge`
- `authorize_rejects_empty_code_challenge_method`
- `authorize_rejects_invalid_scope`
- `authorize_rejects_resource_parameter`
- `authorize_redirect_error_appends_with_ampersand_when_redirect_uri_has_query`
- `authorize_redirect_error_preserves_decoded_state_semantics`
- `authorize_issues_code_and_redirects_with_state`
- `authorize_stores_only_code_hash`
- `authorize_code_contains_user_client_redirect_scope_pkce_metadata`
- `authorize_success_redirect_appends_with_ampersand_when_redirect_uri_has_query`
- `authorize_success_does_not_return_access_or_refresh_token`
- `authorize_success_code_can_be_exchanged_for_tokens`
- `authorize_accepts_user_pat_for_code_issuance`
- `authorize_rejects_oauth2_access_token_without_issuing_code`
- `authorize_rejects_agent_token_without_issuing_code`
- `authorize_rejects_account_credential_without_issuing_code`
- `authorize_does_not_issue_code_on_any_error`
- `oauth_authorization_server_metadata_is_public`
- `oauth_authorization_server_metadata_fields`
- `oauth_authorization_server_metadata_trims_trailing_issuer_slash`
- `oauth_authorization_server_metadata_disabled_returns_404`
- `openid_configuration_not_exposed`

Additional useful tests:

- Empty `scope` defaults to the normalized client/global OAuth intersection.
- Empty default scope intersection returns `invalid_scope`.
- `agent:*` and `admin` scopes are rejected.
- `resource` is rejected while unsupported.
- `state` round-trips as the same decoded opaque value on success and error
  redirects.

## Authorization Server Metadata

`GET /.well-known/oauth-authorization-server` is public and does not require a
Bearer token. When OAuth2 is disabled it returns 404 with
`{"error":"OAuth2 is not enabled"}`, matching protected resource metadata.

Authorization server metadata only advertises endpoints and capabilities that
are real. The metadata includes:

- `issuer`
- `authorization_endpoint`
- `token_endpoint`
- `revocation_endpoint`
- `response_types_supported = ["code"]`
- `grant_types_supported = ["authorization_code", "refresh_token"]`
- `code_challenge_methods_supported = ["S256"]`
- `token_endpoint_auth_methods_supported = ["client_secret_post", "none"]`
- `scopes_supported`

The endpoint URLs are derived from `config.oauth2.issuer` (fallback:
`http://localhost`) after trimming a trailing slash before appending
`/oauth/authorize`, `/oauth/token`, and `/oauth/revoke`.

Do not advertise unimplemented features:

- No `/.well-known/openid-configuration`.
- No `jwks_uri`, `userinfo_endpoint`, registration endpoint, device
  authorization endpoint, introspection endpoint, claims metadata, or ID token
  signing algorithms.
- No MCP resource/audience binding yet.

## Implementation Sequencing

Completed Phase 2e-1 order:

1. Add authorize request parsing and error types.
2. Add scope normalization helper and tests.
3. Mount `GET /oauth/authorize` and validate first-party Bearer PAT or
   authorize-session identity in the handler.
4. Implement client and exact redirect URI validation.
5. Implement post-redirect-trust validation for `response_type`, PKCE,
   `scope`, and `resource`.
6. Insert hashed authorization code metadata on validation success.
7. Redirect with plaintext `code` and decoded/re-encoded `state`.
8. Add authorization server metadata in the next metadata phase.
9. Add first-party client management and the minimal browser login/consent UX.

## Open Questions

- Should a later browser UI add richer consent semantics for third-party
  clients, or will WebCodex continue treating registered OAuth clients as
  first-party integrations?
- Should `resource` become required for MCP-bound tokens once audience
  enforcement exists?
- Should authorization-server metadata be served from the issuer root only, or
  also support RFC 8414 path variants if deployment prefixes are introduced?
