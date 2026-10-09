//! Native session-ending notification, independent of WebView/tray visibility.
use std::cell::Cell;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::SetProcessShutdownParameters;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Application-owned range, before ordinary children (the default is 0x280).
const OWNER_SHUTDOWN_LEVEL: u32 = 0x3ff;

pub(crate) struct SessionShutdownObserver {
    // Serialize posting against destruction so a reused HWND is never targeted.
    window: Arc<Mutex<usize>>,
}

struct Callback {
    shutdown: Box<dyn Fn() + Send>,
    ended: Cell<bool>,
    window: Arc<Mutex<usize>>,
}

impl SessionShutdownObserver {
    pub(crate) fn start(shutdown: impl Fn() + Send + 'static) -> io::Result<Self> {
        // SAFETY: changes only this process's documented shutdown ordering.
        if unsafe { SetProcessShutdownParameters(OWNER_SHUTDOWN_LEVEL, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let window = Arc::new(Mutex::new(0));
        let callback = Callback {
            shutdown: Box::new(shutdown),
            ended: Cell::new(false),
            window: Arc::clone(&window),
        };
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("webcodex-session-shutdown".into())
            .spawn(move || run_message_loop(callback, ready_tx))?;
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "session shutdown observer did not start",
                )
            })??;
        Ok(Self { window })
    }

    pub(crate) fn close(&self) {
        let window = self
            .window
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *window != 0 {
            // SAFETY: only our live window, protected against concurrent destruction.
            unsafe { PostMessageW(*window as HWND, WM_CLOSE, 0, 0) };
        }
    }
}

impl Drop for SessionShutdownObserver {
    fn drop(&mut self) {
        self.close();
    }
}

fn run_message_loop(callback: Callback, ready: mpsc::SyncSender<io::Result<()>>) {
    // A normal hidden top-level window receives session broadcasts. A
    // message-only window does not. This thread owns the HWND and callback
    // until destruction; the callback never moves while Win32 borrows it.
    let callback = Box::new(callback);
    let class: Vec<u16> = format!("WebCodexSessionShutdown-{}", uuid::Uuid::new_v4())
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: null requests this process's module, owned for process lifetime.
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let mut definition: WNDCLASSW = unsafe { std::mem::zeroed() };
    definition.hInstance = instance;
    definition.lpszClassName = class.as_ptr();
    definition.lpfnWndProc = Some(window_proc);
    // SAFETY: definition/class remain valid through registration and creation.
    if unsafe { RegisterClassW(&definition) } == 0 {
        let _ = ready.send(Err(io::Error::last_os_error()));
        return;
    }
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            (&*callback as *const Callback).cast_mut().cast(),
        )
    };
    if window.is_null() {
        let error = io::Error::last_os_error();
        unsafe { UnregisterClassW(class.as_ptr(), instance) };
        let _ = ready.send(Err(error));
        return;
    }
    *callback
        .window
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = window as usize;
    if ready.send(Ok(())).is_ok() {
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: valid message storage and a thread-owned native message loop.
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    let mut owned = callback
        .window
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *owned != 0 {
        *owned = 0;
        drop(owned);
        // SAFETY: created by this thread and callback still lives until return.
        unsafe { DestroyWindow(window) };
    }
    unsafe { UnregisterClassW(class.as_ptr(), instance) };
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: Win32 supplies CREATESTRUCTW for this message; lpCreateParams
        // is the stable callback allocation owned by run_message_loop.
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Callback;
    if !pointer.is_null() {
        let callback = &*pointer;
        match message {
            WM_QUERYENDSESSION => return 1,
            WM_ENDSESSION => {
                // Queries and cancelled shutdowns must leave runtime/leases
                // intact. Repeated confirmed notifications clean up only once.
                if wparam != 0 && !callback.ended.replace(true) {
                    if catch_unwind(AssertUnwindSafe(|| (callback.shutdown)())).is_err() {
                        eprintln!("WebCodex session shutdown cleanup panicked; restart fences remain authoritative");
                    }
                }
                return 0;
            }
            WM_CLOSE => {
                *callback
                    .window
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = 0;
                DestroyWindow(window);
                return 0;
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                PostQuitMessage(0);
            }
            _ => {}
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

#[cfg(test)]
#[path = "session_shutdown_tests.rs"]
mod tests;
