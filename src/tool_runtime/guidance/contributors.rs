//! Ordered built-in policy contributors. Keep core advice separate from the
//! selected client strategy; the workflow envelope and host catalog have their
//! own owners in startup_brief and ToolDefinition.

use super::super::tool_inputs::CodingGuidanceProfile;
use super::{GuidanceContributor, GuidanceTarget};

pub(super) static CORE_SAFETY: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Core,
    items: &[
        "Follow host safety and user/project scope/rules; carry authorized work to concrete, reviewable completion. Ask only for missing requirements/authority; guidance grants no authority.",
        "Verify Project/changes/nested rules, plus branch/HEAD for Git work. Recovery/compaction/exact Session resume is continuation: reuse still-current Git/read/validation/Job facts; revalidate changed snapshots/HEAD/worktree/instructions.",
        "Preserve unrelated work; push/publish/deploy/restart need explicit action/target. If a user answer/Job/validation/result is not a dependency, continue independent work; wait only on real dependencies.",
    ],
};

pub(super) static CORE_IMPLEMENTATION: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Core,
    items: &[
        "For inspection, files/data/artifacts, diagnostics, or coding: inspect → act or produce → review → task-fit validation → deliver. Coding maps cross-layer changes end to end; choose Git/Cargo/commit only when needed. Coding: compiler/schema/exhaustiveness failures expose gaps; avoid speculative redesign.",
    ],
};

pub(super) static CORE_VALIDATION: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Core,
    items: &[
        "Validation failure is evidence, not queue cleanliness. Fix dependent blockers; continue otherwise. Reuse assertion_name; outcome_unknown fails closed. After Rust stabilizes, format once. Development validation may overlap independent work; covered-source edits make it stale for final evidence.",
        "For closeout evidence, freeze source covered by final validation. Continue read-only review/docs/external inspection; if covered source must change, invalidate that evidence and rerun the appropriate final validation.",
    ],
};

pub(super) static CORE_JOBS: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Core,
    items: &[
        "Keep one execution/Job. Generated calls carry follow_up_kind. mechanically_followable is an exact continuation: copy args unchanged after Host inputSchema validation. fallback_recovery is recovery/detail/dependency, never auto-followed. Pending Job continuation is fallback_recovery; passive Job attention may surface.",
        "observe_jobs is for logs/details/recovery; list_jobs is identity recovery; finish ready work before one wait_for_job_readiness join. any may unblock a branch; all requires every blocker. Deadline recomputes work/set; never mechanically refill. Current turn; no automatic next turn. Never retry/redispatch.",
    ],
};

pub(super) static DIRECT: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Strategy(CodingGuidanceProfile::Direct),
    items: &[
        "General loop: inspect → act/produce → review → task-fit validation → deliver. Non-Git work can use list_project_files/read_files; Git, Cargo checks, and commits are task-dependent.",
        "Text source edits: read_files → edit_project_files with its read_revision fence; use exact edits for unique text and replace_range for deterministic whole-line rewrites. Review with review_changes when Git review is useful.",
        "Use artifact import/export or transfer_project_artifact for binary files; do not rename or transform binary payloads through text edits. Use run_script/Python for computation or non-source transforms, then independently verify generated/report outputs.",
        "Never use scripts to bypass edit_project_files revision fences, rollback, or sensitive-path policy.",
        "Coalesce known work: read_files(items), search_project_texts(queries), search_file_context for search→source inspection, cargo_check(packages), and one edit_project_files batch. Keep result-dependent operations sequential; avoid ritual model turns.",
        "Simple observation: direct primitive. Batch predetermined independent observations; adaptive follow-ups stay sequential across model calls.",
        "Known target: bounded targeted reads. Broad discovery: small files/count search then targeted reads. Avoid ritual turns.",
    ],
};

