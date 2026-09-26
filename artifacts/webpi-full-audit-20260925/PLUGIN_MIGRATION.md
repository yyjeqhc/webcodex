# WebPi Custom GPT -> Plugin migration plan

Status: prepared; migration execution is blocked on first-release PR/deploy and ChatGPT-side migration UI/admin permissions.

## Current platform contract (2026-09-26)

- Custom GPT instructions migrate into a plugin skill; knowledge files migrate into reference files.
- GPT custom actions do not migrate automatically. Required integrations must be rebuilt as supported apps/actions or a custom MCP server.
- The migrated plugin starts private. Test before sharing or publishing.
- The original GPT becomes read-only after migration and remains usable only until its retirement date.
- Existing GPT conversations do not migrate.

Official references reviewed on 2026-09-26:
- OpenAI Help Center: Custom GPT retirement and migration FAQ
- ChatGPT Learn: Moving your custom GPT workflows to plugins

## WebPi target architecture

Keep one execution/security backend:

ChatGPT plugin skill
  -> custom MCP / WebPi gateway
  -> existing WebPi auth + scopes
  -> WebPi Server
  -> Runner / Project / Pi bridge / artifacts

Do not create a second durable state store, shared administrator token, or bypass path around WebPi scopes. Plugin references and UI affordances grant no authority by themselves.

## Migration sequence

1. Finish first-release review and PR for the WebPi runtime candidate.
2. Deploy the exact reviewed revision through WebPi service control only.
3. Verify public origin, source alignment, auth, MCP, file/edit, Git, Job recovery and plugin/Pi bridge smoke against the deployed revision.
4. In ChatGPT, migrate the latest published WebPi Agent GPT to a private plugin.
5. Review the migrated skill instructions and reference files; keep WebPi as the sole local-project execution entry.
6. Rebuild the GPT custom Action integration as a custom MCP/app pointing at the deployed WebPi public origin. Do not copy the Action bearer credential into skill text or reference files.
7. Run the acceptance matrix below while the plugin remains private.
8. Only after acceptance, review sharing/publishing permissions and make a separate visibility decision.

## Acceptance matrix

- Skill selection: explicit @/plugin selection chooses WebPi workflow when requested.
- Automatic selection: a normal local-project task selects the WebPi skill when appropriate without forcing it on unrelated tasks.
- Read-only discovery: runtime status, projects, project bootstrap, bounded search/read.
- Safe editing: read_revision-fenced edit, conflict rejection, diff review.
- Execution: structured process and validation Jobs; continuation tokens survive handoff.
- Git: status/diff/log and explicit-path commit behavior; no automatic push.
- Recovery: reconnect/reconciliation/lost-job semantics stay fail-closed.
- Pi bridge: inventory -> describe -> call; no implicit extension approval.
- Authority: missing service/plugin/communication scopes remain blocked; MCP/plugin metadata never widens authority.
- Security: no secrets in skill/reference files, logs, prompts or plugin metadata.
- Output quality: familiar prompts plus one harder end-to-end coding task match or improve the custom GPT workflow.
- Performance: compare tool-call count/handoff/cleanup against the preserved baseline.

## Current blockers

- PR publication: upstream repository is read-only for the current GitHub account and no fork exists. Creating a fork is an external side effect that requires explicit approval.
- Deployment: current WebPi credential lacks service:restart and service:deploy; deployment_preflight is blocked.
- Candidate provenance: the source worktree was already heavily dirty before this audit and no start-of-session file snapshot exists. Do not claim a clean goal-only PR from the whole worktree.
- Disposable Docker image smoke: local build did not start because Docker Hub OAuth returned EOF and required base images were not cached; remote release-readiness CI must provide that image proof.
- ChatGPT migration: requires the account/workspace migration UI and any needed plugin/custom-MCP permissions. This is a user/admin UI boundary when reached.
