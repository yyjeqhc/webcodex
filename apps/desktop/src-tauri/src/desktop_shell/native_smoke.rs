//! Explicit Windows/WebView2 smoke. Runs in a separate test child with isolated
//! AppData, Environment and WebView storage, using the actual builder and callbacks.
//! Requires the Desktop dev server on the configured URL (or embedded assets).
use crate::{desktop_shell, state::AppState};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const CHILD_ROOT: &str = "WEBCODEX_LIGHTWEIGHT_SMOKE_ROOT";
const TEST_NAME: &str = "desktop_native_smoke::desktop_real_process_windows_lightweight_smoke";

fn wait_until(label: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(25);
    while !check() {
        assert!(Instant::now() < deadline, "native smoke deadline: {label}");
        // Observation-only smoke polling, never production navigation timing.
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn menu(app: &AppHandle, id: &'static str) {
    let handle = app.clone();
    app.run_on_main_thread(move || crate::tray::handle_menu_event(&handle, id))
        .unwrap();
}

fn visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

fn active_page(app: &AppHandle) -> String {
    let Some(window) = app.get_webview_window("main") else {
        return String::new();
    };
    let (tx, rx) = mpsc::sync_channel(1);
    window
        .eval_with_callback(
            "document.querySelector('button[aria-current=page]')?.textContent ?? ''",
            move |value| {
                let _ = tx.try_send(value);
            },
        )
        .unwrap();
    let raw = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("renderer JavaScript response");
    serde_json::from_str(&raw).unwrap_or_default()
}

fn unloaded(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_none()
        && app
            .state::<desktop_shell::DesktopShellState>()
            .prevent_implicit_exit(None)
}

fn exercise(app: &AppHandle) {
    wait_until("initial visible React UI", || {
        visible(app) && !active_page(app).is_empty()
    });
    wait_until("initial renderer bootstrap complete", || {
        app.state::<desktop_shell::DesktopShellState>()
            .can_enter_lightweight()
    });
    let state_address = app.state::<AppState>().inner() as *const AppState as usize;
    assert!(
        app.state::<AppState>().get_state().topology.is_none(),
        "smoke must not adopt a real Environment"
    );
    let stable_backend = || {
        assert_eq!(
            app.state::<AppState>().inner() as *const AppState as usize,
            state_address
        );
        let state = app.state::<AppState>().get_state();
        assert!(state.topology.is_none());
        assert!(state.current_operation.is_none());
        assert!(app.tray_by_id("webcodex-desktop").is_some());
    };
    app.get_webview_window("main").unwrap().close().unwrap();
    wait_until("ordinary X hides", || !visible(app));
    assert!(
        app.get_webview_window("main").is_some(),
        "ordinary close must not destroy"
    );
    menu(app, "tray.open");
    wait_until("open hidden window", || visible(app));
    eprintln!("NATIVE_SMOKE ordinary_close=hidden main=present");

    for (id, expected) in [
        ("tray.activity", "活动"),
        ("tray.settings", "设置"),
        ("tray.connections", "连接"),
    ] {
        menu(app, "tray.lightweight");
        menu(app, "tray.lightweight");
        wait_until("main WebView destroyed", || unloaded(app));
        assert!(app.webview_windows().is_empty());
        stable_backend();
        eprintln!("NATIVE_SMOKE lightweight main=absent tray=present backend=same");
        // Same production tray callbacks; simultaneous Open and page intent.
        menu(app, "tray.open");
        menu(app, id);
        menu(app, "tray.open");
        wait_until("single rebuilt window and requested page", || {
            visible(app) && active_page(app).contains(expected)
        });
        wait_until("navigation acknowledged", || {
            app.state::<desktop_shell::DesktopShellState>()
                .pending_navigation()
                .is_none()
        });
        assert_eq!(app.webview_windows().len(), 1);
        assert!(app
            .state::<desktop_shell::DesktopShellState>()
            .restore_only());
        stable_backend();
        eprintln!("NATIVE_SMOKE recreated page={id} main_count=1 navigation=consumed");
        app.get_webview_window("main").unwrap().close().unwrap();
        wait_until("ordinary X still hides after recreation", || !visible(app));
        assert!(app.get_webview_window("main").is_some());
        menu(app, "tray.open");
        wait_until("open after ordinary close", || visible(app));
    }
    menu(app, "tray.lightweight");
    wait_until("lightweight before second instance", || unloaded(app));
    desktop_shell::handle_second_instance(app, &["WebCodex".into()]);
    wait_until("second-instance entry recreates", || {
        visible(app) && !active_page(app).is_empty()
    });
    assert_eq!(app.webview_windows().len(), 1);
    menu(app, "tray.lightweight");
    wait_until("Quit from lightweight", || unloaded(app));
    stable_backend();
    eprintln!("NATIVE_SMOKE second_instance=recreated quit_from=lightweight");
}

#[test]
#[ignore = "real Windows WebView2 and dev server; separate isolated child; run explicitly"]
fn desktop_real_process_windows_lightweight_smoke() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        let root = tempfile::tempdir().unwrap();
        for directory in ["roaming", "local", "desktop", "webview"] {
            std::fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                TEST_NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ROOT, root.path())
            .env("APPDATA", root.path().join("roaming"))
            .env("LOCALAPPDATA", root.path().join("local"))
            .env("WEBCODEX_DESKTOP_DATA_DIR", root.path().join("desktop"))
            .env_remove("WEBCODEX_DESKTOP_BIN_DIR")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            output.status.success(),
            "isolated native smoke failed: {}",
            output.status
        );
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert_eq!(std::env::var_os("APPDATA").unwrap(), root.join("roaming"));
    assert_eq!(
        std::env::var_os("WEBCODEX_DESKTOP_DATA_DIR").unwrap(),
        root.join("desktop")
    );
    assert!(webcodex_environment::default_environment_dir()
        .unwrap()
        .starts_with(&root));
    // Independent hard deadline for an event-loop deadlock; this is a dedicated
    // child process, never the user's installed Desktop or background Runtime.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(120));
        eprintln!("NATIVE_SMOKE watchdog deadline");
        std::process::exit(124);
    });
    let mut context = tauri::generate_context!();
    context.config_mut().identifier =
        format!("dev.webcodex.lightweight-smoke.{}", std::process::id());
    context.config_mut().app.windows[0].data_directory = Some(root.join("webview"));
    let outcome = Arc::new(Mutex::new(None));
    let outcome_for_run = outcome.clone();
    let mut driver_started = false;
    let app = crate::desktop_builder()
        .any_thread()
        .build(context)
        .unwrap();
    let exit_code = app.run_return(move |app, event| {
        if matches!(&event, tauri::RunEvent::Ready) && !driver_started {
            driver_started = true;
            let handle = app.clone();
            let outcome = outcome_for_run.clone();
            std::thread::spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| exercise(&handle)));
                let passed = result.is_ok();
                *outcome.lock().unwrap() = Some(passed);
                // Use the actual Quit path even on test failure to release UI.
                menu(&handle, "tray.quit");
            });
        }
        crate::handle_run_event(app, event);
    });
    assert_eq!(exit_code, 0);
    assert_eq!(*outcome.lock().unwrap(), Some(true));
    eprintln!("NATIVE_SMOKE explicit_quit=exited");
}
