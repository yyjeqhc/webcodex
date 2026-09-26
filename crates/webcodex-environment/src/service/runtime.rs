//! SCM process entrypoint for Server, Runner and Tunnel binaries. A service
//! process owns one SCM name and one runtime; no second Runner is started.

#[cfg(windows)]
mod implementation {
    use super::super::{ServiceError, ServiceErrorCode};
    use std::ffi::c_void;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use tokio::sync::Notify;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Services::*;

    type Entry = Box<dyn FnOnce(ServiceStop) -> Result<(), String> + Send + 'static>;
    static ENTRY: OnceLock<Mutex<Option<Entry>>> = OnceLock::new();
    static RESULT: OnceLock<Mutex<Option<Result<(), String>>>> = OnceLock::new();
    static STOP: OnceLock<Arc<StopState>> = OnceLock::new();
    static STATUS: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    static SERVICE_NAME: OnceLock<Vec<u16>> = OnceLock::new();

    struct StopState {
        requested: AtomicBool,
        notify: Notify,
    }

    #[derive(Clone)]
    pub struct ServiceStop(Arc<StopState>);

    impl ServiceStop {
        pub fn is_requested(&self) -> bool {
            self.0.requested.load(Ordering::Acquire)
        }
        pub async fn cancelled(&self) {
            loop {
                let notified = self.0.notify.notified();
                if self.is_requested() {
                    return;
                }
                notified.await;
            }
        }
    }

    fn report(state: SERVICE_STATUS_CURRENT_STATE, error: u32) {
        let handle = STATUS.load(Ordering::Acquire);
        if handle.is_null() {
            return;
        }
        let status = SERVICE_STATUS {
            dwServiceType: SERVICE_WIN32_OWN_PROCESS,
            dwCurrentState: state,
            dwControlsAccepted: if state == SERVICE_RUNNING {
                SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
            } else {
                0
            },
            dwWin32ExitCode: error,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: if matches!(state, SERVICE_START_PENDING | SERVICE_STOP_PENDING) {
                1
            } else {
                0
            },
            dwWaitHint: if matches!(state, SERVICE_START_PENDING | SERVICE_STOP_PENDING) {
                30000
            } else {
                0
            },
        };
        unsafe {
            SetServiceStatus(handle, &status);
        }
    }

    unsafe extern "system" fn handler(control: u32, _: u32, _: *mut c_void, _: *mut c_void) -> u32 {
        if matches!(control, SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN) {
            report(SERVICE_STOP_PENDING, 0);
            if let Some(stop) = STOP.get() {
                stop.requested.store(true, Ordering::Release);
                stop.notify.notify_one();
            }
        }
        0
    }

    unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
        let name = SERVICE_NAME
            .get()
            .expect("service name registered before SCM dispatch");
        let handle =
            unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(handler), null_mut()) };
        if handle.is_null() {
            *RESULT.get().unwrap().lock().unwrap() =
                Some(Err("SCM control handler registration failed".into()));
            return;
        }
        STATUS.store(handle, Ordering::Release);
        report(SERVICE_START_PENDING, 0);
        let entry = ENTRY.get().unwrap().lock().unwrap().take();
        let Some(entry) = entry else {
            report(SERVICE_STOPPED, 1);
            return;
        };
        report(SERVICE_RUNNING, 0);
        let stop = ServiceStop(STOP.get().unwrap().clone());
        let result = catch_unwind(AssertUnwindSafe(|| entry(stop)))
            .unwrap_or_else(|_| Err("service runtime panicked".into()));
        report(SERVICE_STOPPED, if result.is_ok() { 0 } else { 1 });
        *RESULT.get().unwrap().lock().unwrap() = Some(result);
    }

    /// Call only after the binary has recognized its internal
    /// `--windows-service NAME` mode. `entry` should run the existing async
    /// runtime and select on `ServiceStop::cancelled()` for graceful stop.
    pub fn run_windows_service<F>(name: &str, entry: F) -> Result<(), ServiceError>
    where
        F: FnOnce(ServiceStop) -> Result<(), String> + Send + 'static,
    {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "invalid SCM service name",
            ));
        }
        let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        SERVICE_NAME.set(name_wide).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "SCM entry already initialized",
            )
        })?;
        ENTRY.set(Mutex::new(Some(Box::new(entry)))).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "SCM entry already initialized",
            )
        })?;
        RESULT.set(Mutex::new(None)).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "SCM result already initialized",
            )
        })?;
        STOP.set(Arc::new(StopState {
            requested: AtomicBool::new(false),
            notify: Notify::new(),
        }))
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "SCM stop signal already initialized",
            )
        })?;
        let name = SERVICE_NAME.get().unwrap();
        let table = [
            SERVICE_TABLE_ENTRYW {
                lpServiceName: name.as_ptr().cast_mut(),
                lpServiceProc: Some(service_main),
            },
            SERVICE_TABLE_ENTRYW {
                lpServiceName: null_mut(),
                lpServiceProc: None,
            },
        ];
        if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::OperationFailed,
                format!(
                    "StartServiceCtrlDispatcher failed (Windows error {})",
                    unsafe { GetLastError() }
                ),
            ));
        }
        RESULT
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Err("service runtime exited without reporting an outcome".into()))
            .map_err(|message| ServiceError::new(ServiceErrorCode::OperationFailed, message))
    }
}

#[cfg(windows)]
pub use implementation::{run_windows_service, ServiceStop};
