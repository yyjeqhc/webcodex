//! Allowlisted behavior facts only; never copy request or result strings.
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

const KEYS: [&str; 7] = [
    "project.instructions",
    "webcodex.workflow",
    "workflow.resume",
    "jobs.attention",
    "skills.catalog",
    "plugins.catalog",
    "memory.bootstrap",
];
const COUNT_CAP: usize = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct InvocationFacts {
    pub record_present: bool,
    pub ack_present: bool,
    pub ack_count: usize,
    pub ack_ref_present: bool,
    pub reply_present: bool,
    pub resolve_present: bool,
    pub context_present: bool,
    pub context_requested_count: usize,
    pub context_known: BTreeMap<&'static str, bool>,
    pub context_unknown_count: usize,
    pub control_present: bool,
    pub control_before: Option<&'static str>,
    pub control_after_success: Option<&'static str>,
}

impl InvocationFacts {
    pub(crate) fn from_metadata(
        metadata: &super::super::kernel::ToolInvocationMetadata,
        record_present: bool,
    ) -> Self {
        // Reuse the same closed-key classifier; only context keys are inspected,
        // and no request Value survives this projection.
        let mut facts = Self::from_arguments(
            &serde_json::json!({"_wc": {"context": metadata.context_request}}),
        );
        facts.record_present = record_present;
        facts.ack_present = !metadata.ack_session_message_ids.is_empty();
        facts.ack_count = metadata.ack_session_message_ids.len().min(COUNT_CAP);
        facts.ack_ref_present = metadata.ack_ref.is_some();
        facts.reply_present = metadata.window_reply.is_some();
        facts.resolve_present = metadata.session_message_resolution.is_some();
        facts.context_present = !metadata.context_request.is_empty();
        facts.control_present = metadata.control.is_some();
        facts.control_before = metadata
            .control
            .as_ref()
            .and_then(|c| c.before.as_ref())
            .map(|c| c.kind());
        facts.control_after_success = metadata
            .control
            .as_ref()
            .and_then(|c| c.after_success.as_ref())
            .map(|c| c.kind());
        facts
    }

    pub(crate) fn from_arguments(arguments: &Value) -> Self {
        let wc = &arguments["_wc"];
        let context = wc["context"].as_array();
        let mut facts = Self {
            record_present: wc.get("record").is_some(),
            ack_present: wc.get("ack").is_some(),
            ack_count: wc["ack"].as_array().map_or(0, |v| v.len().min(COUNT_CAP)),
            ack_ref_present: wc.get("ack_ref").is_some(),
            reply_present: wc.get("reply").is_some(),
            resolve_present: wc.get("resolve").is_some(),
            context_present: wc.get("context").is_some(),
            context_requested_count: context.map_or(0, |v| v.len().min(COUNT_CAP)),
            control_present: wc.get("control").is_some(),
            control_before: mutation_kind(
                &wc["control"]["before"],
                &[
                    "goal_progress",
                    "wake_consume",
                    "attempt_heartbeat",
                    "session_context_update",
                ],
            ),
            control_after_success: mutation_kind(
                &wc["control"]["after_success"],
                &["todo_completion", "goal_completion", "session_close"],
            ),
            ..Default::default()
        };
        for key in KEYS {
            facts.context_known.insert(
                key,
                context.is_some_and(|v| v.iter().any(|v| v.as_str().map(str::trim) == Some(key))),
            );
        }
        facts.context_unknown_count = context.map_or(0, |v| {
            v.iter()
                .filter(|v| !v.as_str().is_some_and(|s| KEYS.contains(&s.trim())))
                .count()
                .min(COUNT_CAP)
        });
        facts
    }
}

