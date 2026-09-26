# WebPi prompt recipes

These prompts are designed for a WebPi-connected coding agent. They assume the persistent rules in [WEBPI_GPT_INSTRUCTIONS.md](WEBPI_GPT_INSTRUCTIONS.md): exact project authority, observe-before-mutate, structured tools, reconciliation after uncertain outcomes, focused TDD, broader validation, and explicit side-effect boundaries.

Use the shortest prompt that states the real outcome. Add concrete acceptance criteria, known constraints, and examples when available; do not repeat the entire system contract in every request.

## Prompt design principles

A good WebPi task prompt answers five things:

1. **Outcome** — what should be true when the task is finished?
2. **Context** — which project/component/user flow matters?
3. **Constraints** — compatibility, security, performance, rollout, or “do not change” boundaries.
4. **Evidence** — what tests, observations, or external sources should prove the result?
5. **Finish authority** — may the agent commit, deploy, push, migrate, install, or only prepare the change?

Prefer acceptance criteria over prescribing a tool sequence. Let the agent choose the narrowest WebPi tools, but state consequential boundaries explicitly.

## Design basis

These recipes intentionally follow the same division that Pi itself recommends: keep reusable task invocations as Prompt Templates, keep larger on-demand procedures as Skills, and reserve executable Extensions/Packages for capabilities that truly need code. Pi documents Skills as progressive disclosure (name/description always visible, full instructions loaded only when needed) and warns that skills/packages may execute or instruct arbitrary host actions, so third-party content must be reviewed before use.

Primary references used when maintaining these prompts:

- Pi Skills: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md
- Pi Prompt Templates: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/prompt-templates.md
- Pi Packages / package security: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md
- Pi Extensions: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md
- OpenAI prompt engineering: https://developers.openai.com/api/docs/guides/prompt-engineering
- Anthropic prompt engineering overview: https://docs.anthropic.com/en/docs/build-with-claude/prompt-engineering/overview
- Google Gemini prompting strategies: https://ai.google.dev/gemini-api/docs/prompting-strategies

WebPi adds its own execution rules on top: exact project/session authority, structured tools, outcome reconciliation, fresh validation after the final change, and explicit user authority for consequential effects.

---

## 1. Repository research / architecture map

Use when you want understanding, not changes.

```text
Research this WebPi project/component: <topic>.

Goal:
- Answer: <questions>.
- Map the relevant architecture, entry points, data/authority flow, tests, and operational boundaries.

Method:
- Start read-only. Inspect runtime/Git state and the smallest relevant source set.
- Follow definitions/references rather than guessing from filenames.
- If current external facts matter, use primary/official sources and distinguish source facts from inference.
- Do not modify files or invoke mutation/control tools.

Output:
- Findings with source/file evidence.
- Risks or unknowns.
- The smallest safe next step if implementation is warranted.
```

## 2. Feature implementation

```text
Implement this feature in WebPi: <feature>.

User outcome:
- <observable user behavior>.

Acceptance criteria:
- <criterion 1>
- <criterion 2>
- <criterion 3>

Constraints:
- Preserve <compatibility/API/security boundary>.
- Do not change <out-of-scope area>.
- Prefer the existing WebPi authority/tool model over parallel mechanisms.

Execution:
- Inspect runtime, workspace, relevant implementation and tests first.
- Write or strengthen a focused failing test when practical.
- Implement the smallest coherent vertical slice.
- Run targeted tests, then relevant broader regression.
- Review the final diff for compatibility, security, error recovery, and user-facing naming.

Finish authority:
- Do not commit/deploy/push unless I explicitly authorize it.

Report: changed, validated, remaining risk, user action.
```

## 3. Bug fix

```text
Fix this WebPi bug: <symptom>.

Expected behavior:
- <expected>.

Observed behavior / reproduction:
- <steps or evidence>.

Requirements:
- Reproduce or establish the failing condition before changing code.
- Identify the concrete root cause; do not patch only the visible symptom.
- Add a regression test that fails against the unfixed behavior when practical.
- Make the narrowest fix that preserves existing authority/compatibility boundaries.
- Prove targeted green, then run the relevant broader suite.
- If a mutation has an uncertain outcome, inspect state before retrying.

Do not commit/deploy/push unless explicitly authorized.
```

## 4. Security / permission review

