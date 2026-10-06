//! Typed compatibility reduction shared by runtime diagnostics and internal UIs.
//!
//! Build/protocol facts are canonical typed data. JSON projections may describe
//! them, but internal consumers must not parse those projections back into
//! business semantics.

use crate::runner_protocol::RunnerView;
use webcodex_core::desktop_runtime_contract::{
    build_alignment, runner_protocol_compatibility, BuildAlignment, ProtocolCompatibility,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FleetProtocolStatus {
    NoRunners,
    Compatible,
    Incompatible,
    Unknown,
}

impl FleetProtocolStatus {
    pub(super) const fn status_name(self) -> &'static str {
        match self {
            Self::NoRunners => "no_runners",
            Self::Compatible => "compatible",
            Self::Incompatible => "incompatible",
            Self::Unknown => "unknown",
        }
    }

    pub(super) const fn protocol_name(self) -> &'static str {
        match self {
            Self::NoRunners | Self::Unknown => "unknown",
            Self::Compatible => "compatible",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceAlignmentStatus {
    NoRunners,
    Aligned,
    Different,
    Unknown,
}

impl SourceAlignmentStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::NoRunners => "no_runners",
            Self::Aligned => "aligned",
            Self::Different => "different",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RunnerCompatibilityFacts {
    pub(super) client_id: String,
    pub(super) build_version: Option<String>,
    pub(super) build_git_commit: Option<String>,
    pub(super) build_git_dirty: Option<bool>,
    pub(super) version_matches_server: Option<bool>,
    pub(super) git_commit_matches_server: Option<bool>,
    pub(super) source_matches_server: Option<bool>,
    pub(super) source_alignment: SourceAlignmentStatus,
    pub(super) protocol_compatibility: ProtocolCompatibility,
    pub(super) build_alignment: BuildAlignment,
}

impl RunnerCompatibilityFacts {
    pub(super) const fn status_name(&self) -> &'static str {
        protocol_compatibility_name(self.protocol_compatibility)
    }
}

#[derive(Debug, Clone)]
pub(super) struct FleetCompatibilityFacts {
    pub(super) status: FleetProtocolStatus,
    pub(super) build_alignment: BuildAlignment,
    pub(super) source_alignment: SourceAlignmentStatus,
    pub(super) mixed_builds_present: bool,
    pub(super) runners: Vec<RunnerCompatibilityFacts>,
}

pub(super) const fn protocol_compatibility_name(value: ProtocolCompatibility) -> &'static str {
    match value {
        ProtocolCompatibility::Compatible => "compatible",
        ProtocolCompatibility::Incompatible => "incompatible",
        ProtocolCompatibility::Unknown => "unknown",
    }
}

pub(super) const fn build_alignment_name(value: BuildAlignment) -> &'static str {
    match value {
        BuildAlignment::Exact => "exact",
        BuildAlignment::Unknown => "unknown",
        BuildAlignment::DifferentCommit => "different_commit",
        BuildAlignment::DifferentVersion => "different_version",
        BuildAlignment::Dirty => "dirty",
    }
}

const fn build_alignment_rank(value: BuildAlignment) -> usize {
    match value {
        BuildAlignment::Exact => 0,
        BuildAlignment::Unknown => 1,
        BuildAlignment::DifferentCommit => 2,
        BuildAlignment::DifferentVersion => 3,
        BuildAlignment::Dirty => 4,
    }
}

const fn build_alignment_for_rank(rank: usize) -> BuildAlignment {
    match rank {
        0 => BuildAlignment::Exact,
        1 => BuildAlignment::Unknown,
        2 => BuildAlignment::DifferentCommit,
        3 => BuildAlignment::DifferentVersion,
        _ => BuildAlignment::Dirty,
    }
}

pub(super) fn current_compatibility_facts(clients: &[RunnerView]) -> FleetCompatibilityFacts {
    let build = crate::build_info::runtime_build_info();
    compatibility_facts_against(
        clients,
        env!("CARGO_PKG_VERSION"),
        build.git_commit,
        build.git_dirty,
    )
}

