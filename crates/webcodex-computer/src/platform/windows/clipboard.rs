use super::*;

#[cfg(windows)]
struct ClipboardOpenGuard {
    open: bool,
}

#[cfg(windows)]
impl ClipboardOpenGuard {
    fn open(owner: Option<WinHwnd>, error: &'static str) -> Result<Self, String> {
        unsafe { OpenClipboard(owner) }.map_err(|_| error.to_string())?;
        Ok(Self { open: true })
    }

    fn close_once(&mut self) -> bool {
        if !self.open {
            return false;
        }
        let closed = unsafe { CloseClipboard() }.is_ok();
        // A reported close failure still fails the operation, but keep Drop armed for
        // one best-effort cleanup attempt so the shared clipboard is not deliberately
        // left open for the lifetime of the Runner. This never retries clipboard data.
        self.record_close_result(closed)
    }

    fn record_close_result(&mut self, closed: bool) -> bool {
        if closed {
            self.open = false;
        }
        closed
    }
}

#[cfg(windows)]
impl Drop for ClipboardOpenGuard {
    fn drop(&mut self) {
        if self.open {
            self.open = false;
            let _ = unsafe { CloseClipboard() };
        }
    }
}

#[cfg(windows)]
fn unlock_clipboard_global(handle: HGLOBAL) -> Result<(), String> {
    // GlobalUnlock returns zero both when the final lock is successfully released
    // and on failure. Clear last-error first so the two cases remain distinguishable.
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let result = unsafe { GlobalUnlock(handle) };
    if result.is_ok() || unsafe { GetLastError() }.0 == 0 {
        Ok(())
    } else {
        Err("clipboard_failed: GlobalUnlock failed".to_string())
    }
}

#[cfg(windows)]
struct OwnedClipboardGlobal {
    handle: Option<HGLOBAL>,
}

#[cfg(windows)]
impl OwnedClipboardGlobal {
    fn allocate(prepared: &PreparedClipboardText) -> Result<Self, String> {
        if prepared.storage_bytes == 0
            || prepared.storage_bytes > crate::MAX_CLIPBOARD_NATIVE_STORAGE_BYTES
            || prepared.utf16.len().checked_mul(std::mem::size_of::<u16>())
                != Some(prepared.storage_bytes)
        {
            return Err("not_started: clipboard native allocation metadata is invalid".to_string());
        }
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, prepared.storage_bytes) }
            .map_err(|_| "not_started: GlobalAlloc failed for clipboard text".to_string())?;
        let owned = Self {
            handle: Some(handle),
        };
        let locked = unsafe { GlobalLock(handle) };
        if locked.is_null() {
            return Err("not_started: GlobalLock failed for clipboard text".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                prepared.utf16.as_ptr(),
                locked.cast::<u16>(),
                prepared.utf16.len(),
            );
        }
        unlock_clipboard_global(handle).map_err(|_| {
            "not_started: GlobalUnlock failed for prepared clipboard text".to_string()
        })?;
        Ok(owned)
    }

    fn data_handle(&self) -> HANDLE {
        let handle = self.handle.expect("clipboard global memory ownership");
        HANDLE(handle.0)
    }

    fn transfer_to_windows(&mut self) {
        self.handle = None;
    }
}

#[cfg(windows)]
impl Drop for OwnedClipboardGlobal {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // GlobalFree returns NULL on successful free, while the windows crate
            // Result wrapper treats NULL as an error. The call itself is authoritative;
            // no retry is attempted here.
            let _ = unsafe { GlobalFree(Some(handle)) };
        }
    }
}

#[cfg(windows)]
fn validate_clipboard_owner_hwnd(owner: WinHwnd) -> Result<(), String> {
    if owner.0.is_null() {
        Err("not_started: Runner-owned clipboard HWND is null".to_string())
    } else {
        Ok(())
    }
}

#[cfg(all(test, windows))]
pub(crate) fn clipboard_owner_hwnd_contract_for_test(non_null: bool) -> bool {
    let raw = if non_null {
        std::ptr::NonNull::<u8>::dangling()
            .as_ptr()
            .cast::<std::ffi::c_void>()
    } else {
        std::ptr::null_mut()
    };
    validate_clipboard_owner_hwnd(WinHwnd(raw)).is_ok()
}

