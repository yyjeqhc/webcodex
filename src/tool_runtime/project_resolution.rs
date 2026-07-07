use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::projects::ProjectConfig;
use crate::shell_protocol::{ShellAgentProjectSummary, ShellClientView};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub(crate) struct ProjectResolverCandidate {
    pub(crate) id: String,
    pub(crate) client_id: String,
    pub(crate) agent_project_id: String,
    pub(crate) name: Option<String>,
    pub(crate) path: String,
    pub(crate) allow_patch: bool,
    pub(crate) connected: bool,
    pub(crate) status: String,
    pub(crate) last_seen: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedProject {
    pub(crate) input: String,
    pub(crate) resolved_id: String,
    pub(crate) config: ProjectConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectResolverErrorKind {
    UnknownProject,
    AmbiguousProject,
}

impl ProjectResolverErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::UnknownProject => "unknown_project",
            Self::AmbiguousProject => "ambiguous_project",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectResolverError {
    pub(crate) kind: ProjectResolverErrorKind,
    pub(crate) project: String,
    pub(crate) candidates: Vec<ProjectResolverCandidate>,
}

impl ProjectResolverError {
    fn candidate_payload(candidate: &ProjectResolverCandidate) -> Value {
        json!({
            "id": candidate.id,
            "client_id": candidate.client_id,
            "agent_project_id": candidate.agent_project_id,
            "name": candidate.name,
            "path": candidate.path,
            "connected": candidate.connected,
            "status": candidate.status,
            "last_seen": candidate.last_seen,
        })
    }

    fn to_output(&self) -> Value {
        let candidates: Vec<Value> = self
            .candidates
            .iter()
            .map(Self::candidate_payload)
            .collect();
        json!({
            "error_kind": self.kind.as_str(),
            "project": self.project,
            "hint": "Use a full runtime project id in the form agent:<client_id>:<project_id> from list_projects.",
            "candidates": candidates,
        })
    }

    pub(crate) fn to_message(&self) -> String {
        let mut message = format!(
            "{} '{}'. Use a full runtime project id in the form agent:<client_id>:<project_id> from list_projects.",
            match self.kind {
                ProjectResolverErrorKind::UnknownProject => "unknown_project",
                ProjectResolverErrorKind::AmbiguousProject => "ambiguous_project",
            },
            self.project
        );
        if self.candidates.is_empty() {
            return message;
        }
        let candidate_summary = self
            .candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{} [{}] {} ({})",
                    candidate.id, candidate.client_id, candidate.path, candidate.status
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        message.push_str(" Candidates: ");
        message.push_str(&candidate_summary);
        message
    }

    pub(crate) fn into_tool_result(self) -> ToolResult {
        ToolResult::err_with_output(self.to_message(), self.to_output())
    }
}

impl std::fmt::Display for ProjectResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_message())
    }
}

impl From<ProjectResolverError> for String {
    fn from(value: ProjectResolverError) -> Self {
        value.to_message()
    }
}

pub(crate) fn agent_project_runtime_id(client_id: &str, project_id: &str) -> String {
    format!("agent:{}:{}", client_id, project_id)
}

impl ToolRuntime {
    fn project_candidate_from_view(
        client: &ShellClientView,
        project: &ShellAgentProjectSummary,
    ) -> ProjectResolverCandidate {
        ProjectResolverCandidate {
            id: agent_project_runtime_id(&client.client_id, &project.id),
            client_id: client.client_id.clone(),
            agent_project_id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
            allow_patch: project.allow_patch,
            connected: client.connected,
            status: client.status.clone(),
            last_seen: client.last_seen,
        }
    }

    fn project_config_from_candidate(candidate: &ProjectResolverCandidate) -> ProjectConfig {
        ProjectConfig {
            path: candidate.path.clone(),
            client_id: candidate.client_id.clone(),
            allow_patch: candidate.allow_patch,
        }
    }

    fn sort_resolver_candidates(candidates: &mut [ProjectResolverCandidate]) {
        candidates.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.status.cmp(&b.status))
                .then_with(|| b.last_seen.cmp(&a.last_seen))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    async fn agent_project_candidates_for_auth(
        &self,
        auth: Option<&AuthContext>,
    ) -> Vec<ProjectResolverCandidate> {
        let mut candidates = Vec::new();
        for client in self.shell_clients.list_clients_for_auth(auth).await {
            for project in client.projects.iter().filter(|project| !project.disabled) {
                candidates.push(Self::project_candidate_from_view(&client, project));
            }
        }
        Self::sort_resolver_candidates(&mut candidates);
        candidates
    }

