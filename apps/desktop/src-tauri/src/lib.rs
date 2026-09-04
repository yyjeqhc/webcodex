mod activity;
mod commands;
mod error;
mod models;
mod platform;
mod process;
mod state;
mod webcodex;

use state::AppState;
use tauri::Manager;

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_desktop_state,
            commands::refresh_runtime_status,
            commands::inspect_project,
            commands::configure_local_setup,
            commands::configure_remote_setup,
            commands::start_quick_share,
            commands::stop_quick_share,
            commands::start_regular_tunnel,
            commands::stop_regular_tunnel,
            commands::stop_local_runtime,
            commands::get_bounded_activity,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build WebCodex Desktop");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<AppState>();
            tauri::async_runtime::block_on(state.shutdown());
        }
    });
}
