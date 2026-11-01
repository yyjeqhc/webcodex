//! Native activity hints and local Escape detection never own execution authority.
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct Presentation {
    pub controlling: AtomicBool,
    #[cfg(windows)]
    pub shortcut_ready: AtomicBool,
}

pub fn update_edge(app: &AppHandle, activity: bool) {
    let monitors = app.available_monitors().unwrap_or_default();
    for (index, monitor) in monitors.iter().enumerate().take(16) {
        let label = format!("computer-activity-{index}");
        let existing = app.get_webview_window(&label);
        if !activity {
            if let Some(window) = existing {
                let _ = window.hide();
            }
            continue;
        }
        let window = existing.or_else(|| {
            tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App("computer-activity.html".into()),
            )
            .title("WebCodex Computer activity")
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .focusable(false)
            .content_protected(true)
            .visible(false)
            .inner_size(
                f64::from(monitor.size().width) / monitor.scale_factor(),
                30.0,
            )
            .build()
            .ok()
        });
        if let Some(window) = window {
            if window.set_ignore_cursor_events(true).is_err() {
                let _ = window.hide();
                continue;
            }
            let _ = window.set_position(*monitor.position());
            let _ = window.set_size(tauri::LogicalSize::new(
                f64::from(monitor.size().width) / monitor.scale_factor(),
                30.0,
            ));
            let _ = window.show();
        }
    }
    // Hide orphaned overlays after a display is removed.
    for (label, window) in app.webview_windows() {
        if label
            .strip_prefix("computer-activity-")
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .is_some_and(|index| index >= monitors.len())
        {
            let _ = window.hide();
        }
    }
}

#[cfg(windows)]
pub fn start_shortcut_listener(app: &AppHandle) {
    let listener_app = app.clone();
    let result = std::thread::Builder::new()
        .name("computer-pause-shortcut".into())
        .spawn(move || {
            let app = listener_app;
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                PeekMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG, PM_REMOVE,
                WH_KEYBOARD_LL,
            };
            let hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_hook),
                    GetModuleHandleW(std::ptr::null()),
                    0,
                )
            };
            if hook.is_null() {
                eprintln!(
                    "WebCodex Computer Esc listener unavailable: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }
            app.state::<Presentation>()
                .shortcut_ready
                .store(true, Ordering::Release);
            loop {
                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) }
                    != 0
                {}
                if ESCAPE_REQUESTED.with(|requested| requested.replace(false))
                    && app
                        .state::<Presentation>()
                        .controlling
                        .load(Ordering::Acquire)
                {
                    crate::tray::pause_computer(&app);
                }
                if app
                    .state::<crate::desktop_shell::DesktopShellState>()
                    .close_disposition()
                    == crate::desktop_shell::CloseDisposition::AllowExit
                {
                    unsafe { UnhookWindowsHookEx(hook) };
                    app.state::<Presentation>()
                        .shortcut_ready
                        .store(false, Ordering::Release);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });
    if result.is_err() {
        eprintln!("WebCodex Computer Esc listener thread could not start");
    }
}

#[cfg(windows)]
thread_local! {
    static ESCAPE_REQUESTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(windows)]
fn physical_escape(
    code: i32,
    message: usize,
    key: &windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows_sys::Win32::UI::WindowsAndMessaging::{LLKHF_INJECTED, WM_KEYDOWN, WM_SYSKEYDOWN};
    code == 0
        && matches!(message as u32, WM_KEYDOWN | WM_SYSKEYDOWN)
        && key.vkCode == u32::from(VK_ESCAPE)
        && key.flags & LLKHF_INJECTED == 0
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook(code: i32, message: usize, data: isize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CallNextHookEx, KBDLLHOOKSTRUCT};
    if code == 0 && physical_escape(code, message, &*(data as *const KBDLLHOOKSTRUCT)) {
        ESCAPE_REQUESTED.with(|requested| requested.set(true));
    }
    // Observe only: Escape still reaches the foreground app, and injected
    // Computer input cannot trigger this local-user pause path.
    CallNextHookEx(std::ptr::null_mut(), code, message, data)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN,
    };

    #[test]
    fn escape_pause_accepts_local_key_down_and_rejects_injected_input() {
        let mut key = KBDLLHOOKSTRUCT {
            vkCode: u32::from(VK_ESCAPE),
            scanCode: 0,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        assert!(physical_escape(0, WM_KEYDOWN as usize, &key));
        assert!(physical_escape(0, WM_SYSKEYDOWN as usize, &key));
        assert!(!physical_escape(-1, WM_KEYDOWN as usize, &key));
        assert!(!physical_escape(0, WM_KEYUP as usize, &key));
        key.flags = LLKHF_INJECTED;
        assert!(!physical_escape(0, WM_KEYDOWN as usize, &key));
        key.flags |= LLKHF_LOWER_IL_INJECTED;
        assert!(!physical_escape(0, WM_KEYDOWN as usize, &key));
        key.flags = 0;
        key.vkCode = 0x41;
        assert!(!physical_escape(0, WM_KEYDOWN as usize, &key));

        key.vkCode = u32::from(VK_ESCAPE);
        ESCAPE_REQUESTED.with(|requested| requested.set(false));
        for flags in [LLKHF_INJECTED, 0] {
            key.flags = flags;
            unsafe {
                keyboard_hook(0, WM_KEYDOWN as usize, &key as *const _ as isize);
            }
            assert_eq!(
                ESCAPE_REQUESTED.with(|requested| requested.replace(false)),
                flags == 0,
                "only a local Escape should reach the pause dispatcher"
            );
        }
    }
}
