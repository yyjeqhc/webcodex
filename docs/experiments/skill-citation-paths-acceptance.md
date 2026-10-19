# Skill citation path acceptance

[简体中文](skill-citation-paths-acceptance.zh-CN.md)

This report records the bounded T03 follow-up for
[Issue #1196](https://github.com/yyjeqhc/webcodex/issues/1196), related to
[the original knowledge/Skill acceptance](knowledge-skill-acceptance-results.md).
That historical report includes correct audit facts with package-relative
`SKILL.md` / `reference.md` citations that failed the Project-relative rubric.
Those results and the rubric remain unchanged.

## Change and provenance

Successful `load_skill` and `read_skill_file` responses now distinguish
`skill_path` (normalized package-relative read input) from `project_path`
(authoritative Project-relative citation). The latter comes from the resolved
package directory, which need not equal the Skill metadata name. Runner-configured
and managed Skills omit `project_path`. The ambiguous output `path` is removed;
the input `read_skill_file.path` and Runner wire `path` remain unchanged.
The current contract is described in [the coding workflow](../CODING_WORKFLOW.md).

The implementation milestone is `4f839f25356f9f95614999b4d7d14175bd7e6c71`.
The test deployment used clean, aligned dogfood Server/Runner source
`62671f015a77718a35dd0144d808d0751e5fafb8`, a local integration of that milestone
and Plugin recovery source `b7ff9e85d1abc36666d9e78483e0cdffc100026a`.
This preserved the preceding test deployment's Plugin behavior.
Before publication, Plugin PR #1193 landed in upstream `9a2ba651`; merging that
target into this feature produced `0a47fddb`. Its complete tree was verified
identical to the measured integration tree before adding this report.
The PR's remaining diff contains only the Skill follow-up and its documentation;
there is no dependency on an unmerged Plugin PR.

## Deterministic and integration checks

- Tool contracts: 316 passed; one optional cost probe remained ignored.
- Skill runtime: 16 passed, including different folder/metadata names, normalized
  nested references, line paging, revisions, Project scope, source conflicts,
  sensitive paths and traversal denial. Configured/managed reads retain a
  package-relative locator without inventing Project paths.
- Audit privacy: one focused check passed; source text remains omitted while
  citation metadata is retained. MCP description and serialized-size checks
  each passed without increasing budgets.
- Knowledge fixture/evidence/Host-driver Python modules: 59 checks passed on the
  implementation branch; 61 passed in the integration with Plugin recovery.
  Historical captures remain readable. Mixed old/new locators, wrong scopes,
  mismatched paths, hashes and revisions remain rejected.
- Clean dogfood build passed. The existing six-case scripted MCP/Runner matrix
  and Host handoff passed with Apps disabled, including both update cases and
  assertions on actual canonical Skill output. These checks have **zero model
  samples**. The Project Knowledge smoke also passed its binding, source-guard,
  recovery and provider-removal checks.
- Rust formatting, local Markdown links and diff checks passed.

## Real ChatGPT Web sample

One fresh conversation was executed through the existing test connection on
2026-10-19, with user-reported `6-sol` / extreme reasoning and Apps disabled.
The frozen prompt requested current audit facts through the Project Skill and
its referenced resources, with Project-relative citations. It supplied no answer,
answer-bearing file path, tool sequence or operator checkpoint. No corrective follow-up was
needed. The independent fixture/oracle and the prompt were stored separately.

The private capture and Workflow Session audit independently establish six
canonical calls: one `work_on_project`, one `load_skill`, three
`read_tool_manifest`, and one `read_skill_file`. All succeeded; adapter rejection
count was zero. The two actual Skill reads returned:

| Read | `skill_path` | `project_path` | Read lines |
| --- | --- | --- | --- |
| Definition | `SKILL.md` | `.agents/skills/receipt-audit/SKILL.md` | 1–9 |
| Reference | `reference.md` | `.agents/skills/receipt-audit/reference.md` | 1–4 |

Both reads belonged to the exact fixture Project and the same verified definition
revision, with source SHA-256 and ranges checked against frozen bytes. Neither
response carried the old output `path` alias. The answer established audit order
from definition line 8 and marker/batch from reference lines 3–4, using the full
Project-relative paths. All six strict checks passed: fixture/Session/Project
binding, fixture integrity, observed source content, answer facts/citations,
required Skill resource use, and coordinator order (no update required here).
There were no checkout edits, shell/process/script calls or Jobs.

Only the test runtime was replaced. Stable service identities and the existing
native tunnel processes remained unchanged. After verification the test service
returned to ordinary metadata-only capture, keeping the tested binaries and
its non-expiring test grant. Rollback binaries and a private database snapshot
were retained; no credential or protected tunnel record was reset.

## Limits and next observation

The result is one attributable **pass**, not a production success rate, speedup,
or controlled before/after comparison. The prompt explicitly required Skill
resources and current Project citations; it does not establish adoption under
free tool choice. Backend model ID, tokens, cache hits and model-only timing are
unknown. The user performed the conversation and forwarded its reply; the capture
verifies the WebCodex side, not browser origin or backend model identity.
Three manifest reads remain an observation, not proof of a discovery
defect or a reason to broaden this fix.

The next bounded observation is canonical citation adoption in the existing
free-choice procedure case. It should retain its own frozen prompt and count,
without changing this result or starting retrieval/Memory work. Raw captures,
model reply, Session/Runner identities, private paths and credentials remain
outside Git; this report contains only sanitized facts and verification summaries.