pub(super) fn compatibility_facts_against(
    clients: &[RunnerView],
    server_version: &str,
    server_git_commit: Option<&str>,
    server_git_dirty: Option<bool>,
) -> FleetCompatibilityFacts {
    let mut status = if clients.is_empty() {
        FleetProtocolStatus::NoRunners
    } else {
        FleetProtocolStatus::Compatible
    };
    let mut source_alignment = if clients.is_empty() {
        SourceAlignmentStatus::NoRunners
    } else {
        SourceAlignmentStatus::Aligned
    };
    let mut alignment_rank = usize::from(clients.is_empty());
    let mut mixed_builds_present = false;
    let mut runners = Vec::with_capacity(clients.len());

    for client in clients {
        let build_version = client
            .build
            .as_ref()
            .and_then(|build| build.version.clone());
        let build_git_commit = client
            .build
            .as_ref()
            .and_then(|build| build.git_commit.clone());
        let build_git_dirty = client.build.as_ref().and_then(|build| build.git_dirty);
        let version_matches_server = build_version
            .as_deref()
            .map(|version| version == server_version);
        let git_commit_matches_server = match (build_git_commit.as_deref(), server_git_commit) {
            (Some(runner), Some(server)) => Some(runner == server),
            _ => None,
        };
        let source_matches_server =
            match (git_commit_matches_server, build_git_dirty, server_git_dirty) {
                (Some(false), _, _) => Some(false),
                (Some(true), Some(false), Some(false)) => Some(true),
                (Some(true), Some(true), _) | (Some(true), _, Some(true)) => Some(false),
                _ => None,
            };
        let runner_source_alignment = match source_matches_server {
            Some(true) => SourceAlignmentStatus::Aligned,
            Some(false) => SourceAlignmentStatus::Different,
            None => SourceAlignmentStatus::Unknown,
        };
        source_alignment = match (source_alignment, runner_source_alignment) {
            (_, SourceAlignmentStatus::Different) => SourceAlignmentStatus::Different,
            (SourceAlignmentStatus::Aligned, SourceAlignmentStatus::Unknown) => {
                SourceAlignmentStatus::Unknown
            }
            (current, _) => current,
        };

        let protocol_compatibility =
            runner_protocol_compatibility(client.runner_protocol_generation.get());
        status = match (status, protocol_compatibility) {
            (_, ProtocolCompatibility::Incompatible) => FleetProtocolStatus::Incompatible,
            (FleetProtocolStatus::Compatible, ProtocolCompatibility::Unknown) => {
                FleetProtocolStatus::Unknown
            }
            (current, _) => current,
        };
        let runner_build_alignment = build_alignment(
            build_version.as_deref(),
            build_git_commit.as_deref(),
            build_git_dirty,
            Some(server_version),
            server_git_commit,
            server_git_dirty,
        );
        alignment_rank = alignment_rank.max(build_alignment_rank(runner_build_alignment));
        mixed_builds_present |=
            version_matches_server == Some(false) || source_matches_server == Some(false);
        runners.push(RunnerCompatibilityFacts {
            client_id: client.client_id.clone(),
            build_version,
            build_git_commit,
            build_git_dirty,
            version_matches_server,
            git_commit_matches_server,
            source_matches_server,
            source_alignment: runner_source_alignment,
            protocol_compatibility,
            build_alignment: runner_build_alignment,
        });
    }

    FleetCompatibilityFacts {
        status,
        build_alignment: build_alignment_for_rank(alignment_rank),
        source_alignment,
        mixed_builds_present,
        runners,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner_protocol::{RunnerBuildInfo, RunnerView, RUNNER_PROTOCOL_GENERATION_V2};

    fn runner(client_id: &str, version: Option<&str>, commit: Option<&str>) -> RunnerView {
        RunnerView {
            client_id: client_id.to_string(),
            runner_instance_id: format!("{client_id}-instance"),
            display_name: None,
            owner: None,
            hostname: None,
            status: "online".to_string(),
            host_context: None,
            connected: true,
            last_seen: 0,
            capabilities: Default::default(),
            computer_session_availability: None,
            coding_agent_providers: None,
            pending_requests: 0,
            projects: Vec::new(),
            project_inventory: None,
            runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
            transport: "websocket".to_string(),
            policy: None,
            registered_at: 0,
            connected_at: 0,
            disconnected_at: None,
            process_started_at: None,
            build: Some(RunnerBuildInfo {
                version: version.map(str::to_string),
                git_commit: commit.map(str::to_string),
                git_dirty: Some(false),
                built_at: None,
                target: None,
                architecture: None,
            }),
            job_concurrency_limit: None,
        }
    }

    #[test]
    fn fleet_reduction_preserves_protocol_and_build_precedence() {
        let facts = compatibility_facts_against(
            &[
                runner("aligned", Some("1.0"), Some("aaaaaaaa")),
                runner("different", Some("0.9"), Some("bbbbbbbb")),
            ],
            "1.0",
            Some("aaaaaaaa"),
            Some(false),
        );
        assert_eq!(facts.status, FleetProtocolStatus::Compatible);
        assert_eq!(facts.build_alignment, BuildAlignment::DifferentVersion);
        assert_eq!(facts.source_alignment, SourceAlignmentStatus::Different);
        assert!(facts.mixed_builds_present);
        assert_eq!(facts.runners[0].status_name(), "compatible");
        assert_eq!(
            facts.runners[1].build_alignment,
            BuildAlignment::DifferentVersion
        );
    }

    #[test]
    fn empty_fleet_keeps_no_runner_status_and_unknown_compatibility() {
        let facts = compatibility_facts_against(&[], "1.0", None, None);
        assert_eq!(facts.status.status_name(), "no_runners");
        assert_eq!(facts.status.protocol_name(), "unknown");
        assert_eq!(facts.build_alignment, BuildAlignment::Unknown);
        assert_eq!(facts.source_alignment, SourceAlignmentStatus::NoRunners);
    }
}
