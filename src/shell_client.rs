use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentPollResponse, ShellAgentResultRequest,
    ShellAgentResultResponse, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientRegisterRequest, ShellClientRegisterResponse, ShellClientView, ShellClientsResponse,
    ShellRunRequest, ShellRunResponse,
};
use salvo::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const MAX_CLIENT_ID_LEN: usize = 80;
const MAX_CLIENT_FIELD_LEN: usize = 200;
const MAX_COMMAND_LEN: usize = 8_000;
const MAX_CWD_LEN: usize = 1_024;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_SYNC_WAIT_SECS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone)]
struct ShellClientRecord {
    client_id: String,
    display_name: Option<String>,
    owner: Option<String>,
    hostname: Option<String>,
    capabilities: ShellClientCapabilities,
    last_seen: i64,
}

#[derive(Debug)]
struct PendingShellRequest {
    request: ShellAgentShellRequest,
    waiter: Option<oneshot::Sender<ShellRunResponse>>,
}

#[derive(Debug, Default)]
struct ShellClientRegistryInner {
    clients: HashMap<String, ShellClientRecord>,
    pending_by_id: HashMap<String, PendingShellRequest>,
    queues_by_client: HashMap<String, VecDeque<String>>,
}

#[derive(Debug, Default)]
pub struct ShellClientRegistry {
    inner: Mutex<ShellClientRegistryInner>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn validate_id(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_CLIENT_ID_LEN {
        return Err(format!(
            "{} must be 1..={} characters",
            field, MAX_CLIENT_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '-', '_', and '.'",
            field
        ));
    }
    Ok(())
}

fn validate_optional_field(value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(value) = value {
        if value.chars().count() > MAX_CLIENT_FIELD_LEN {
            return Err(format!(
                "{} is too long; maximum is {} characters",
                field, MAX_CLIENT_FIELD_LEN
            ));
        }
        if value.contains('\0') {
            return Err(format!("{} cannot contain NUL bytes", field));
        }
    }
    Ok(())
}

fn validate_run_request(body: &ShellRunRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    let command = body.command.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if body.command.len() > MAX_COMMAND_LEN {
        return Err(format!(
            "command is too long; maximum is {} bytes",
            MAX_COMMAND_LEN
        ));
    }
    if body.command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if body.timeout_secs == 0 || body.timeout_secs > MAX_COMMAND_TIMEOUT_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {}",
            MAX_COMMAND_TIMEOUT_SECS
        ));
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for synchronous runShell",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

fn trim_string(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn command_preview(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    const MAX_PREVIEW: usize = 120;
    if first_line.chars().count() <= MAX_PREVIEW {
        first_line.to_string()
    } else {
        let preview = first_line.chars().take(MAX_PREVIEW).collect::<String>();
        format!("{}…", preview)
    }
}

fn truncate_output(value: Option<String>) -> Option<String> {
    value.map(|s| {
        if s.len() <= MAX_OUTPUT_BYTES {
            s
        } else {
            let mut start = s.len() - MAX_OUTPUT_BYTES;
            while start < s.len() && !s.is_char_boundary(start) {
                start += 1;
            }
            format!(
                "[output truncated to last {} bytes]\n{}",
                MAX_OUTPUT_BYTES,
                &s[start..]
            )
        }
    })
}

impl ShellClientRegistry {
    pub async fn register(
        &self,
        body: ShellClientRegisterRequest,
    ) -> Result<ShellClientView, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_optional_field(&body.display_name, "display_name")?;
        validate_optional_field(&body.owner, "owner")?;
        validate_optional_field(&body.hostname, "hostname")?;

        let client_id = body.client_id.trim().to_string();
        let record = ShellClientRecord {
            client_id: client_id.clone(),
            display_name: trim_string(body.display_name),
            owner: trim_string(body.owner),
            hostname: trim_string(body.hostname),
            capabilities: body.capabilities.unwrap_or_default(),
            last_seen: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        inner.clients.insert(client_id.clone(), record);
        Ok(Self::client_view_locked(&inner, &client_id).expect("client just inserted"))
    }

    pub async fn list_clients(&self) -> Vec<ShellClientView> {
        let inner = self.inner.lock().await;
        let mut ids = inner.clients.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| Self::client_view_locked(&inner, &id))
            .collect()
    }

    pub async fn enqueue_run(
        &self,
        body: ShellRunRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_run_request(&body)?;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            command: body.command.clone(),
            timeout_secs: body.timeout_secs,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(&body.client_id) {
            return Err(format!("unknown shell client: {}", body.client_id));
        }
        inner
            .queues_by_client
            .entry(body.client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
            },
        );
        Ok((request_id, rx))
    }

    pub async fn cancel_request(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending_by_id.remove(request_id);
        for queue in inner.queues_by_client.values_mut() {
            queue.retain(|id| id != request_id);
        }
    }

    pub async fn poll(
        &self,
        body: ShellAgentPollRequest,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        validate_id(&body.client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(&body.client_id) else {
            return Err(format!("unknown shell client: {}", body.client_id));
        };
        client.last_seen = now_ts();
        loop {
            let request_id = {
                let Some(queue) = inner.queues_by_client.get_mut(&body.client_id) else {
                    return Ok(None);
                };
                queue.pop_front()
            };
            let Some(request_id) = request_id else {
                return Ok(None);
            };
            if let Some(pending) = inner.pending_by_id.get(&request_id) {
                return Ok(Some(pending.request.clone()));
            }
        }
    }

    pub async fn complete(&self, body: ShellAgentResultRequest) -> Result<(), String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.request_id, "request_id")?;
        let mut inner = self.inner.lock().await;
        if let Some(client) = inner.clients.get_mut(&body.client_id) {
            client.last_seen = now_ts();
        }
        let Some(mut pending) = inner.pending_by_id.remove(&body.request_id) else {
            return Err(format!(
                "unknown or expired shell request: {}",
                body.request_id
            ));
        };
        if pending.request.client_id != body.client_id {
            return Err("request_id does not belong to client_id".to_string());
        }
        let error = body.error.clone();
        let response = ShellRunResponse {
            success: error.is_none() && body.exit_code == Some(0),
            request_id: body.request_id,
            client_id: body.client_id,
            cwd: pending.request.cwd,
            command_preview: command_preview(&pending.request.command),
            exit_code: body.exit_code,
            stdout: truncate_output(body.stdout),
            stderr: truncate_output(body.stderr),
            duration_ms: body.duration_ms,
            error,
        };
        if let Some(waiter) = pending.waiter.take() {
            let _ = waiter.send(response);
        }
        Ok(())
    }

    fn client_view_locked(
        inner: &ShellClientRegistryInner,
        client_id: &str,
    ) -> Option<ShellClientView> {
        let client = inner.clients.get(client_id)?;
        let pending_requests = inner
            .queues_by_client
            .get(client_id)
            .map(VecDeque::len)
            .unwrap_or(0);
        let age = now_ts().saturating_sub(client.last_seen);
        let connected = age <= 60;
        Some(ShellClientView {
            client_id: client.client_id.clone(),
            display_name: client.display_name.clone(),
            owner: client.owner.clone(),
            hostname: client.hostname.clone(),
            status: if connected { "online" } else { "stale" }.to_string(),
            connected,
            last_seen: client.last_seen,
            capabilities: client.capabilities.clone(),
            pending_requests,
        })
    }
}

