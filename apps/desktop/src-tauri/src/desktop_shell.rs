use crate::error::{DesktopError, DesktopResult};
use serde::Serialize;
use std::sync::Mutex;

mod lifecycle;
pub use lifecycle::NavigationIntent;
use lifecycle::{Lifecycle, OpenAction};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;

pub const MAIN_WINDOW_LABEL: &str = "main";
pub const NAVIGATE_EVENT: &str = "desktop:navigate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDisposition {
    HideWindow,
    AllowExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationTarget {
    Activity,
    Settings,
    Connections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShowPlan {
    show: bool,
    unminimize: bool,
    focus: bool,
}

#[derive(Default)]
pub struct DesktopShellState {
    lifecycle: Mutex<Lifecycle>,
}

impl DesktopShellState {
    fn with<T>(&self, f: impl FnOnce(&mut Lifecycle) -> T) -> T {
        f(&mut self.lifecycle.lock().unwrap_or_else(|p| p.into_inner()))
    }

    pub fn close_disposition(&self) -> CloseDisposition {
        self.with(|state| state.close_disposition())
    }

    pub fn mark_exit_requested(&self) {
        self.with(Lifecycle::request_exit);
    }

    pub fn prevent_implicit_exit(&self, code: Option<i32>) -> bool {
        self.with(|state| state.prevent_implicit_exit(code))
    }

    pub fn can_enter_lightweight(&self) -> bool {
        self.with(|state| state.can_enter_lightweight())
    }

    pub fn mark_bootstrap_complete(&self) {
        self.with(Lifecycle::mark_bootstrap_complete);
    }

    pub fn needs_background_observation(&self) -> bool {
        self.with(|state| state.prevent_implicit_exit(None))
    }

    pub fn restore_only(&self) -> bool {
        self.with(|state| state.restore_only())
    }

    pub fn pending_navigation(&self) -> Option<NavigationIntent> {
        self.with(|state| state.pending_navigation())
    }

    pub fn acknowledge_navigation(&self, sequence: u32) {
        self.with(|state| state.acknowledge_navigation(sequence));
    }
}

fn show_plan(visible: bool, minimized: bool) -> ShowPlan {
    ShowPlan {
        show: !visible,
        unminimize: minimized,
        focus: true,
    }
}

pub fn is_background_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--background")
}

pub fn second_instance_requests_focus(argv: &[String]) -> bool {
    !is_background_launch(argv.iter().map(String::as_str))
}

pub fn handle_second_instance(app: &AppHandle, argv: &[String]) {
    if second_instance_requests_focus(argv) {
        let _ = show_main_window(app);
    }
}

/// All entry points enqueue UI intent. Only the separate recreation worker may
/// build a WebView; synchronous tray/single-instance/Reopen callbacks never do.
pub fn show_main_window(app: &AppHandle) -> DesktopResult<()> {
    schedule_open(app, None)
}

fn show_existing(window: &tauri::WebviewWindow) -> DesktopResult<()> {
    let visible = window.is_visible().map_err(window_error)?;
    let minimized = window.is_minimized().map_err(window_error)?;
    let plan = show_plan(visible, minimized);
    if plan.unminimize {
        window.unminimize().map_err(window_error)?;
    }
    if plan.show {
        window.show().map_err(window_error)?;
    }
    if plan.focus {
        window.set_focus().map_err(window_error)?;
    }
    Ok(())
}

pub fn hide_main_window(app: &AppHandle) -> DesktopResult<()> {
    let window = app.get_webview_window(MAIN_WINDOW_LABEL).ok_or_else(|| {
        DesktopError::new(
            "desktop_window_unavailable",
            "The main WebCodex window is unavailable",
            "Quit WebCodex and start it again.",
        )
    })?;
    window.hide().map_err(window_error)
}

pub fn navigate(app: &AppHandle, target: NavigationTarget) -> DesktopResult<()> {
    schedule_open(app, Some(target))
}

fn schedule_open(app: &AppHandle, target: Option<NavigationTarget>) -> DesktopResult<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let action = handle
            .state::<DesktopShellState>()
            .with(|state| state.open(target));
        perform_open(&handle, action);
    })
    .map_err(window_error)
}

fn perform_open(app: &AppHandle, action: OpenAction) {
    match action {
        OpenAction::Show => {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                if let Err(error) = show_existing(&window) {
                    report_window_error(&error);
                }
                notify_navigation(app);
            } else {
                eprintln!("WebCodex window action failed: desktop_window_unavailable");
            }
        }
        OpenAction::Recreate(generation) => recreate_main_window(app, generation),
        OpenAction::None => {}
    }
    refresh_tray(app);
}