    pub(crate) async fn resolve_project_input_for_auth(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Result<ResolvedProject, ProjectResolverError> {
        let raw = project.trim();
        if raw.is_empty() {
            return Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: project.to_string(),
                candidates: self.agent_project_candidates_for_auth(auth).await,
            });
        }

        let all_candidates = self.agent_project_candidates_for_auth(auth).await;

        if raw.starts_with("agent:") {
            let Some(rest) = raw.strip_prefix("agent:") else {
                unreachable!();
            };
            let Some((client_id, agent_project_id)) = rest.split_once(':') else {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::UnknownProject,
                    project: raw.to_string(),
                    candidates: all_candidates,
                });
            };
            if client_id.trim().is_empty() || agent_project_id.trim().is_empty() {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::UnknownProject,
                    project: raw.to_string(),
                    candidates: all_candidates,
                });
            }
            if let Some(candidate) = all_candidates.iter().find(|candidate| candidate.id == raw) {
                return Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(candidate),
                });
            }
            let mut same_client: Vec<ProjectResolverCandidate> = all_candidates
                .iter()
                .filter(|candidate| candidate.client_id == client_id)
                .cloned()
                .collect();
            Self::sort_resolver_candidates(&mut same_client);
            return Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: raw.to_string(),
                candidates: same_client,
            });
        }

        if let Some((client_id, agent_project_id)) = raw.split_once(':') {
            if !client_id.trim().is_empty() && !agent_project_id.trim().is_empty() {
                let mut matches: Vec<ProjectResolverCandidate> = all_candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.client_id == client_id
                            && candidate.agent_project_id == agent_project_id
                    })
                    .cloned()
                    .collect();
                Self::sort_resolver_candidates(&mut matches);
                match matches.len() {
                    1 => {
                        let candidate = matches.remove(0);
                        return Ok(ResolvedProject {
                            input: project.to_string(),
                            resolved_id: candidate.id.clone(),
                            config: Self::project_config_from_candidate(&candidate),
                        });
                    }
                    0 => {
                        let mut same_client: Vec<ProjectResolverCandidate> = all_candidates
                            .iter()
                            .filter(|candidate| candidate.client_id == client_id)
                            .cloned()
                            .collect();
                        Self::sort_resolver_candidates(&mut same_client);
                        return Err(ProjectResolverError {
                            kind: ProjectResolverErrorKind::UnknownProject,
                            project: raw.to_string(),
                            candidates: same_client,
                        });
                    }
                    _ => {
                        return Err(ProjectResolverError {
                            kind: ProjectResolverErrorKind::AmbiguousProject,
                            project: raw.to_string(),
                            candidates: matches,
                        });
                    }
                }
            }
        }

        let mut short_id_matches: Vec<ProjectResolverCandidate> = all_candidates
            .iter()
            .filter(|candidate| candidate.agent_project_id == raw)
            .cloned()
            .collect();
        Self::sort_resolver_candidates(&mut short_id_matches);
        match short_id_matches.len() {
            1 => {
                let candidate = short_id_matches.remove(0);
                return Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(&candidate),
                });
            }
            n if n > 1 => {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::AmbiguousProject,
                    project: raw.to_string(),
                    candidates: short_id_matches,
                });
            }
            _ => {}
        }

        let mut name_matches: Vec<ProjectResolverCandidate> = all_candidates
            .iter()
            .filter(|candidate| candidate.name.as_deref() == Some(raw))
            .cloned()
            .collect();
        Self::sort_resolver_candidates(&mut name_matches);
        match name_matches.len() {
            1 => {
                let candidate = name_matches.remove(0);
                Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(&candidate),
                })
            }
            n if n > 1 => Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::AmbiguousProject,
                project: raw.to_string(),
                candidates: name_matches,
            }),
            _ => Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: raw.to_string(),
                candidates: all_candidates,
            }),
        }
    }

    pub(crate) async fn resolve_project_input(
        &self,
        project: &str,
    ) -> Result<ResolvedProject, ProjectResolverError> {
        self.resolve_project_input_for_auth(project, None).await
    }

    pub(crate) async fn resolve_project(
        &self,
        project: &str,
    ) -> Result<ProjectConfig, ProjectResolverError> {
        self.resolve_project_input(project)
            .await
            .map(|resolved| resolved.config)
    }

    pub(crate) async fn resolve_project_for_auth(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Result<ProjectConfig, ProjectResolverError> {
        self.resolve_project_input_for_auth(project, auth)
            .await
            .map(|resolved| resolved.config)
    }
}
