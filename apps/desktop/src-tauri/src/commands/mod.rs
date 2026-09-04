use crate::activity::ActivityEntry;
use crate::error::DesktopError;
use crate::models::{DesktopStateSnapshot, ProjectSelection};
use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub project_path: String,
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

#[tauri::command]
pub async fn get_desktop_state(
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state.lock().await.get_state().await
}

#[tauri::command]
pub async fn refresh_runtime_status(
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state.lock().await.refresh_runtime_status().await
}

#[tauri::command]
pub async fn inspect_project(
    request: ProjectRequest,
    state: State<'_, AppState>,
) -> Result<ProjectSelection, DesktopError> {
    state
        .lock()
        .await
        .inspect_project(&request.project_path)
        .await
}

#[tauri::command]
pub async fn configure_local_setup(
    request: ProjectRequest,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state
        .lock()
        .await
        .configure_local_setup(&request.project_path)
        .await
}

#[tauri::command]
pub async fn configure_remote_setup(
    request: RemoteSetupRequest,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state
        .lock()
        .await
        .configure_remote_setup(
            &request.server_url,
            &request.pairing_code,
            &request.project_path,
        )
        .await
}

#[tauri::command]
pub async fn start_quick_share(
    request: QuickShareRequest,
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state
        .lock()
        .await
        .start_quick_share(&request.project_path, &request.provider)
        .await
}

#[tauri::command]
pub async fn stop_quick_share(
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state.lock().await.stop_quick_share().await
}

#[tauri::command]
pub async fn stop_local_runtime(
    state: State<'_, AppState>,
) -> Result<DesktopStateSnapshot, DesktopError> {
    state.lock().await.stop_local_runtime().await
}

#[tauri::command]
pub async fn get_bounded_activity(
    state: State<'_, AppState>,
) -> Result<Vec<ActivityEntry>, DesktopError> {
    Ok(state.lock().await.activity())
}
