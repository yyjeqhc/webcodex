# OAuth Shared-Key Bridge Plan

Long-running design and sequencing for the shared-key OAuth bridge.
**Daily agent safety and editing rules live in [`AGENTS.md`](../../AGENTS.md).**
Do not re-litigate v1 constraints during ordinary implementation tasks unless
the user explicitly opens a design discussion.

Full threat model, endpoint notes, and open questions:
[`OAUTH2_BRIDGE_THREAT_MODEL.md`](../OAUTH2_BRIDGE_THREAT_MODEL.md).

Auth vocabulary: [`AUTH_MODEL.md`](../AUTH_MODEL.md),
[`OAUTH2_INTERNALS.md`](../OAUTH2_INTERNALS.md).

---

## 1. Standing v1 decisions

### Supported identity paths

- **Formal managed-user OAuth** remains fully supported.
- **Low-config OAuth onboarding** for MCP / AI platforms should use explicit
  shared-key OAuth principal support.
- **Synthetic managed users are rejected.** Do not create rows like
  `user_id = shared-key:<hash>` and do not auto-insert user rows for shared keys.

### Subject model contract

OAuth token subjects distinguish two kinds:

| kind | identifier | notes |
|---|---|---|
| `managed_user` | `user_id` | existing managed-account OAuth flow |
| `shared_key` | `shared_key_hash` | non-managed principal; no user row required |

Key invariants:

- `shared_key_hash` affects shared-key project and job visibility.
- `shared_key_hash` does **not** convert an `OAuth2Token` into
  `AuthKind::SharedKey`.
- A shared-key OAuth principal **must not** receive `account:manage`, `admin`,
  or agent-transport scopes by default.
- A shared-key OAuth principal **may** use `runtime`, `project`, and `job`
  scopes according to OAuth scope policy.
- OAuth2 tokens remain **rejected** on agent transport endpoints.
- Current-session identity remains OAuth identity semantics unless a future
  task explicitly changes it.

### Non-goals for v1 bridge

- No blank OAuth field fallback.
- No open anonymous bridge.
- No plaintext shared key storage.
- No public bridge endpoint until subject model and tests are stable
  (public flow is further gated; see threat model for flag semantics).

---

## 2. Approved implementation order

Do not skip ahead until earlier phases are stable and tested.

1. **Subject model schema refactor**
   - `oauth_authorization_codes`, `oauth_access_tokens`, and
     `oauth_refresh_tokens` support `managed_user` and `shared_key` subjects.
   - A shared-key subject must **not** require a managed-user lookup.

2. **OAuth2Verifier subject dispatch**
   - `managed_user` branch checks user existence and disabled state.
   - `shared_key` branch requires `shared_key_hash` and constructs a
     non-managed OAuth `AuthContext`.

3. **Scope policy enforcement**
   - Allow `runtime`, `project`, and `job` scopes as explicitly configured.
   - Reject `admin`, `account:manage`, and agent-transport scopes for
     shared-key OAuth tokens.

4. **Public authorize UI / route**
   - Implement **only after** subject model and tests are stable.
   - Must not include blank OAuth field fallback, open anonymous bridge, or
     plaintext shared key storage.

---

## 3. Agent validation when touching this area

Minimum for auth / OAuth / scope / DB subject-model work (see also `AGENTS.md`
validation matrix):

```
cargo fmt --check
cargo check --all-targets
cargo test --bin webcodex oauth -- --nocapture
cargo test --bin webcodex scope -- --nocapture
cargo test --bin webcodex metadata -- --nocapture
git diff --check
git status --short --branch
```