```text
Review this WebPi surface for security and authority correctness: <surface/change>.

Focus on:
- authentication and scope separation;
- project/path/sensitive-file boundaries;
- observation vs control permissions;
- credential/token leakage;
- replay, stale identity, TOCTOU, outcome-unknown retries;
- plugin/Pi discovery vs executable-code approval;
- public-route exposure and error information leakage;
- rollback and audit evidence.

Use code/tests/runtime evidence, not security slogans.
Do not modify files during the review.

Return findings ordered by severity, each with:
- concrete evidence/location;
- exploit/failure condition;
- impact;
- minimal remediation;
- missing test, if any.
If no material finding exists, say what was checked and what remains unverified.
```

## 5. Code review

```text
Review the current WebPi change against this requirement: <requirement>.

Check two axes separately:
1. Specification: does the change actually satisfy the requested behavior and compatibility constraints?
2. Quality: correctness, security, failure recovery, maintainability, performance, and test strength.

Inspect the real diff and relevant surrounding code/tests.
Do not make edits.
Do not report style preferences as defects.

For each finding provide severity, file/line evidence, why it matters, and a concrete fix.
Call out missing validation or assumptions explicitly.
```

## 6. Release / production deployment

```text
Prepare WebPi for production deployment of: <change>.

Required gates before any production effect:
- workspace/source revision is understood and intended;
- relevant targeted and full tests pass after the last change;
- formatting/build gates pass;
- secret/security scan appropriate to the change passes;
- Server/Runner build identity and exact hashes are recorded;
- rollback artifact/plan is available;
- public/auth/security smoke plan is defined.

Do not deploy until I explicitly authorize the production effect.
When authorized, use an atomic or rollback-capable deployment path, verify runtime readiness and source alignment, then run loopback and public smoke where applicable.
Do not equate process start or HTTP 200 with successful deployment.
```

## 7. Incident / broken production

```text
Investigate this WebPi production incident: <symptom>.

Priority:
1. Preserve evidence and current service state.
2. Determine blast radius and whether execution/auth boundaries are affected.
3. Restore service by the smallest reversible action.
4. Only then pursue the durable root-cause fix.

Rules:
- Start read-only unless immediate containment is explicitly authorized.
- Never retry an uncertain mutation without reconciliation.
- Do not delete logs/backups or rotate credentials unless justified and authorized.
- Distinguish transport/CDN failures from WebPi application failures.

Report timeline/evidence, root cause or leading hypotheses, recovery performed, validation, and follow-up.
```

## 8. Plugin / Pi ecosystem task

```text
Add or evaluate this WebPi Pi/Plugin capability: <capability/package>.

First discover existing capabilities/resources/packages. Prefer a maintained existing solution when it fits.
Before executable code is trusted:
- inspect source/dependencies/install/lifecycle scripts and permissions;
- identify project-local vs global scope;
- obtain exact candidate id + SHA-256;
- request explicit authority for package mutation/trust/approval;
- approve only the reviewed fingerprint;
- reload, list, describe, then invoke the exact tool;
- validate behavior and revoke/rollback on failure.

Never infer tool arguments or treat discovery as execution.
```

## 9. UX / onboarding improvement

```text
Improve this WebPi user flow: <flow>.

Optimize for a first-time user who does not know internal crate/protocol names.
Check:
- first command / next action is obvious;
- errors explain what failed, whether state may have changed, and the safest recovery;
- user-facing naming says WebPi; internal compatibility identifiers appear only when genuinely needed;
- secret handling is explicit;
- safe/read-only defaults are easiest;
- docs and CLI/help agree with actual runtime behavior.

Back user-facing claims with tests or runtime evidence.
```

## 10. Autonomous bounded implementation

Use this when you want WebPi to carry a substantial task to completion without stopping for routine decisions.

```text
Research and complete this WebPi task end-to-end: <task>.

You may make ordinary project-local code/test/doc changes and run required validation without asking about routine implementation choices. Continue until the requested outcome is genuinely verified.

Stop for explicit user authority before: credential entry/rotation, admin/trust elevation, package install with executable lifecycle scripts, extension approval, destructive cleanup, broad overwrite, production deployment, push/release, or any other consequential external effect not already authorized.

Keep changes bounded to the task, preserve unrelated work, reconcile uncertain outcomes before retry, and report only verified completion.
```

## Anti-patterns

Avoid prompts such as:

- “Make it better” with no user outcome.
- Huge repeated policy blocks that hide the actual task.
- Prescribing every tool call when only the result matters.
- “Do whatever is necessary” without defining consequential authority.
- “Keep trying until tests pass” (this encourages weakening tests or blind retries).
- Asking the model to infer secrets, IDs, tool schemas, or production state.

A strong prompt is specific about **outcome and boundaries**, then lets WebPi observe reality and choose the narrowest correct execution path.
