mod activity;
mod coding_agents;
mod commands;
mod connection_id;
mod connections;
mod deadline;
mod desktop_shell;
mod diagnostics;
mod error;
mod mcp_providers;
mod models;
mod operation;
mod platform;
mod process;
mod project_inventory;
mod runner_capability_grant;
mod runtime_selection;
mod ssh_resources;
mod state;
mod tray;
mod tunnel_config;
mod updates;
mod webcodex;
mod workspace;

use state::AppState;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

pub fn run() {
    let app = tauri::Builder::default()
        // Tauri recommends registering single-instance first so a secondary
        // process is rejected before any other plugin can initialize state.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            desktop_shell::handle_second_instance(app, &argv);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            let resource_dir = app.path().resource_dir()?;
            app.manage(AppState::new(data_dir, resource_dir)?);
            app.manage(desktop_shell::DesktopShellState::default());
            app.manage(tray::TrayPresentationCache::default());
            tray::setup(app.handle())?;

            let snapshot = app.state::<AppState>().get_state();
            tray::refresh_from_snapshot(app.handle(), &snapshot);
            if !desktop_shell::is_background_launch(std::env::args()) {
                desktop_shell::show_main_window(app.handle())?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_desktop_state,
            commands::get_runtime_settings,
            commands::get_desktop_build_info,
            commands::check_for_updates,
            commands::remind_update_later,
            commands::open_latest_release,
            commands::get_diagnostics,
            commands::set_tool_request_tracing,
            commands::open_diagnostic_resource,
            commands::copy_runtime_console_credential,
            commands::copy_diagnostic_report,
            commands::export_support_bundle,
            commands::probe_runtime,
            commands::recheck_runtime,
            commands::switch_runtime,
            commands::restore_previous_configuration,
            commands::workspace_query,
            commands::get_computer_permissions,
            commands::request_computer_permission,
            commands::get_runner_settings,
            commands::add_runner_plugin,
            commands::update_runner_settings,
            commands::restart_owned_runner,
            commands::open_powershell_install_guide,
            commands::get_launch_at_login,
            commands::set_launch_at_login,
            commands::refresh_runtime_status,
            commands::observe_chatgpt_activity,
            commands::resume_saved_runtime,
            commands::update_tunnel_proxy,
            commands::update_tunnel_config,
            commands::save_tunnel_profile,
            commands::save_mcp_provider,
            commands::remove_mcp_provider,
            commands::save_coding_agent,
            commands::remove_coding_agent,
            commands::ssh_resource_list,
            commands::ssh_resource_register,
            commands::ssh_resource_remove,
            commands::authorize_runner_capabilities,
            commands::runner_capability_authorization,
            commands::tunnel_profile_action,
            commands::inspect_project,
            commands::configure_local_setup,
            commands::activate_local_project,
            commands::prepare_project_unregister,
            commands::unregister_project,
            commands::configure_remote_setup,
            commands::start_quick_share,
            commands::stop_quick_share,
            commands::start_regular_tunnel,
            commands::stop_regular_tunnel,
            commands::stop_local_runtime,
            commands::cancel_desktop_operation,
            commands::get_bounded_activity,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build WebCodex Desktop");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if label == desktop_shell::MAIN_WINDOW_LABEL => {
            let shell = app_handle.state::<desktop_shell::DesktopShellState>();
            if shell.close_disposition() == desktop_shell::CloseDisposition::HideWindow {
                api.prevent_close();
                let _ = desktop_shell::hide_main_window(app_handle);
            }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } => {
            if !has_visible_windows {
                let _ = desktop_shell::show_main_window(app_handle);
            }
        }
        tauri::RunEvent::ExitRequested { .. } => {
            app_handle
                .state::<desktop_shell::DesktopShellState>()
                .mark_exit_requested();
            let state = app_handle.state::<AppState>();
            tauri::async_runtime::block_on(state.shutdown());
        }
        tauri::RunEvent::Exit => {
            let state = app_handle.state::<AppState>();
            tauri::async_runtime::block_on(state.shutdown());
        }
        _ => {}
    });
}
