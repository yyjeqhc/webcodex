use crate::activity::ActivityEntry;
use crate::desktop_shell;
use crate::error::{DesktopError, DesktopResult};
use crate::models::{DesktopStateSnapshot, ProjectSelection, TunnelProxyMode};
use crate::state::AppState;
use crate::tray;
use serde::Deserialize;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn authorize_runner_capabilities(
    state: State<'_, AppState>,
    request: crate::runner_capability_grant::GrantRequest,
) -> DesktopResult<crate::runner_capability_grant::AuthorizationSnapshot> {
    state.authorize_runner_capabilities(request).await
}

#[tauri::command]
pub async fn runner_capability_authorization(
    state: State<'_, AppState>,
    expected: crate::webcodex::settings::SettingsTarget,
) -> DesktopResult<crate::runner_capability_grant::AuthorizationSnapshot> {
    state.runner_capability_authorization(expected).await
}

#[tauri::command]
pub async fn ssh_resource_list(
    state: State<'_, AppState>,
) -> DesktopResult<crate::ssh_resources::SshResourcesSnapshot> {
    state.ssh_resource_list().await
}

#[tauri::command]
pub async fn ssh_resource_register(
    state: State<'_, AppState>,
    request: crate::ssh_resources::SshRegisterRequest,
) -> DesktopResult<crate::ssh_resources::SshMutationResult> {
    state.ssh_resource_register(request).await
}

#[tauri::command]
pub async fn ssh_resource_remove(
    state: State<'_, AppState>,
    request: crate::ssh_resources::SshRemoveRequest,
) -> DesktopResult<crate::ssh_resources::SshMutationResult> {
    state.ssh_resource_remove(request).await
}

#[tauri::command]
pub async fn save_coding_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::coding_agents::CodingAgentUpdate,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.save_coding_agent(request).await)
}

#[tauri::command]
pub async fn remove_coding_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::coding_agents::CodingAgentRemove,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.remove_coding_agent(request).await)
}

#[tauri::command]
pub async fn save_mcp_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::mcp_providers::McpProviderRequest,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.save_mcp_provider(request).await)
}

#[tauri::command]
pub async fn remove_mcp_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    expected_revision: u64,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.remove_mcp_provider(id, expected_revision).await)
}

#[tauri::command]
pub async fn save_tunnel_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::tunnel_config::TunnelProfileRequest,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.save_tunnel_profile(request).await)
}

#[tauri::command]
pub async fn tunnel_profile_action(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: crate::connection_id::TunnelProfileId,
    action: crate::state::ConnectionAction,
) -> DesktopResult<DesktopStateSnapshot> {
    project_state_result(&app, state.tunnel_profile_action(profile_id, action).await)
}

