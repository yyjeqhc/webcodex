use super::tool_spec;
use crate::tool_spec::ToolSpec;
use webcodex_core::skill_store::{
    SKILL_STORE_REPLAY_CLAIMED_RETENTION_SECS, SKILL_STORE_REPLAY_EFFECT_RETENTION_SECS,
};

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    let replay_retention = format!(
        "Same-key replay is durable but retention-bounded: pre-effect claimed intent is retained for {} hours, while prepared/completed effect recovery is retained for {} days. Reuse the same key for an uncertain outcome within that window. After the window, an old key is not proof of a prior effect; reconcile current state with skill_versions before deciding whether to issue a new mutation.",
        SKILL_STORE_REPLAY_CLAIMED_RETENTION_SECS / (60 * 60),
        SKILL_STORE_REPLAY_EFFECT_RETENTION_SECS / (24 * 60 * 60),
    );
    vec![
        tool_spec(
            "skill_load",
            "Load one uniquely named Skill by exact case-insensitive name for an authorized Project. Returns the selected descriptor plus bounded SKILL.md text and revision metadata in one read-only call. Ambiguous names fail closed; scripts and other Skill resources are never executed.",
        ),
        tool_spec(
            "run_skill_resource",
            "Execute one supported scripts/*.py or scripts/*.sh resource from a trusted Runner-configured live Skill or Runner-installed managed Skill without exposing or retransmitting its source through model context. Configured Skills are live resources: expected_definition_revision fences the selected SKILL.md definition, but resource bytes are read at execution and are not package-revision-pinned; skill_sha256 reports the bytes actually executed. Managed installed Skills additionally require expected_package_revision to fence the immutable package. WebPi selects the interpreter from the resource extension and callers supply only script arguments; project-content Skills are rejected.",
        ),
        tool_spec(
            "skill_list",
            "Fresh, bounded discovery of project-scoped Skills, configured live Skills on the Project's exact owning Runner, and active operator-installed immutable Skills. WebPi does not modify configured Skill roots, but supported scripts from that operator-trusted source may execute through run_skill_resource. Returns lightweight descriptors only; bodies require skill_read_file. trust and package_revision distinguish live configured content from managed installed revisions, and same names across sources remain independently selectable by opaque skill_id.",
        ),
        tool_spec(
            "skill_read_file",
            "Read one bounded UTF-8 text resource from a selected project, configured live Runner Skill, or active operator-installed immutable Skill. expected_definition_revision guards every source; expected_package_revision applies only to managed installed Skills. Configured live files are re-read from the Runner filesystem and carry no package revision. Scripts are text-only resources and are never executed by this tool.",
        ),
        tool_spec(
            "skill_versions",
            "List bounded immutable revisions and current active state for one operator-installed logical Skill on the exact Runner owning project. Requires Skill-management authority; returns metadata only.",
        ),
        tool_spec(
            "skill_install",
            format!("Install one verified project-relative ZIP artifact into the exact owning Runner's operator Skill store. Uses immutable package revisions, bounded archive validation, a caller idempotency key, and optional CAS-guarded activation. No URLs or native store paths are accepted. {replay_retention}"),
        ),
        tool_spec(
            "skill_activate",
            format!("Atomically switch one operator-installed logical Skill to an already installed immutable package revision using expected_state_revision CAS plus an idempotency key. Reactivating an older revision is rollback. {replay_retention}"),
        ),
        tool_spec(
            "skill_remove_revision",
            format!("Remove one inactive immutable operator Skill revision using expected_state_revision CAS plus an idempotency key. The active revision is never removable through this operation. {replay_retention}"),
        ),
    ]
}