fn recreate_main_window(app: &AppHandle, generation: u32) {
    let handle = app.clone();
    // Tauri 2 / WebView2 can deadlock if build() is called from a synchronous
    // event callback. A single fenced worker builds hidden from the exact config;
    // completion is serialized back onto the event loop before showing/focusing.
    let spawned = std::thread::Builder::new()
        .name("desktop-ui-recreate".into())
        .spawn(move || {
            if !handle
                .state::<DesktopShellState>()
                .with(|state| state.is_recreating(generation))
            {
                return;
            }
            let result = handle
                .config()
                .app
                .windows
                .iter()
                .find(|config| config.label == MAIN_WINDOW_LABEL)
                .ok_or_else(|| {
                    DesktopError::new(
                        "desktop_window_config_missing",
                        "Main window configuration is unavailable",
                        "Restart Desktop.",
                    )
                })
                .and_then(|config| {
                    tauri::WebviewWindowBuilder::from_config(&handle, config)
                        .and_then(|builder| builder.visible(false).build())
                        .map_err(window_error)
                });
            let complete = handle.clone();
            if let Err(error) = handle.run_on_main_thread(move || {
                match result {
                    Ok(window) => {
                        if complete
                            .state::<DesktopShellState>()
                            .with(|state| state.recreated(generation))
                        {
                            if let Err(error) = show_existing(&window) {
                                report_window_error(&error);
                            }
                            notify_navigation(&complete);
                        } else {
                            // Explicit Quit wins over late creation. Never revive UI.
                            let _ = window.destroy();
                        }
                    }
                    Err(error) => {
                        complete
                            .state::<DesktopShellState>()
                            .with(|state| state.recreate_failed(generation));
                        report_window_error(&error);
                    }
                }
                refresh_tray(&complete);
            }) {
                handle
                    .state::<DesktopShellState>()
                    .with(|state| state.recreate_failed(generation));
                report_window_error(&window_error(error));
            }
        });
    if spawned.is_err() {
        app.state::<DesktopShellState>()
            .with(|state| state.recreate_failed(generation));
        eprintln!("WebCodex window action failed: desktop_recreation_worker_unavailable");
    }
}

pub fn enter_lightweight_mode(app: &AppHandle) -> DesktopResult<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if !handle
            .state::<DesktopShellState>()
            .with(Lifecycle::begin_lightweight)
        {
            return;
        }
        let result = handle
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| {
                DesktopError::new(
                    "desktop_window_unavailable",
                    "Main window is unavailable",
                    "Retry opening Desktop.",
                )
            })
            .and_then(|window| window.destroy().map_err(window_error));
        if let Err(error) = result {
            handle
                .state::<DesktopShellState>()
                .with(Lifecycle::destroy_failed);
            report_window_error(&error);
        }
        // Success here only means destruction was requested. Destroyed is the
        // authoritative transition; no hidden replacement WebView is retained.
        refresh_tray(&handle);
    })
    .map_err(window_error)
}

pub fn main_window_destroyed(app: &AppHandle) {
    // Tauri has removed the old label from its manager before this event.
    let action = app.state::<DesktopShellState>().with(Lifecycle::destroyed);
    perform_open(app, action);
}

fn notify_navigation(app: &AppHandle) {
    if let Some(intent) = app.state::<DesktopShellState>().pending_navigation() {
        // Event is a wake-up only. The retained slot is acknowledged after the
        // renderer applies it, so registration delay cannot drop navigation.
        if let Err(error) = app.emit_to(MAIN_WINDOW_LABEL, NAVIGATE_EVENT, intent.target) {
            report_window_error(&window_error(error));
        }
    }
}

fn refresh_tray(app: &AppHandle) {
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        crate::tray::refresh_from_snapshot(app, &state.get_state());
    }
}

fn report_window_error(error: &DesktopError) {
    // No unbounded platform error or user path in tray diagnostics.
    eprintln!("WebCodex window action failed: {}", error.code);
}

pub fn request_application_exit(app: &AppHandle) {
    app.state::<DesktopShellState>().mark_exit_requested();
    app.exit(0);
}

pub fn launch_at_login_enabled(app: &AppHandle) -> DesktopResult<bool> {
    app.autolaunch().is_enabled().map_err(autostart_error)
}

pub fn set_launch_at_login(app: &AppHandle, enabled: bool) -> DesktopResult<bool> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(autostart_error)?;
    } else {
        manager.disable().map_err(autostart_error)?;
    }
    manager.is_enabled().map_err(autostart_error)
}

fn window_error(error: tauri::Error) -> DesktopError {
    DesktopError::new(
        "desktop_window_action_failed",
        format!("Desktop window action failed: {error}"),
        "Retry the Desktop window action.",
    )
}

fn autostart_error(error: impl std::fmt::Display) -> DesktopError {
    DesktopError::new(
        "desktop_autostart_unavailable",
        format!("Could not update the operating system login registration: {error}"),
        "Retry Launch at Login from Desktop Settings.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_requested_background_decision_is_not_quit() {
        let shell = DesktopShellState::default();
        assert_eq!(shell.close_disposition(), CloseDisposition::HideWindow);
        shell.mark_exit_requested();
        assert_eq!(shell.close_disposition(), CloseDisposition::AllowExit);
    }

    #[test]
    fn background_startup_detection_is_exact() {
        assert!(is_background_launch(["WebCodex", "--background"]));
        assert!(!is_background_launch(["WebCodex", "--backgroundish"]));
        assert!(!is_background_launch(["WebCodex"]));
    }

    #[test]
    fn second_instance_normal_launch_requests_focus_but_background_does_not() {
        assert!(second_instance_requests_focus(&["WebCodex".into()]));
        assert!(!second_instance_requests_focus(&[
            "WebCodex".into(),
            "--background".into(),
        ]));
    }

    #[test]
    fn show_plan_handles_visible_hidden_and_minimized_windows_idempotently() {
        assert_eq!(
            show_plan(true, false),
            ShowPlan {
                show: false,
                unminimize: false,
                focus: true,
            }
        );
        assert_eq!(
            show_plan(false, false),
            ShowPlan {
                show: true,
                unminimize: false,
                focus: true,
            }
        );
        assert_eq!(
            show_plan(true, true),
            ShowPlan {
                show: false,
                unminimize: true,
                focus: true,
            }
        );
    }
}
