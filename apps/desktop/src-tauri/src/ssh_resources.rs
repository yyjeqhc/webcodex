//! Closed Desktop facade over the canonical Server `ssh_resource` gateway.
//! Only a short-lived observation is held here, never a second SSH registry.
//! Credentials, targets and canonical gateway bindings never enter public state.
use crate::models::StoredRuntime;
use crate::webcodex::settings::SettingsTarget;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use webcodex_core::ssh_resource::{
    normalize_ssh_resource_default_cwd, normalize_ssh_resource_target,
    validate_response_for_request, validate_ssh_resource_name, SshResourceInventoryEntry,
    SshResourceRequest, SshResourceResponse, SshResourceSource, SSH_RESOURCE_RESPONSE_MAX_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SshResourceError {
    InsufficientScope,
    AuthorizationUnavailable,
    RunnerReplaced,
    SshResourceRegistryStale,
    SshResourceOutcomeUnknown,
    SshResourceRegistryUnavailable,
    SshResourceInvalid,
    SshResourceStaticReadOnly,
    SshResourceStaticConflict,
    SshResourceNameConflict,
    SshResourceNotFound,
}

impl SshResourceError {
    fn from_code(code: Option<&str>, mutation: bool) -> Self {
        match code {
            Some("insufficient_scope") => Self::InsufficientScope,
            Some("runner_replaced") => Self::RunnerReplaced,
            Some("ssh_resource_registry_stale" | "ssh_resource_binding_required") => {
                Self::SshResourceRegistryStale
            }
            Some("ssh_resource_outcome_unknown") => Self::SshResourceOutcomeUnknown,
            Some("ssh_resource_static_read_only") => Self::SshResourceStaticReadOnly,
            Some("ssh_resource_static_conflict") => Self::SshResourceStaticConflict,
            Some("ssh_resource_name_conflict") => Self::SshResourceNameConflict,
            Some("ssh_resource_not_found") => Self::SshResourceNotFound,
            Some("ssh_resource_invalid") => Self::SshResourceInvalid,
            Some("ssh_resource_registry_unavailable") => Self::SshResourceRegistryUnavailable,
            _ => Self::unconfirmed(mutation),
        }
    }
    fn unconfirmed(mutation: bool) -> Self {
        if mutation {
            Self::SshResourceOutcomeUnknown
        } else {
            Self::SshResourceRegistryUnavailable
        }
    }
}

#[derive(Clone, Serialize)]
pub struct SshResourcesSnapshot {
    pub runner: String,
    pub available: bool,
    pub can_authorize: bool,
    pub observation_id: Option<String>,
    pub resources: Vec<SshResourceInventoryEntry>,
    pub error_kind: Option<SshResourceError>,
}

#[derive(Serialize)]
pub struct SshMutationResult {
    pub success: bool,
    pub error_kind: Option<SshResourceError>,
    pub restart_required: bool,
    pub inventory: SshResourcesSnapshot,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshRegisterRequest {
    pub expected: SettingsTarget,
    pub observation_id: String,
    pub name: String,
    pub target: String,
    pub default_cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshRemoveRequest {
    pub expected: SettingsTarget,
    pub observation_id: String,
    pub name: String,
}

pub(crate) enum Mutation {
    Register {
        name: String,
        target: String,
        default_cwd: Option<String>,
    },
    Remove {
        name: String,
    },
}

impl Mutation {
    fn name(&self) -> &str {
        match self {
            Self::Register { name, .. } | Self::Remove { name } => name,
        }
    }
    fn arguments(self, binding: String) -> Result<Value, SshResourceError> {
        validate_ssh_resource_name(self.name())
            .map_err(|_| SshResourceError::SshResourceInvalid)?;
        Ok(match self {
            Self::Register {
                name,
                target,
                default_cwd,
            } => {
                let mut arguments = json!({
                    "action":"register", "binding":binding, "name":name,
                    "target":normalize_ssh_resource_target(&target).map_err(|_| SshResourceError::SshResourceInvalid)?,
                });
                if let Some(cwd) = normalize_ssh_resource_default_cwd(default_cwd.as_deref())
                    .map_err(|_| SshResourceError::SshResourceInvalid)?
                {
                    arguments["default_cwd"] = json!(cwd);
                }
                arguments
            }
            Self::Remove { name } => json!({"action":"remove", "binding":binding, "name":name}),
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Identity {
    server_url: String,
    runner: String,
    config: Option<PathBuf>,
    credential_file: Option<PathBuf>,
}
impl Identity {
    fn new(runtime: &StoredRuntime) -> Self {
        Self {
            server_url: runtime.server_url.clone(),
            runner: runtime.runner_client_id.clone().unwrap_or_default(),
            config: runtime.runner_config.clone(),
            credential_file: runtime.user_token_file.clone(),
        }
    }
}

struct Observation {
    identity: Identity,
    id: String,
    binding: String,
    resources: Vec<SshResourceInventoryEntry>,
}

#[derive(Default)]
pub(crate) struct SshResourcesManager {
    observation: Option<Observation>,
}

pub(crate) trait Gateway {
    fn call(
        &self,
        runtime: &StoredRuntime,
        arguments: Value,
    ) -> impl Future<Output = Result<Value, SshResourceError>> + Send;
}

impl SshResourcesManager {
    pub fn invalidate(&mut self) {
        self.observation = None;
    }

    pub async fn list(
        &mut self,
        runtime: &StoredRuntime,
        gateway: &impl Gateway,
    ) -> SshResourcesSnapshot {
        self.invalidate();
        let identity = Identity::new(runtime);
        let result = if identity.runner.is_empty() || identity.runner.len() > 128 {
            Err(SshResourceError::SshResourceRegistryUnavailable)
        } else {
            gateway
                .call(runtime, json!({"action":"list", "runner":identity.runner}))
                .await
                .and_then(|output| decode_list(output, &identity.runner))
        };
        match result {
            Ok(output) => {
                let id = uuid::Uuid::new_v4().to_string();
                let snapshot = SshResourcesSnapshot {
                    runner: identity.runner.clone(),
                    available: true,
                    can_authorize: crate::runner_capability_grant::can_authorize(runtime),
                    observation_id: Some(id.clone()),
                    resources: output.resources.clone(),
                    error_kind: None,
                };
                self.observation = Some(Observation {
                    identity,
                    id,
                    binding: output.binding,
                    resources: output.resources,
                });
                snapshot
            }
            Err(error) => SshResourcesSnapshot {
                runner: identity.runner,
                available: false,
                can_authorize: crate::runner_capability_grant::can_authorize(runtime),
                observation_id: None,
                resources: Vec::new(),
                error_kind: Some(error),
            },
        }
    }

    pub async fn mutate(
        &mut self,
        runtime: &StoredRuntime,
        observation_id: &str,
        mutation: Mutation,
        gateway: &impl Gateway,
    ) -> SshMutationResult {
        // Single-use locally even when validation/transport fails. The Server
        // independently checks caller, exact Runner instance and registry revision.
        let observed = self.observation.take();
        let result = async {
            let observed = observed
                .filter(|observed| observed.id == observation_id)
                .ok_or(SshResourceError::SshResourceRegistryStale)?;
            if observed.identity != Identity::new(runtime) {
                return Err(SshResourceError::RunnerReplaced);
            }
            let name = mutation.name().to_owned();
            if matches!(mutation, Mutation::Remove { .. }) {
                match observed
                    .resources
                    .iter()
                    .find(|resource| resource.name == name)
                {
                    Some(resource) if resource.source == SshResourceSource::Static => {
                        return Err(SshResourceError::SshResourceStaticReadOnly)
                    }
                    Some(_) => {}
                    None => return Err(SshResourceError::SshResourceNotFound),
                }
            }
            let remove = matches!(mutation, Mutation::Remove { .. });
            let arguments = mutation.arguments(observed.binding)?;
            let output = gateway.call(runtime, arguments).await?;
            let response: MutationOutput = serde_json::from_value(output)
                .map_err(|_| SshResourceError::SshResourceOutcomeUnknown)?;
            if response.resource != name
                || !response.persisted
                || response.restart_required
                    != if remove {
                        response.active
                    } else {
                        !response.active
                    }
            {
                return Err(SshResourceError::SshResourceOutcomeUnknown);
            }
            Ok(response.restart_required)
        }
        .await;
        // Observe after *every* attempt. Never repeat a register/remove request,
        // including timeout, outcome_unknown, stale revision and replacement.
        let inventory = self.list(runtime, gateway).await;
        SshMutationResult {
            success: result.is_ok(),
            error_kind: result.as_ref().err().copied(),
            restart_required: result.unwrap_or(false),
            inventory,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListOutput {
    runner: String,
    binding: String,
    resources: Vec<SshResourceInventoryEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MutationOutput {
    resource: String,
    persisted: bool,
    active: bool,
    restart_required: bool,
}

fn decode_list(output: Value, runner: &str) -> Result<ListOutput, SshResourceError> {
    let output: ListOutput = serde_json::from_value(output)
        .map_err(|_| SshResourceError::SshResourceRegistryUnavailable)?;
    if output.runner != runner
        || output.binding.len() > 128
        || !output.binding.starts_with("wc_sbind_")
    {
        return Err(SshResourceError::RunnerReplaced);
    }
    // Reuse the canonical inventory validator. The revision is a validation-only
    // placeholder; Desktop never sees or invents the binding's actual revision.
    validate_response_for_request(
        &SshResourceRequest::List,
        &SshResourceResponse::List {
            revision: 0,
            resources: output.resources.clone(),
        },
    )
    .map_err(|_| SshResourceError::SshResourceRegistryUnavailable)?;
    Ok(output)
}

pub(crate) struct HttpGateway;
impl Gateway for HttpGateway {
    async fn call(
        &self,
        runtime: &StoredRuntime,
        arguments: Value,
    ) -> Result<Value, SshResourceError> {
        let mutation = arguments.get("action").and_then(Value::as_str) != Some("list");
        let unavailable = || SshResourceError::SshResourceRegistryUnavailable;
        let uncertain = || SshResourceError::unconfirmed(mutation);
        let mut url = url::Url::parse(&runtime.server_url).map_err(|_| unavailable())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(unavailable());
        }
        url.set_path("/api/tools/call");
        url.set_query(None);
        url.set_fragment(None);
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            // Canonical gateway waits up to 65s for an exact Runner response.
            .timeout(Duration::from_secs(75));
        if matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ) {
            builder = builder.no_proxy();
        }
        let path = runtime.user_token_file.as_ref().ok_or_else(unavailable)?;
        let mut token = String::new();
        tokio::fs::File::open(path)
            .await
            .map_err(|_| unavailable())?
            .take(16_385)
            .read_to_string(&mut token)
            .await
            .map_err(|_| unavailable())?;
        if token.len() > 16_384 || token.trim().is_empty() {
            return Err(unavailable());
        }
        let client = builder.build().map_err(|_| unavailable())?;
        // Closed native call. Neither tool name, URL, headers nor token are WebView inputs.
        let response = client
            .post(url)
            .bearer_auth(token.trim())
            .json(&json!({"tool":"ssh_resource", "params":arguments}))
            .send()
            .await;
        drop(token);
        let mut response = response.map_err(|_| uncertain())?;
        let status = response.status().as_u16();
        if status == 403 {
            return Err(SshResourceError::InsufficientScope);
        }
        if status == 401 {
            return Err(SshResourceError::AuthorizationUnavailable);
        }
        if !matches!(status, 200 | 400) {
            return Err(uncertain());
        }
        let max = SSH_RESOURCE_RESPONSE_MAX_BYTES + 4096;
        if response
            .content_length()
            .is_some_and(|size| size > max as u64)
        {
            return Err(uncertain());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| uncertain())? {
            if bytes.len().saturating_add(chunk.len()) > max {
                return Err(uncertain());
            }
            bytes.extend_from_slice(&chunk);
        }
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| uncertain())?;
        if envelope.success && status == 200 {
            Ok(envelope.output)
        } else {
            Err(SshResourceError::from_code(
                envelope.output.get("error_kind").and_then(Value::as_str),
                mutation,
            ))
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    success: bool,
    output: Value,
}

#[cfg(test)]
mod tests;