#[cfg(all(test, windows))]
pub(crate) fn clipboard_close_cleanup_armed_for_test(close_succeeded: bool) -> bool {
    let mut guard = ClipboardOpenGuard { open: true };
    let _ = guard.record_close_result(close_succeeded);
    let cleanup_armed = guard.open;
    // Prevent the synthetic test guard from touching the real process clipboard in Drop.
    guard.open = false;
    cleanup_armed
}

#[cfg(windows)]
struct OwnedClipboardWindow(WinHwnd);

#[cfg(windows)]
impl OwnedClipboardWindow {
    fn new() -> Result<Self, String> {
        // Use the system STATIC class as a short-lived message-only window. This
        // creates a non-NULL Runner-owned HWND without activating/focusing user UI
        // and requires no custom WndProc or message-loop framework.
        let owner = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!(""),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|_| "not_started: failed to create Runner-owned clipboard window".to_string())?;
        validate_clipboard_owner_hwnd(owner)?;
        Ok(Self(owner))
    }

    fn hwnd(&self) -> WinHwnd {
        self.0
    }
}

#[cfg(windows)]
impl Drop for OwnedClipboardWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

#[cfg(windows)]
pub(crate) fn read_clipboard() -> Result<Value, String> {
    let mut clipboard = ClipboardOpenGuard::open(
        None,
        "clipboard_busy: OpenClipboard failed for bounded clipboard read",
    )?;
    let read_result = (|| {
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_err() {
            return clipboard_read_result_from_utf16(None, 0);
        }
        let data = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }
            .map_err(|_| "clipboard_failed: GetClipboardData(CF_UNICODETEXT) failed".to_string())?;
        let global = HGLOBAL(data.0);
        let native_storage_bytes = unsafe { GlobalSize(global) };
        if native_storage_bytes == 0 {
            return Err("clipboard_malformed: clipboard Unicode storage is empty".to_string());
        }
        if native_storage_bytes > crate::MAX_CLIPBOARD_NATIVE_STORAGE_BYTES {
            return Err(
                "clipboard_too_large: clipboard Unicode storage exceeds the bounded native range"
                    .to_string(),
            );
        }
        if native_storage_bytes % std::mem::size_of::<u16>() != 0 {
            return Err(
                "clipboard_malformed: clipboard Unicode storage has odd byte length".to_string(),
            );
        }
        let locked = unsafe { GlobalLock(global) };
        if locked.is_null() {
            return Err("clipboard_failed: GlobalLock failed for clipboard read".to_string());
        }
        let units = native_storage_bytes / std::mem::size_of::<u16>();
        let storage = unsafe { std::slice::from_raw_parts(locked.cast::<u16>(), units) };
        let decoded = clipboard_read_result_from_utf16(Some(storage), native_storage_bytes);
        let unlock = unlock_clipboard_global(global);
        match (decoded, unlock) {
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    })();
    finish_clipboard_read(read_result, || clipboard.close_once())
}

#[cfg(windows)]
pub(crate) fn write_clipboard(text: &str) -> Result<Value, String> {
    // Everything below through owner creation is pre-effect preparation.
    let prepared = prepare_clipboard_write_text(text)?;
    let mut global = OwnedClipboardGlobal::allocate(&prepared)?;
    let owner = OwnedClipboardWindow::new()?;
    let mut clipboard = ClipboardOpenGuard::open(
        Some(owner.hwnd()),
        "not_started: OpenClipboard failed before clipboard state changed",
    )?;

    let effect = run_clipboard_write_effect_steps(
        || unsafe { EmptyClipboard() }.is_ok(),
        || {
            let result = unsafe {
                SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(global.data_handle()))
            };
            if result.is_ok() {
                // SetClipboardData success transfers HGLOBAL ownership to Windows.
                global.transfer_to_windows();
                true
            } else {
                false
            }
        },
        || clipboard.close_once(),
    );

    match effect {
        ClipboardWriteEffectState::NotStarted => Err(
            "not_started: EmptyClipboard failed before clipboard content changed".to_string(),
        ),
        ClipboardWriteEffectState::OutcomeUnknown => Err(
            "outcome_unknown: clipboard state changed after EmptyClipboard but the complete CF_UNICODETEXT replacement could not be proven"
                .to_string(),
        ),
        ClipboardWriteEffectState::Success => Ok(json!({
            "platform": "windows",
            "text_bytes": prepared.text_bytes,
            "success": true,
        })),
    }
}