pub(super) static HOST_BATCHING: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Strategy(CodingGuidanceProfile::HostCodeMode),
    items: &[
        "Host-native Code Mode is model guidance only. It grants no WebCodex capability/authority, changes no effects/retry/idempotency, and does not require WebCodex nested Code Mode. Host support is not verified by WebCodex; emit compact evidence.",
        "Known same-kind inputs: prefer native batches such as read_files(items), search_project_texts(queries), cargo_check(packages), or one edit_project_files batch; do not Promise.all same-kind micro-calls. Never per-Job waits, Promise.race, or automatic redispatch.",
        "Independent cross-tool read-only observations: native batches first; then Promise.allSettled for partial evidence or Promise.all for all-or-nothing. Prefer search_file_context for search→read; keep mechanically determined dependent chains in one Host cell. Never use observe_jobs heartbeat polling.",
    ],
};

pub(super) static HOST_CONTINUATION: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Strategy(CodingGuidanceProfile::HostCodeMode),
    items: &[
        "Run to quiescence: finish ready independent calls; follow only follow_up_kind=mechanically_followable with generated arguments unchanged after current input-schema validation. Continue ready work in the same cell. Job terminal does not imply mechanically_followable; fallback_recovery never auto-runs.",
        "Return to the model for semantic choice, ambiguous result, new user decision, authority/permission change, outcome_unknown, competing recovery, unresolved mutation intent or effect uncertainty. Child-call completion alone is not a boundary. Cell return is not turn completion.",
        "Exact stale-source reread may be mechanical. reread_required=true or direct_retry_safe=false stops effectful replay; recovery is not mutation retry authority. Keep full ToolResults in the Host cell; preserve exact pending continuations, observation_ref/read_revision, and failure/recovery fields.",
        "execution_state=pending: retain exact Job identity/continuation as fallback. Run currently-ready independent calls. Ready work exhausted: wait_for_job_readiness join barrier on the entire exact blocked set. any when one terminal Job can unlock a useful branch; all only when every blocked dependency is required.",
        "Choose wait_secs as the largest safe value from the remaining Host activation budget, up to the 45s maximum; do not prefer fixed 10/15/20s slices. Deadline: recompute ready work and the blocked set; do not mechanically repeat the same-set wait. Final validation: freeze covered source; edits invalidate evidence.",
    ],
};

#[cfg(feature = "experimental-code-mode")]
pub(super) static CODE_MODE: GuidanceContributor = GuidanceContributor {
    target: GuidanceTarget::Strategy(CodingGuidanceProfile::CodeMode),
    items: &[
        "For one simple observation use a direct primitive; do not wrap it in Code Mode. If one bounded search will immediately inspect its matches, prefer direct search_file_context. Native commands and structured tools are first-class; choose the simplest sufficient primitive. Narrow broad discovery before targeted reads.",
        "Prefer read-only execute_code_mode for multi-step related search/read observations, cross-file/module investigation, or synthesis of independent observations when it reduces outer model round trips. Three or more related observations is a soft heuristic, never a correctness rule.",
        "Keep adaptive follow-up inside one cell: search, inspect result, dependent read, inspect, further search, compact final projection. Dependent calls remain sequential inside the cell; they need not cross model turns.",
        "Plan each cell as a small dependency DAG: use Promise.all for independent observations and keep true dependencies sequential. Prefer a native batch shape (read_files items, search_project_texts queries) over same-kind calls. Use JavaScript for branching/cross-tool composition, not avoidable micro-calls.",
        "Keep raw child ToolResults inside the cell. Filter, extract, cross-reference and synthesize search results, file bodies and diff chunks before text(...). Emit only compact structured evidence needed for the next model decision; no fixed JSON shape is required.",
        "Avoid raw-result dumping: do not batch calls then text(results). Project proactively before hitting the bounded outer-output limit. If nested signatures or result fields are not retained, exact read_tool_manifest on the selected Code Mode entry returns its bounded callable contract.",
        "Canonical mutation is the default edit path; structured validation is the default validation path. Consider effectful Code Mode only to reduce outer round trips for multiple related validations; mutating Code Mode only when adaptive read -> one guarded edit benefits.",
        "This profile grants no capability or nested admission. All children retain canonical Project/Session authority, permission, risk, approval, effects, idempotency, validation evidence, Job continuation, retry and effect-certainty semantics.",
    ],
};