#[tauri::command]
pub async fn update_tunnel_config(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::tunnel_config::TunnelConfigRequest,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.update_tunnel_config(request).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub project_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSetupRequest {
    pub project_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSetupRequest {
    pub server_url: String,
    pub pairing_code: String,
    pub project_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickShareRequest {
    pub project_path: String,
    pub provider: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelDesktopOperationRequest {
    pub operation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelProxyRequest {
    pub mode: TunnelProxyMode,
    pub custom_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchAtLoginRequest {
    pub enabled: bool,
}

fn project_state_result(
    app: &AppHandle,
    result: Result<DesktopStateSnapshot, DesktopError>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    if let Ok(snapshot) = &result {
        tray::refresh_from_snapshot(app, snapshot);
    }
    result
}

#[tauri::command]
pub async fn get_desktop_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    let snapshot = state.get_state();
    tray::refresh_from_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn open_powershell_install_guide() -> Result<(), DesktopError> {
    crate::platform::open_powershell_install_guide()
}

#[tauri::command]
pub async fn get_launch_at_login(app: AppHandle) -> Result<bool, DesktopError> {
    match desktop_shell::launch_at_login_enabled(&app) {
        Ok(enabled) => {
            tray::set_launch_at_login_observation(&app, Some(enabled));
            Ok(enabled)
        }
        Err(error) => {
            tray::set_launch_at_login_observation(&app, None);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn set_launch_at_login(
    request: LaunchAtLoginRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, DesktopError> {
    match desktop_shell::set_launch_at_login(&app, request.enabled) {
        Ok(enabled) => {
            tray::set_launch_at_login_observation(&app, Some(enabled));
            Ok(enabled)
        }
        Err(error) => {
            tray::set_launch_at_login_observation(&app, None);
            let snapshot = state.get_state();
            tray::refresh_from_snapshot(&app, &snapshot);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn refresh_runtime_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.refresh_runtime_status().await)
}

#[tauri::command]
pub async fn observe_chatgpt_activity(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.observe_chatgpt_activity().await)
}

#[tauri::command]
pub async fn resume_saved_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.resume_saved_runtime().await)
}

#[tauri::command]
pub async fn update_tunnel_proxy(
    request: TunnelProxyRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    let result = state
        .update_tunnel_proxy(request.mode, request.custom_url.as_deref())
        .await;
    project_state_result(&app, result)
}

#[tauri::command]
pub async fn inspect_project(
    request: ProjectRequest,
    state: State<'_, AppState>,
) -> Result<ProjectSelection, DesktopError> {
    state.inspect_project(&request.project_path).await
}

#[tauri::command]
pub async fn configure_local_setup(
    request: LocalSetupRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    let result = state
        .configure_local_setup(request.project_path.as_deref())
        .await;
    project_state_result(&app, result)
}

#[tauri::command]
pub async fn activate_local_project(
    request: ProjectRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(
        &app,
        state.activate_local_project(&request.project_path).await,
    )
}

#[tauri::command]
pub async fn configure_remote_setup(
    request: RemoteSetupRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    let result = state
        .configure_remote_setup(
            &request.server_url,
            &request.pairing_code,
            &request.project_path,
        )
        .await;
    project_state_result(&app, result)
}

#[tauri::command]
pub async fn start_quick_share(
    request: QuickShareRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    let result = state
        .start_quick_share(&request.project_path, &request.provider)
        .await;
    project_state_result(&app, result)
}

#[tauri::command]
pub async fn stop_quick_share(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.stop_quick_share().await)
}

#[tauri::command]
pub async fn start_regular_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.start_regular_tunnel().await)
}

#[tauri::command]
pub async fn stop_regular_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.stop_regular_tunnel().await)
}

#[tauri::command]
pub async fn stop_local_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.stop_local_runtime().await)
}

#[tauri::command]
pub async fn cancel_desktop_operation(
    request: CancelDesktopOperationRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.cancel_operation(&request.operation_id))
}

#[tauri::command]
pub async fn get_bounded_activity(
    state: State<'_, AppState>,
) -> Result<Vec<ActivityEntry>, DesktopError> {
    Ok(state.activity())
}

#[tauri::command]
pub async fn get_runner_settings(
    state: State<'_, AppState>,
) -> Result<crate::webcodex::settings::RunnerSettings, DesktopError> {
    state.runner_settings().await
}
#[tauri::command]
pub async fn update_runner_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::webcodex::settings::SettingsUpdate,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.update_runner_settings(request).await)
}
#[tauri::command]
pub async fn restart_owned_runner(
    app: AppHandle,
    state: State<'_, AppState>,
    target: crate::webcodex::settings::SettingsTarget,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.restart_owned_runner(target).await)
}

#[tauri::command]
pub fn get_computer_permissions(
    app: AppHandle,
) -> crate::platform::permissions::ComputerPermissions {
    use tauri::Manager;
    let mut permissions = crate::platform::permissions::probe();
    permissions.foreground = app
        .get_webview_window(crate::desktop_shell::MAIN_WINDOW_LABEL)
        .is_some_and(|window| {
            window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
        });
    permissions
}
#[tauri::command]
pub fn request_computer_permission(
    app: AppHandle,
    action: crate::platform::permissions::PermissionAction,
) -> Result<crate::platform::permissions::ComputerPermissions, DesktopError> {
    crate::platform::permissions::request(action)?;
    Ok(get_computer_permissions(app))
}

#[tauri::command]
pub async fn add_runner_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::webcodex::settings::PluginAddRequest,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.add_runner_plugin(request).await)
}

#[tauri::command]
pub async fn workspace_query(
    state: State<'_, AppState>,
    request: crate::workspace::WorkspaceRequest,
) -> Result<serde_json::Value, DesktopError> {
    state.workspace_query(request).await
}