fn mutation_kind(value: &Value, known: &[&'static str]) -> Option<&'static str> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    known.iter().copied().find(|key| object.contains_key(*key))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BootstrapFacts {
    instructions_available: Option<bool>,
    instructions_content_included: Option<bool>,
    instructions_truncated: Option<bool>,
    instruction_source_count: Option<usize>,
    instruction_observation_status: Option<&'static str>,
    workflow_available: Option<bool>,
    skills_returned_count: Option<u64>,
    skills_truncated: Option<bool>,
    skills_available: Option<bool>,
    plugins_returned_count: Option<u64>,
    plugins_truncated: Option<bool>,
    plugins_available: Option<bool>,
    workspace_status: Option<&'static str>,
    semantic_supported: Option<bool>,
    semantic_available: Option<bool>,
    semantic_status: Option<&'static str>,
}

fn label(value: &Value, labels: &[&'static str]) -> Option<&'static str> {
    labels
        .iter()
        .copied()
        .find(|label| value.as_str() == Some(*label))
}
fn available(value: &Value) -> Option<bool> {
    match value["status"].as_str() {
        Some("available") => Some(true),
        Some("unavailable") => Some(false),
        _ => None,
    }
}
impl BootstrapFacts {
    pub(crate) fn from_output(output: &Value) -> Self {
        let brief = output.get("startup_brief").unwrap_or(output);
        let materials = output
            .pointer("/context_projection/materials")
            .and_then(Value::as_array);
        let material = |key| {
            materials
                .and_then(|v| v.iter().find(|v| v["key"] == key))
                .unwrap_or(&Value::Null)
        };
        let instructions = material("project.instructions");
        let projection = &instructions["projection"];
        let skills = &brief["extensions"]["skills"];
        let plugins = &brief["extensions"]["plugins"];
        Self {
            instructions_available: available(instructions),
            instructions_content_included: projection["content_included"].as_bool(),
            instructions_truncated: projection["truncated"].as_bool(),
            instruction_source_count: projection["sources"]
                .as_array()
                .or_else(|| brief["instructions"]["sources"].as_array())
                .map(|v| v.len().min(COUNT_CAP)),
            instruction_observation_status: label(
                &brief["instructions"]["status"],
                &["loaded", "changed", "reused", "not_found", "unavailable"],
            ),
            workflow_available: available(material("webcodex.workflow")),
            skills_returned_count: skills["returned_count"]
                .as_u64()
                .map(|n| n.min(COUNT_CAP as u64)),
            skills_truncated: skills["truncated"].as_bool(),
            skills_available: available(skills),
            plugins_returned_count: plugins["returned_count"]
                .as_u64()
                .map(|n| n.min(COUNT_CAP as u64)),
            plugins_truncated: plugins["truncated"].as_bool(),
            plugins_available: available(plugins),
            workspace_status: label(
                &brief["workspace"]["git"]["status"],
                &[
                    "clean",
                    "dirty",
                    "conflicted",
                    "unavailable",
                    "not_applicable",
                ],
            ),
            semantic_supported: brief["semantic_navigation"]["supported"].as_bool(),
            semantic_available: brief["semantic_navigation"]["available"].as_bool(),
            semantic_status: label(
                &brief["semantic_navigation"]["status"],
                &[
                    "running",
                    "available",
                    "initializing",
                    "crashed",
                    "unavailable",
                    "not_applicable",
                    "agent_unavailable",
                    "agent_capability_unavailable",
                    "probe_timeout",
                    "probe_failed",
                ],
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InstructionReadFacts {
    agents_md: bool,
    claude_md: bool,
}
impl InstructionReadFacts {
    pub(crate) fn from_arguments(tool: &str, arguments: &Value) -> Option<Self> {
        if tool != "read_files" {
            return None;
        }
        let reads = |name| {
            arguments["items"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item["path"]
                        .as_str()
                        .is_some_and(|path| path.rsplit(['/', '\\']).next() == Some(name))
                })
            })
        };
        Some(Self {
            agents_md: reads("AGENTS.md"),
            claude_md: reads("CLAUDE.md"),
        })
    }
}