fn get_registry(depot: &Depot) -> Option<Arc<ShellClientRegistry>> {
    depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned()
}

fn registry_error() -> Json<ShellClientsResponse> {
    Json(ShellClientsResponse {
        success: false,
        clients: Vec::new(),
        error: Some("Shell client registry not configured".to_string()),
    })
}

#[handler]
pub async fn shell_clients(depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(registry_error());
        return;
    };
    res.render(Json(ShellClientsResponse {
        success: true,
        clients: registry.list_clients().await,
        error: None,
    }));
}

#[handler]
pub async fn shell_agent_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellClientRegisterRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.register(body).await {
        Ok(client) => res.render(Json(ShellClientRegisterResponse {
            success: true,
            client: Some(client),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_poll(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentPollRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.poll(body).await {
        Ok(request) => res.render(Json(ShellAgentPollResponse {
            success: true,
            request,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_result(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentResultRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.complete(body).await {
        Ok(()) => res.render(Json(ShellAgentResultResponse {
            success: true,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellRunResponse {
            success: false,
            request_id: String::new(),
            client_id: String::new(),
            cwd: None,
            command_preview: String::new(),
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellRunRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id: String::new(),
                cwd: None,
                command_preview: String::new(),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let wait_timeout_secs = body.wait_timeout_secs;
    let client_id = body.client_id.clone();
    let cwd = body.cwd.clone();
    let preview = command_preview(&body.command);
    let (request_id, rx) = match registry
        .enqueue_run(body, "gpt_action_or_web".to_string())
        .await
    {
        Ok(result) => result,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(e),
            }));
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => res.render(Json(response)),
        Ok(Err(_closed)) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id,
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some("shell request waiter was dropped".to_string()),
            }));
        }
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            res.status_code(StatusCode::REQUEST_TIMEOUT);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id,
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(format!(
                    "timed out waiting {} seconds for shell client result",
                    wait_timeout_secs
                )),
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_registers_and_lists_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                display_name: Some("XRH".to_string()),
                owner: Some("yyjeqhc".to_string()),
                hostname: Some("fineserver".to_string()),
                capabilities: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_id, "xrh");
        assert!(clients[0].connected);
        assert_eq!(clients[0].pending_requests, 0);
    }

    #[tokio::test]
    async fn registry_enqueues_polls_and_completes_shell_request() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
            })
            .await
            .unwrap();
        let (request_id, rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "xrh".to_string(),
                    cwd: Some("/tmp".to_string()),
                    command: "echo hello".to_string(),
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "xrh".to_string(),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.command, "echo hello");
        registry
            .complete(ShellAgentResultRequest {
                client_id: "xrh".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(12),
                error: None,
            })
            .await
            .unwrap();
        let response = rx.await.unwrap();
        assert!(response.success);
        assert_eq!(response.stdout.as_deref(), Some("hello\n"));
    }

    #[tokio::test]
    async fn registry_rejects_unknown_client_run() {
        let registry = ShellClientRegistry::default();
        let err = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "missing".to_string(),
                    cwd: None,
                    command: "pwd".to_string(),
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
    }
}