#[tauri::command]
pub async fn get_runtime_settings(
    state: State<'_, AppState>,
) -> DesktopResult<crate::runtime_selection::RuntimeSettings> {
    state.runtime_settings().await
}

#[tauri::command]
pub async fn probe_runtime(
    source: crate::runtime_selection::RuntimeSource,
    state: State<'_, AppState>,
) -> DesktopResult<crate::runtime_selection::RuntimeSettings> {
    state.probe_runtime(source).await
}

#[tauri::command]
pub async fn recheck_runtime(
    state: State<'_, AppState>,
) -> DesktopResult<crate::runtime_selection::RuntimeSettings> {
    state.recheck_runtime().await
}

#[tauri::command]
pub async fn switch_runtime(
    request: crate::runtime_selection::RuntimeSwitchRequest,
    state: State<'_, AppState>,
) -> DesktopResult<crate::runtime_selection::RuntimeSwitchResult> {
    state.switch_runtime(request).await
}

#[tauri::command]
pub async fn restore_previous_configuration(
    expected_primary_sha256: String,
    state: State<'_, AppState>,
) -> DesktopResult<DesktopStateSnapshot> {
    state
        .restore_previous_configuration(expected_primary_sha256)
        .await
}

#[tauri::command]
pub async fn get_diagnostics(
    state: State<'_, AppState>,
) -> DesktopResult<crate::diagnostics::DiagnosticSnapshot> {
    state.diagnostics().await
}

#[tauri::command]
pub async fn set_tool_request_tracing(
    request: crate::diagnostics::TraceUpdate,
    state: State<'_, AppState>,
) -> DesktopResult<crate::diagnostics::TraceSettings> {
    state.set_tool_request_tracing(request).await
}

#[tauri::command]
pub async fn open_diagnostic_resource(
    kind: crate::diagnostics::ResourceKind,
    state: State<'_, AppState>,
) -> DesktopResult<()> {
    state.open_diagnostic_resource(kind).await
}

#[tauri::command]
pub async fn copy_runtime_console_credential(
    expected_fence: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> DesktopResult<()> {
    state.copy_console_credential(&app, &expected_fence).await
}

#[tauri::command]
pub async fn copy_diagnostic_report(
    app: AppHandle,
    state: State<'_, AppState>,
) -> DesktopResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let report = state.diagnostics().await?;
    app.clipboard()
        .write_text(report.markdown)
        .map_err(|_| crate::diagnostics::diagnostic_error("clipboard_unavailable"))
}

#[tauri::command]
pub async fn export_support_bundle(path: String, state: State<'_, AppState>) -> DesktopResult<()> {
    let report = state.diagnostics().await?.report;
    tokio::task::spawn_blocking(move || {
        crate::diagnostics::export_support(std::path::Path::new(&path), &report)
    })
    .await
    .map_err(|_| crate::diagnostics::diagnostic_error("support_bundle_write_unconfirmed"))?
}

#[tauri::command]
pub async fn check_for_updates(
    manual: bool,
    state: State<'_, AppState>,
) -> DesktopResult<crate::updates::UpdateStatus> {
    state.check_for_updates(manual).await
}
#[tauri::command]
pub async fn remind_update_later(
    state: State<'_, AppState>,
) -> DesktopResult<crate::updates::UpdateStatus> {
    state.remind_update_later().await
}
#[tauri::command]
pub async fn open_latest_release(state: State<'_, AppState>) -> DesktopResult<()> {
    state.open_latest_release().await
}

#[tauri::command]
pub fn get_desktop_build_info() -> webcodex_core::desktop_runtime_contract::MachineBuildInfo {
    let mut info = webcodex_core::build_info::machine_build_info("webcodex-desktop");
    info.version = env!("CARGO_PKG_VERSION").to_string();
    info
}

#[tauri::command]
pub async fn prepare_project_unregister(
    project: String,
    state: State<'_, AppState>,
) -> Result<crate::project_inventory::UnregisterObservation, DesktopError> {
    state.prepare_project_unregister(&project).await
}

#[tauri::command]
pub async fn unregister_project(
    request: crate::project_inventory::UnregisterRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    project_state_result(&app, state.unregister_project(request).await)
}
