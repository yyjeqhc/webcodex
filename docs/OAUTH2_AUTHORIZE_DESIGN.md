# OAuth2 Authorization Endpoint Design

## Status

Phase 2e-1b validation-only handler. `/oauth/authorize` is mounted behind
`AuthMiddleware`, but authorization code issuance is not implemented yet.

This document is the implementation contract for Phase 2e-1. It describes the
request contract, state machine, security invariants, storage shape, and test
plan for the future authorization endpoint. Phase 2e-1b wires the Phase 2e-1a
helpers into an authenticated HTTP handler and route, while intentionally
leaving authorization code issuance for a later phase.

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

- `GET /oauth/authorize` mounted behind `AuthMiddleware`.
- OAuth2 enable gate and authenticated `AuthContext.user_id` requirement.
- Client lookup and exact registered `redirect_uri` validation.
- Direct 400 errors before `client_id` and `redirect_uri` are trusted.
- Redirect errors only after client and redirect URI validation.
- `response_type=code`, PKCE S256, normalized scope, and unsupported
  `resource` validation.
- Validation success returns `501 Not Implemented` with no redirect and no
  authorization code issuance.

Still not implemented:

- Authorization code generation or insertion into
  `oauth_authorization_codes`.
- Authorization server metadata
  (`/.well-known/oauth-authorization-server` or
  `/.well-known/openid-configuration`).

## Goals

- Define `GET /oauth/authorize` for the OAuth2 authorization code flow.
- Use the existing opaque authorization code storage model:
  `oauth_authorization_codes`.
- Preserve existing `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, MCP,
  agent transport, and protected resource metadata semantics.
- Keep authorization-server discovery gated until the authorize endpoint
  exists and is tested.

## Non-goals

- No `/oauth/authorize` route in Phase 2e-0. Phase 2e-1b mounts the route for
  validation only.
- No HTML login page, username/password flow, consent page, or third-party
  cookie session.
- No authorization code issuance in Phase 2e-0 through Phase 2e-1b.
- No changes to `/oauth/token`, `/oauth/revoke`, `OAuth2Verifier`, or
  `AuthMiddleware` behavior.
- No `/.well-known/oauth-authorization-server` or
  `/.well-known/openid-configuration`.
- No MCP `securitySchemes` changes, GPT Action configuration changes,
  `client_credentials` grant, device code flow, route-level scope enforcement,
  JWT, JWKS, or broad auth architecture refactor.

## Identity Source

Phase 2e-1 should implement an authenticated first-party authorization
endpoint:

- `GET /oauth/authorize` is protected by the existing `AuthMiddleware`.
- The caller must already hold a WebCodex user PAT or equivalent bearer auth
  accepted by regular HTTP AuthMiddleware paths.
- The authorizing user is `AuthContext.user_id`.
- Requests with no authenticated user must fail and must not create a code.
- No independent browser username/password login page is introduced.
- No third-party cookie session is introduced.
- No consent UI is introduced in the first implementation.
- Approval is first-party and based on a registered client plus allowed scopes.

If `AuthMiddleware` later proves unsuitable for browser-based sign-in because
it only supports bearer-token authentication, the compatible alternative is to
add a first-party WebCodex browser session layer and map that session to the
same `AuthContext` shape. That is explicitly a later phase; Phase 2e-1 should
use AuthMiddleware.

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
| `resource` | No | Not supported in Phase 2e-1. If present, reject instead of silently ignoring. |

Unsupported duplicate or ambiguous parameters should be rejected with
`invalid_request`. The handler must not interpret or trust `state`; it
preserves the decoded value semantically and URL-encodes it again when adding
it to redirect responses.

## State Machine

The future handler should follow this order:

1. Require OAuth2 enabled. If disabled, return a direct error and create no
   code.
2. Require authenticated WebCodex user via `AuthMiddleware`.
3. Parse query parameters.
4. Validate `client_id` and load a non-revoked client.
5. Validate `redirect_uri` by exact string match against
   `oauth_clients.redirect_uris`.
6. After the redirect target is trusted, validate `response_type`, PKCE,
   `scope`, and unsupported `resource`.
7. On any validation failure, create no authorization code.
8. In Phase 2e-1b, return `501 Not Implemented` on validation success and
   create no code.
9. In the later issuance phase, generate one plaintext `wc_oac_*` code, store
   only its SHA-256 hash and metadata, and redirect once to the registered
   redirect URI.

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

Phase 2e-1b does not perform a success redirect. After all validation passes,
it returns `501 Not Implemented` with:

```json
{
  "error": "authorization code issuance is not implemented yet"
}
```

No authorization code is generated or stored.

In the later issuance phase, after storing the code, redirect to the validated
registered `redirect_uri` with:

- `code=<plaintext wc_oac_*>`
- `state=<decoded state re-encoded for the redirect>` when `state` was
  provided

The authorize response must never include an access token or refresh token.

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
- `authorize_validation_success_returns_501_without_issuing_code`
- `authorize_issues_code_and_redirects_with_state`
- `authorize_stores_only_code_hash`
- `authorize_code_contains_user_client_redirect_scope_pkce_metadata`
- `authorize_does_not_issue_code_on_any_error`
- `authorization_server_metadata_not_exposed_before_authorize`

Additional useful tests:

- Empty `scope` defaults to the normalized client/global OAuth intersection.
- Empty default scope intersection returns `invalid_scope`.
- `agent:*` and `admin` scopes are rejected.
- `resource` is rejected while unsupported.
- `state` round-trips as the same decoded opaque value on redirect errors.

## Metadata Gating

`/.well-known/oauth-authorization-server` must remain unexposed until
`/oauth/authorize` issues authorization codes and the issuance tests pass.

After `/oauth/authorize` exists, authorization server metadata must only
advertise endpoints and capabilities that are real. The minimum metadata
must include:

- `authorization_endpoint`
- `token_endpoint`
- `revocation_endpoint`
- `code_challenge_methods_supported`
- `response_types_supported`
- `grant_types_supported`
- `token_endpoint_auth_methods_supported`
- `scopes_supported`

Do not advertise authorization server metadata while the browser authorization
flow is validation-only.

## Implementation Sequencing

Recommended Phase 2e-1 order:

1. Add authorize request parsing and error types.
2. Add scope normalization helper and tests.
3. Mount `GET /oauth/authorize` behind `AuthMiddleware`.
4. Implement client and exact redirect URI validation.
5. Implement post-redirect-trust validation for `response_type`, PKCE,
   `scope`, and `resource`.
6. Return `501 Not Implemented` on validation success without issuing a code.
7. Insert hashed authorization code metadata in the later issuance phase.
8. Redirect with plaintext `code` and decoded/re-encoded `state`.
9. Add authorization server metadata only after issuance endpoint tests pass.

## Open Questions

- Should a later browser UI introduce explicit consent for third-party
  clients, or will WebCodex continue treating all registered OAuth clients as
  first-party integrations?
- Should `resource` become required for MCP-bound tokens once audience
  enforcement exists?
- Should authorization-server metadata be served from the issuer root only, or
  also support RFC 8414 path variants if deployment prefixes are introduced?
