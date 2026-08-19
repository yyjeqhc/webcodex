#[cfg(any(target_os = "macos", windows))]
use super::validate_key_input;
use super::{
    bounded_text, ensure_raw_capture_bound, prepare_clipboard_write_text, validate_input_text,
    AccessibilityTreeResult, ApplicationRecord, ClipboardWriteEffectState, ComputerAction,
    DisplayRecord, ElementRecord, PlatformApplication, PlatformDisplay, PlatformWindow,
    PointerAction, PointerPlan, SurfaceRecord,
};
#[cfg(target_os = "macos")]
use super::{
    clipboard_read_result, ensure_correlated_fingerprint, is_secure_text_fingerprint,
    run_macos_clipboard_write_effect_steps, select_exact_ax_window_index,
    validate_element_state_target, validate_key_modifiers, validate_text_input_preflight,
    validate_text_input_target, AxObservationDeadline,
};
#[cfg(windows)]
use super::{
    clipboard_read_result_from_utf16, finish_clipboard_read, run_clipboard_write_effect_steps,
    PreparedClipboardText,
};
#[cfg(any(target_os = "macos", windows))]
use super::{is_supported_text_input_fingerprint, ElementFingerprint};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use xcap::Monitor;
use xcap::Window;

#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
#[cfg(target_os = "macos")]
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFRetained, CFString, CFType, CGPoint, CGSize,
};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{CGEvent, CGEventFlags, CGKeyCode, CGPreflightPostEventAccess};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;
#[cfg(any(target_os = "macos", windows))]
use std::collections::VecDeque;
#[cfg(any(target_os = "macos", windows))]
use std::ptr::NonNull;
#[cfg(windows)]
use std::time::{Duration, Instant};
#[cfg(any(target_os = "macos", windows))]
use uuid::Uuid;

#[cfg(windows)]
use windows::Win32::System::{
    Com::SAFEARRAY,
    Ole::{
        SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
        SafeArrayGetLBound, SafeArrayGetUBound,
    },
};
#[cfg(windows)]
use windows::{
    core::{w, IUnknown, Interface, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, GlobalFree, SetLastError, E_NOINTERFACE, E_POINTER, HANDLE, HGLOBAL,
            HWND as WinHwnd, POINT, RPC_E_CHANGED_MODE, WIN32_ERROR,
        },
        Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx,
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            COINIT_MULTITHREADED,
        },
        System::DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
        System::Ole::CF_UNICODETEXT,
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationInvokePattern,
            IUIAutomationScrollItemPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
            UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId,
            UIA_CustomControlTypeId, UIA_DataGridControlTypeId, UIA_DataItemControlTypeId,
            UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_GroupControlTypeId,
            UIA_HeaderControlTypeId, UIA_HeaderItemControlTypeId, UIA_HyperlinkControlTypeId,
            UIA_InvokePatternId, UIA_ListControlTypeId, UIA_ListItemControlTypeId,
            UIA_MenuControlTypeId, UIA_MenuItemControlTypeId, UIA_PaneControlTypeId,
            UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
            UIA_ScrollItemPatternId, UIA_SeparatorControlTypeId, UIA_SliderControlTypeId,
            UIA_SpinnerControlTypeId, UIA_StatusBarControlTypeId, UIA_TabControlTypeId,
            UIA_TabItemControlTypeId, UIA_TableControlTypeId, UIA_TextControlTypeId,
            UIA_ToolBarControlTypeId, UIA_ToolTipControlTypeId, UIA_TreeControlTypeId,
            UIA_TreeItemControlTypeId, UIA_ValuePatternId, UIA_WindowControlTypeId,
            UIA_CONTROLTYPE_ID, UIA_E_ELEMENTNOTAVAILABLE, UIA_E_NOTSUPPORTED, UIA_PATTERN_ID,
        },
        UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
            KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
            MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE,
            VK_HOME, VK_LBUTTON, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MBUTTON,
            VK_MENU, VK_NEXT, VK_PRIOR, VK_RBUTTON, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU,
            VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB, VK_UP, VK_XBUTTON1, VK_XBUTTON2,
        },
        UI::Shell::{
            Common::ITEMIDLIST, FOLDERID_AppsFolder, IEnumIDList, ILCombine, ILGetSize,
            IShellFolder, IShellItem, SHCreateItemFromIDList, SHGetDesktopFolder,
            SHGetKnownFolderIDList, ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_IDLIST,
            SHCONTF_FOLDERS, SHCONTF_NONFOLDERS, SHELLEXECUTEINFOW, SIGDN_NORMALDISPLAY,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
            IsIconic, ShowWindowAsync, EDD_GET_DEVICE_INTERFACE_NAME, HWND_MESSAGE,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
            SW_RESTORE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
    },
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HWND as SysHwnd,
    Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetCurrentObject, GetDIBits, GetObjectW, GetWindowDC, ReleaseDC, SelectObject, BITMAP,
        BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, OBJ_BITMAP, SRCCOPY,
    },
    Storage::Xps::PrintWindow,
};

#[cfg(windows)]
const MAX_NATIVE_APPLICATION_IDENTITY_BYTES: usize = 64 * 1024;
#[cfg(windows)]
const MAX_NATIVE_DISPLAY_IDENTITY_BYTES: usize = 2048;
#[cfg(windows)]
const MAX_WINDOWS_DISPLAY_DEVICE_CHILDREN: u32 = 16;
#[cfg(windows)]
const MAX_WINDOWS_DISPLAY_SCAN: usize = 64;

#[cfg(windows)]
struct OwnedPidl(*mut ITEMIDLIST);

#[cfg(windows)]
impl OwnedPidl {
    fn from_raw(raw: *mut ITEMIDLIST) -> Result<Self, String> {
        if raw.is_null() {
            Err(
                "application_failed: Windows Shell returned a null application identity"
                    .to_string(),
            )
        } else {
            Ok(Self(raw))
        }
    }

    fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0.cast_const()
    }

    fn identity_bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = unsafe { ILGetSize(Some(self.as_ptr())) } as usize;
        if bytes == 0 || bytes > MAX_NATIVE_APPLICATION_IDENTITY_BYTES {
            return Err(
                "application_failed: Windows application identity exceeds bound".to_string(),
            );
        }
        Ok(unsafe { std::slice::from_raw_parts(self.0.cast::<u8>(), bytes) }.to_vec())
    }
}

#[cfg(windows)]
impl Drop for OwnedPidl {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0 as *const std::ffi::c_void)) };
    }
}

#[cfg(windows)]
struct NativeApplicationCandidate {
    display_name: String,
    native_identity: Vec<u8>,
    pidl: OwnedPidl,
}

#[cfg(windows)]
struct ShellComInitialization;

#[cfg(windows)]
impl ShellComInitialization {
    fn new() -> Result<Self, String> {
        let result =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        if result.is_ok() {
            Ok(Self)
        } else if result == RPC_E_CHANGED_MODE {
            Err(
                "application_failed: Windows Shell requires a compatible STA COM apartment"
                    .to_string(),
            )
        } else {
            Err("application_failed: Windows Shell COM initialization failed".to_string())
        }
    }
}

#[cfg(windows)]
impl Drop for ShellComInitialization {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[cfg(windows)]
fn application_com() -> Result<ShellComInitialization, String> {
    ShellComInitialization::new()
}

#[cfg(windows)]
fn windows_apps_folder() -> Result<(OwnedPidl, IShellFolder), String> {
    let root = unsafe { SHGetKnownFolderIDList(&FOLDERID_AppsFolder, 0, None) }
        .map_err(|_| "application_failed: Windows AppsFolder is unavailable".to_string())
        .and_then(OwnedPidl::from_raw)?;
    let desktop = unsafe { SHGetDesktopFolder() }.map_err(|_| {
        "application_failed: Windows Shell desktop folder is unavailable".to_string()
    })?;
    let folder: IShellFolder = unsafe { desktop.BindToObject(root.as_ptr(), None::<&IBindCtx>) }
        .map_err(|_| "application_failed: Windows AppsFolder could not be bound".to_string())?;
    Ok((root, folder))
}

#[cfg(windows)]
fn display_name_for_application(pidl: &OwnedPidl) -> Result<String, String> {
    let item: IShellItem = unsafe { SHCreateItemFromIDList(pidl.as_ptr()) }.map_err(|_| {
        "application_failed: Windows application Shell item is unavailable".to_string()
    })?;
    let display = unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) }.map_err(|_| {
        "application_failed: Windows application display name is unavailable".to_string()
    })?;
    let text = unsafe { display.to_string() }
        .map_err(|_| "application_failed: Windows application display name is invalid".to_string());
    unsafe { CoTaskMemFree(Some(display.0 as *const std::ffi::c_void)) };
    let text = text?;
    if text.is_empty() || text.contains('\0') {
        Err("application_failed: Windows application display name is invalid".to_string())
    } else {
        Ok(text)
    }
}

#[cfg(windows)]
fn enumerate_native_applications(limit: usize) -> Result<Vec<NativeApplicationCandidate>, String> {
    let _com = application_com()?;
    let (root, folder) = windows_apps_folder()?;
    let mut enumerator: Option<IEnumIDList> = None;
    let flags = (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0) as u32;
    let hr = unsafe { folder.EnumObjects(WinHwnd(std::ptr::null_mut()), flags, &mut enumerator) };
    if hr.is_err() {
        return Err("application_failed: Windows AppsFolder enumeration failed".to_string());
    }
    let Some(enumerator) = enumerator else {
        return Ok(Vec::new());
    };
    let mut applications = Vec::with_capacity(limit.min(16));
    while applications.len() < limit {
        let mut items = [std::ptr::null_mut()];
        let mut fetched = 0u32;
        let hr = unsafe { enumerator.Next(&mut items, Some(&mut fetched)) };
        if hr.is_err() {
            return Err("application_failed: Windows AppsFolder enumeration failed".to_string());
        }
        if fetched == 0 {
            break;
        }
        if fetched != 1 || items[0].is_null() {
            return Err(
                "application_failed: Windows AppsFolder returned invalid enumeration metadata"
                    .to_string(),
            );
        }
        let relative = OwnedPidl::from_raw(items[0])?;
        let absolute = OwnedPidl::from_raw(unsafe {
            ILCombine(Some(root.as_ptr()), Some(relative.as_ptr()))
        })?;
        let native_identity = absolute.identity_bytes()?;
        let display_name = display_name_for_application(&absolute)?;
        applications.push(NativeApplicationCandidate {
            display_name,
            native_identity,
            pidl: absolute,
        });
    }
    Ok(applications)
}

#[cfg(windows)]
pub(super) fn list_applications(limit: usize) -> Result<Vec<PlatformApplication>, String> {
    enumerate_native_applications(limit).map(|applications| {
        let mut applications = applications
            .into_iter()
            .map(|application| PlatformApplication {
                display_name: application.display_name,
                native_identity: application.native_identity,
            })
            .collect::<Vec<_>>();
        super::sort_application_candidates(&mut applications);
        applications
    })
}

#[cfg(windows)]
fn revalidate_application_identity(native_identity: &[u8]) -> Result<OwnedPidl, String> {
    let applications = enumerate_native_applications(super::MAX_APPLICATION_SCAN)?;
    applications
        .into_iter()
        .find(|candidate| candidate.native_identity == native_identity)
        .map(|candidate| candidate.pidl)
        .ok_or_else(|| {
            "stale_application: native application identity changed or disappeared".to_string()
        })
}

#[cfg(windows)]
fn shell_execute_info_for_application(pidl: *const ITEMIDLIST) -> SHELLEXECUTEINFOW {
    let mut info = SHELLEXECUTEINFOW::default();
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_IDLIST | SEE_MASK_FLAG_NO_UI;
    info.lpIDList = pidl as *mut std::ffi::c_void;
    // Launch submission must not itself request window activation/focus.
    info.nShow = SW_SHOWNOACTIVATE.0;
    info
}

#[cfg(windows)]
pub(super) fn launch_application(
    application_id: &str,
    application: &ApplicationRecord,
) -> Result<Value, String> {
    // Hold a dedicated Shell-compatible STA COM apartment through exact
    // revalidation and native dispatch. An incompatible pre-existing apartment
    // fails closed before ShellExecuteExW is reached.
    let _com = application_com()?;
    // Every failure above ShellExecuteExW is definite pre-effect. Only the
    // exact fresh PIDL returned by revalidation may reach native dispatch.
    let pidl = revalidate_application_identity(&application.native_identity)?;
    let mut info = shell_execute_info_for_application(pidl.as_ptr());
    unsafe { ShellExecuteExW(&mut info) }.map_err(|_| {
        "outcome_unknown: Windows application launch result was ambiguous after native dispatch attempt"
            .to_string()
    })?;
    Ok(json!({
        "platform": "windows",
        "application_id": application_id,
        "success": true,
    }))
}

#[cfg(all(test, windows))]
pub(super) fn application_identity_revalidates_for_test(
    native_identity: &[u8],
) -> Result<(), String> {
    revalidate_application_identity(native_identity).map(drop)
}

#[cfg(all(test, windows))]
pub(super) fn application_shell_execute_contract_for_test(
    native_identity: &[u8],
) -> Result<bool, String> {
    let pidl = revalidate_application_identity(native_identity)?;
    let info = shell_execute_info_for_application(pidl.as_ptr());
    Ok(info.fMask & SEE_MASK_IDLIST != 0
        && info.lpIDList == pidl.as_ptr() as *mut std::ffi::c_void
        && info.lpFile.0.is_null()
        && info.lpParameters.0.is_null()
        && info.lpDirectory.0.is_null()
        && info.nShow == SW_SHOWNOACTIVATE.0)
}

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
            || prepared.storage_bytes > super::MAX_CLIPBOARD_NATIVE_STORAGE_BYTES
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
pub(super) fn clipboard_owner_hwnd_contract_for_test(non_null: bool) -> bool {
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
pub(super) fn clipboard_close_cleanup_armed_for_test(close_succeeded: bool) -> bool {
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
pub(super) fn read_clipboard() -> Result<Value, String> {
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
        if native_storage_bytes > super::MAX_CLIPBOARD_NATIVE_STORAGE_BYTES {
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
pub(super) fn write_clipboard(text: &str) -> Result<Value, String> {
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

#[cfg(windows)]
fn fixed_utf16_string(value: &[u16]) -> Result<String, String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16(&value[..end])
        .map_err(|_| "display_failed: Windows display identity is invalid UTF-16".to_string())
}

#[cfg(windows)]
fn windows_display_identity(monitor: &Monitor) -> Result<Vec<u8>, String> {
    let adapter = monitor.name().map_err(|_| {
        "display_failed: Windows display adapter identity is unavailable".to_string()
    })?;
    if adapter.is_empty() || adapter.contains('\0') || adapter.len() > 512 {
        return Err("display_failed: Windows display adapter identity is invalid".to_string());
    }
    let mut adapter_wide = adapter.encode_utf16().collect::<Vec<_>>();
    adapter_wide.push(0);
    let adapter_wide = PCWSTR(adapter_wide.as_ptr());
    let mut interface_id: Option<String> = None;
    for index in 0..MAX_WINDOWS_DISPLAY_DEVICE_CHILDREN {
        let mut device = DISPLAY_DEVICEW::default();
        device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let found = unsafe {
            EnumDisplayDevicesW(
                adapter_wide,
                index,
                &mut device,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool();
        if !found {
            break;
        }
        let candidate = fixed_utf16_string(&device.DeviceID)?;
        if candidate.is_empty() {
            continue;
        }
        if interface_id.is_some() {
            return Err(
                "display_failed: Windows display adapter has ambiguous monitor identity"
                    .to_string(),
            );
        }
        interface_id = Some(candidate);
    }
    let interface_id = interface_id.ok_or_else(|| {
        "display_failed: Windows display monitor interface identity is unavailable".to_string()
    })?;
    let mut identity = b"windows-display-v1\0".to_vec();
    identity.extend_from_slice(adapter.to_ascii_lowercase().as_bytes());
    identity.push(0);
    identity.extend_from_slice(interface_id.to_ascii_lowercase().as_bytes());
    if identity.len() > MAX_NATIVE_DISPLAY_IDENTITY_BYTES {
        return Err("display_failed: Windows display identity exceeds bound".to_string());
    }
    Ok(identity)
}

#[cfg(windows)]
fn platform_display_from_monitor(monitor: &Monitor) -> Result<PlatformDisplay, String> {
    let native_identity = windows_display_identity(monitor)?;
    let width = monitor
        .width()
        .map_err(|_| "display_failed: Windows display width is unavailable".to_string())?;
    let height = monitor
        .height()
        .map_err(|_| "display_failed: Windows display height is unavailable".to_string())?;
    let primary = monitor
        .is_primary()
        .map_err(|_| "display_failed: Windows display primary state is unavailable".to_string())?;
    if width == 0 || height == 0 {
        return Err("display_failed: Windows display geometry is invalid".to_string());
    }
    Ok(PlatformDisplay {
        native_identity,
        width,
        height,
        primary,
    })
}

#[cfg(windows)]
fn windows_monitors() -> Result<Vec<Monitor>, String> {
    let monitors = Monitor::all()
        .map_err(|_| "display_failed: Windows display enumeration failed".to_string())?;
    if monitors.len() > MAX_WINDOWS_DISPLAY_SCAN {
        return Err("display_failed: Windows display count exceeds native scan bound".to_string());
    }
    Ok(monitors)
}

#[cfg(windows)]
pub(super) fn list_displays(limit: usize) -> Result<Vec<PlatformDisplay>, String> {
    if limit == 0 || limit > super::MAX_DISPLAYS + 1 {
        return Err("invalid_request: display discovery native limit is invalid".to_string());
    }
    windows_monitors()?
        .into_iter()
        .take(limit)
        .map(|monitor| platform_display_from_monitor(&monitor))
        .collect()
}

#[cfg(windows)]
fn find_exact_display(display: &DisplayRecord) -> Result<Monitor, String> {
    let mut exact = None;
    for monitor in windows_monitors()? {
        let candidate = platform_display_from_monitor(&monitor)?;
        if candidate.native_identity != display.native_identity {
            continue;
        }
        if candidate.width != display.width || candidate.height != display.height {
            return Err(
                "stale_display: native display geometry changed after discovery".to_string(),
            );
        }
        if exact.is_some() {
            return Err("stale_display: native display identity is no longer unique".to_string());
        }
        exact = Some(monitor);
    }
    exact.ok_or_else(|| "stale_display: native display identity changed or disappeared".to_string())
}

#[cfg(windows)]
pub(super) fn capture_display(display: &DisplayRecord) -> Result<image::RgbaImage, String> {
    ensure_capture_permission()?;
    let monitor = find_exact_display(display)?;
    ensure_raw_capture_bound(display.width, display.height)?;
    let image = monitor
        .capture_image()
        .map_err(|_| "capture_failed: Windows display capture failed".to_string())?;
    if image.width() != display.width || image.height() != display.height {
        return Err(
            "capture_failed: Windows display capture geometry does not match the exact display"
                .to_string(),
        );
    }
    // Revalidate again after capture so a hotplug/replacement racing the read
    // causes the captured bytes to be discarded instead of accepted under a stale handle.
    find_exact_display(display)?;
    Ok(image)
}

#[cfg(windows)]
pub(super) struct PointerCoordinateContext {
    previous: DPI_AWARENESS_CONTEXT,
}

#[cfg(windows)]
impl Drop for PointerCoordinateContext {
    fn drop(&mut self) {
        let restored = unsafe { SetThreadDpiAwarenessContext(self.previous) };
        debug_assert!(!restored.0.is_null());
    }
}

#[cfg(windows)]
pub(super) fn enter_pointer_coordinate_context() -> Result<PointerCoordinateContext, String> {
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(
            "pointer_input_failed: Windows per-monitor DPI coordinate context is unavailable"
                .to_string(),
        );
    }
    Ok(PointerCoordinateContext { previous })
}

#[cfg(windows)]
fn windows_virtual_desktop_metrics() -> Result<(i32, i32, u32, u32), String> {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = u32::try_from(unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }).map_err(|_| {
        "pointer_input_failed: Windows virtual desktop width is invalid".to_string()
    })?;
    let height = u32::try_from(unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }).map_err(|_| {
        "pointer_input_failed: Windows virtual desktop height is invalid".to_string()
    })?;
    if width == 0 || height == 0 {
        return Err("pointer_input_failed: Windows virtual desktop geometry is empty".to_string());
    }
    Ok((left, top, width, height))
}

#[cfg(windows)]
fn windows_monitor_rect(monitor: &Monitor) -> Result<(i32, i32, u32, u32), String> {
    let x = monitor
        .x()
        .map_err(|_| "pointer_input_failed: Windows monitor x origin is unavailable".to_string())?;
    let y = monitor
        .y()
        .map_err(|_| "pointer_input_failed: Windows monitor y origin is unavailable".to_string())?;
    let width = monitor
        .width()
        .map_err(|_| "pointer_input_failed: Windows monitor width is unavailable".to_string())?;
    let height = monitor
        .height()
        .map_err(|_| "pointer_input_failed: Windows monitor height is unavailable".to_string())?;
    if width == 0 || height == 0 {
        return Err("pointer_input_failed: Windows monitor geometry is empty".to_string());
    }
    Ok((x, y, width, height))
}

#[cfg(windows)]
fn windows_xcap_virtual_bounds() -> Result<(i32, i32, u32, u32), String> {
    let monitors = windows_monitors()?;
    let mut left = i64::MAX;
    let mut top = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for monitor in monitors {
        let (x, y, width, height) = windows_monitor_rect(&monitor)?;
        let x = i64::from(x);
        let y = i64::from(y);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x.checked_add(i64::from(width)).ok_or_else(|| {
            "pointer_input_failed: Windows monitor right edge overflowed".to_string()
        })?);
        bottom = bottom.max(y.checked_add(i64::from(height)).ok_or_else(|| {
            "pointer_input_failed: Windows monitor bottom edge overflowed".to_string()
        })?);
    }
    if left == i64::MAX || top == i64::MAX || right <= left || bottom <= top {
        return Err("pointer_input_failed: Windows display topology is empty".to_string());
    }
    let width = u32::try_from(right - left).map_err(|_| {
        "pointer_input_failed: Windows display topology width is invalid".to_string()
    })?;
    let height = u32::try_from(bottom - top).map_err(|_| {
        "pointer_input_failed: Windows display topology height is invalid".to_string()
    })?;
    Ok((
        i32::try_from(left).map_err(|_| {
            "pointer_input_failed: Windows topology left edge is invalid".to_string()
        })?,
        i32::try_from(top).map_err(|_| {
            "pointer_input_failed: Windows topology top edge is invalid".to_string()
        })?,
        width,
        height,
    ))
}

#[cfg(windows)]
fn normalize_pointer_axis(offset: u32, extent: u32) -> Result<i32, String> {
    if extent == 0 || offset >= extent {
        return Err(
            "pointer_input_failed: pointer coordinate is outside virtual desktop bounds"
                .to_string(),
        );
    }
    if extent == 1 {
        return Ok(0);
    }
    // A 16-bit normalized axis cannot uniquely address more than 65,536 pixels.
    if extent > 65_536 {
        return Err("pointer_input_failed: virtual desktop axis exceeds exact absolute-input addressability".to_string());
    }
    let denominator = u64::from(extent - 1);
    let normalized = (u64::from(offset) * 65_535 + denominator / 2) / denominator;
    i32::try_from(normalized)
        .map_err(|_| "pointer_input_failed: normalized pointer coordinate is invalid".to_string())
}

#[cfg(windows)]
fn map_windows_pointer_coordinate(
    monitor_x: i32,
    monitor_y: i32,
    source_width: u32,
    source_height: u32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: u32,
    virtual_height: u32,
    x: u32,
    y: u32,
) -> Result<PointerPlan, String> {
    if x >= source_width || y >= source_height {
        return Err(
            "invalid_request: pointer coordinate is outside snapshot source geometry".to_string(),
        );
    }
    let global_x = i64::from(monitor_x)
        .checked_add(i64::from(x))
        .ok_or_else(|| "pointer_input_failed: global x coordinate overflowed".to_string())?;
    let global_y = i64::from(monitor_y)
        .checked_add(i64::from(y))
        .ok_or_else(|| "pointer_input_failed: global y coordinate overflowed".to_string())?;
    let offset_x = global_x - i64::from(virtual_left);
    let offset_y = global_y - i64::from(virtual_top);
    if offset_x < 0
        || offset_y < 0
        || offset_x >= i64::from(virtual_width)
        || offset_y >= i64::from(virtual_height)
    {
        return Err(
            "pointer_input_failed: exact display lies outside Windows virtual desktop bounds"
                .to_string(),
        );
    }
    Ok(PointerPlan {
        global_x: i32::try_from(global_x)
            .map_err(|_| "pointer_input_failed: global x coordinate is invalid".to_string())?,
        global_y: i32::try_from(global_y)
            .map_err(|_| "pointer_input_failed: global y coordinate is invalid".to_string())?,
        normalized_x: normalize_pointer_axis(u32::try_from(offset_x).unwrap(), virtual_width)?,
        normalized_y: normalize_pointer_axis(u32::try_from(offset_y).unwrap(), virtual_height)?,
    })
}

#[cfg(windows)]
fn validate_windows_pointer_state_with(
    action: PointerAction,
    mut is_down: impl FnMut(VIRTUAL_KEY) -> bool,
) -> Result<(), String> {
    for key in [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2] {
        if is_down(key) {
            return Err(
                "pointer_input_failed: shared desktop mouse button is already down".to_string(),
            );
        }
    }
    if action == PointerAction::Click {
        for key in [
            VK_SHIFT,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_CONTROL,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_MENU,
            VK_LMENU,
            VK_RMENU,
            VK_LWIN,
            VK_RWIN,
        ] {
            if is_down(key) {
                return Err(
                    "pointer_input_failed: modifier or Windows key is already down".to_string(),
                );
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_pointer_state(action: PointerAction) -> Result<(), String> {
    validate_windows_pointer_state_with(action, |key| unsafe {
        GetAsyncKeyState(i32::from(key.0)) < 0
    })
}

#[cfg(windows)]
fn validate_windows_pointer_coordinate_spaces(
    virtual_metrics: (i32, i32, u32, u32),
    xcap_bounds: (i32, i32, u32, u32),
) -> Result<(), String> {
    if virtual_metrics == xcap_bounds {
        Ok(())
    } else {
        Err("pointer_input_failed: Windows DPI/topology coordinate spaces cannot be proven identical".to_string())
    }
}

#[cfg(windows)]
pub(super) fn prepare_pointer(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<PointerPlan, String> {
    let monitor = find_exact_display(display)?;
    let (monitor_x, monitor_y, monitor_width, monitor_height) = windows_monitor_rect(&monitor)?;
    if monitor_width != display.width || monitor_height != display.height {
        return Err(
            "stale_display: native display source geometry changed before pointer input"
                .to_string(),
        );
    }
    let virtual_metrics = windows_virtual_desktop_metrics()?;
    let xcap_bounds = windows_xcap_virtual_bounds()?;
    validate_windows_pointer_coordinate_spaces(virtual_metrics, xcap_bounds)?;
    let plan = map_windows_pointer_coordinate(
        monitor_x,
        monitor_y,
        display.width,
        display.height,
        virtual_metrics.0,
        virtual_metrics.1,
        virtual_metrics.2,
        virtual_metrics.3,
        x,
        y,
    )?;
    let fresh = find_exact_display(display)?;
    let fresh_rect = windows_monitor_rect(&fresh)?;
    if fresh_rect != (monitor_x, monitor_y, monitor_width, monitor_height) {
        return Err(
            "stale_display: native display placement changed during pointer preflight".to_string(),
        );
    }
    validate_windows_pointer_state(action)?;
    Ok(plan)
}

#[cfg(windows)]
fn windows_mouse_input(plan: PointerPlan, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: plan.normalized_x,
                dy: plan.normalized_y,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn windows_pointer_move_inputs(plan: PointerPlan) -> [INPUT; 1] {
    let move_flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
    [windows_mouse_input(plan, move_flags)]
}

#[cfg(windows)]
fn windows_pointer_click_button_inputs(plan: PointerPlan) -> [INPUT; 2] {
    [
        windows_mouse_input(plan, MOUSEEVENTF_LEFTDOWN),
        windows_mouse_input(plan, MOUSEEVENTF_LEFTUP),
    ]
}

#[cfg(windows)]
fn validate_windows_pointer_move_send_input_count(inserted: u32) -> Result<(), String> {
    if inserted == 1 {
        Ok(())
    } else if inserted == 0 {
        Err("not_started: Windows pointer move SendInput inserted no events".to_string())
    } else {
        Err(format!(
            "outcome_unknown: Windows pointer move SendInput reported {inserted} inserted events for one prepared move"
        ))
    }
}

#[cfg(windows)]
fn validate_windows_pointer_button_send_input_count(inserted: u32) -> Result<(), String> {
    if inserted == 2 {
        Ok(())
    } else {
        Err(format!(
            "outcome_unknown: Windows pointer click button SendInput inserted {inserted} of 2 events after the exact move"
        ))
    }
}

#[cfg(windows)]
fn validate_windows_pointer_postcondition(
    plan: PointerPlan,
    action: PointerAction,
    cursor_x: i32,
    cursor_y: i32,
    left_button_down: bool,
) -> Result<(), String> {
    if cursor_x != plan.global_x || cursor_y != plan.global_y {
        return Err("outcome_unknown: Windows pointer position postcondition could not prove the exact target".to_string());
    }
    if action == PointerAction::Click && left_button_down {
        return Err(
            "outcome_unknown: Windows left mouse button remained down after click sequence"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn dispatch_windows_pointer_with(
    plan: PointerPlan,
    action: PointerAction,
    mut send_input: impl FnMut(&[INPUT]) -> u32,
    mut cursor_position: impl FnMut() -> Result<(i32, i32), String>,
    mut validate_click_state: impl FnMut() -> Result<(), String>,
    mut left_button_down: impl FnMut() -> bool,
) -> Result<bool, String> {
    let move_inputs = windows_pointer_move_inputs(plan);
    validate_windows_pointer_move_send_input_count(send_input(&move_inputs))?;
    let (cursor_x, cursor_y) = cursor_position()?;
    validate_windows_pointer_postcondition(plan, PointerAction::Move, cursor_x, cursor_y, false)?;
    if action == PointerAction::Move {
        return Ok(true);
    }

    if validate_click_state().is_err() {
        return Err(
            "outcome_unknown: shared desktop input state changed after the exact pointer move; click button events were not attempted"
                .to_string(),
        );
    }
    let button_inputs = windows_pointer_click_button_inputs(plan);
    validate_windows_pointer_button_send_input_count(send_input(&button_inputs))?;
    let (cursor_x, cursor_y) = cursor_position()?;
    validate_windows_pointer_postcondition(
        plan,
        PointerAction::Click,
        cursor_x,
        cursor_y,
        left_button_down(),
    )?;
    Ok(true)
}

#[cfg(windows)]
pub(super) fn dispatch_pointer(plan: PointerPlan, action: PointerAction) -> Result<bool, String> {
    let input_size = std::mem::size_of::<INPUT>() as i32;
    dispatch_windows_pointer_with(
        plan,
        action,
        |inputs| unsafe { SendInput(inputs, input_size) },
        || {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point) }.map_err(|_| {
                "outcome_unknown: Windows cursor position postcondition is unavailable".to_string()
            })?;
            Ok((point.x, point.y))
        },
        || validate_windows_pointer_state(PointerAction::Click),
        || unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0 },
    )
}
#[cfg(target_os = "macos")]
pub(super) fn read_clipboard() -> Result<Value, String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };
    let Some(native_text) = pasteboard.stringForType(string_type) else {
        return clipboard_read_result("macos", None);
    };
    if native_text.len() > super::MAX_CLIPBOARD_TEXT_BYTES {
        return Err(
            "clipboard_too_large: clipboard UTF-8 text exceeds the 16 KiB bound".to_string(),
        );
    }
    let text = native_text.to_string();
    clipboard_read_result("macos", Some(&text))
}

#[cfg(target_os = "macos")]
pub(super) fn write_clipboard(text: &str) -> Result<Value, String> {
    // Complete caller validation and native string/object construction before
    // clearContents crosses the pasteboard mutation boundary.
    let prepared = prepare_clipboard_write_text(text)?;
    let native_text = NSString::from_str(text);
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };

    let effect = run_macos_clipboard_write_effect_steps(
        || pasteboard.clearContents(),
        || pasteboard.setString_forType(&native_text, string_type),
        || pasteboard.changeCount(),
    );
    match effect {
        ClipboardWriteEffectState::Success => Ok(json!({
            "platform": "macos",
            "text_bytes": prepared.text_bytes,
            "success": true,
        })),
        ClipboardWriteEffectState::OutcomeUnknown => Err(
            "outcome_unknown: macOS pasteboard changed after clearContents but the complete NSPasteboardTypeString replacement could not be proven"
                .to_string(),
        ),
        #[cfg(test)]
        ClipboardWriteEffectState::NotStarted => {
            unreachable!("macOS clearContents has no definite native failure result")
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn list_displays(_limit: usize) -> Result<Vec<PlatformDisplay>, String> {
    Err("unsupported_platform: exact full-display observation is unavailable on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn capture_display(_display: &DisplayRecord) -> Result<image::RgbaImage, String> {
    Err("unsupported_platform: exact full-display observation is unavailable on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn prepare_pointer(
    _display: &DisplayRecord,
    _x: u32,
    _y: u32,
    _action: PointerAction,
) -> Result<PointerPlan, String> {
    Err("unsupported_platform: coordinate pointer control is unavailable on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn dispatch_pointer(_plan: PointerPlan, _action: PointerAction) -> Result<bool, String> {
    Err("unsupported_platform: coordinate pointer control is unavailable on macOS".to_string())
}
#[cfg(target_os = "macos")]
pub(super) fn list_applications(_limit: usize) -> Result<Vec<PlatformApplication>, String> {
    Err("unsupported_platform: application discovery is unavailable on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn launch_application(
    _application_id: &str,
    _application: &ApplicationRecord,
) -> Result<Value, String> {
    Err("unsupported_platform: application launch is unavailable on macOS".to_string())
}

#[cfg(target_os = "macos")]
fn ensure_capture_permission() -> Result<(), String> {
    let granted = objc2_core_graphics::CGPreflightScreenCaptureAccess();
    if granted {
        Ok(())
    } else {
        Err("permission_denied: macOS Screen Recording permission is not granted".to_string())
    }
}

#[cfg(windows)]
fn ensure_capture_permission() -> Result<(), String> {
    Ok(())
}

fn map_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("permission") {
        format!("permission_denied: {message}")
    } else {
        format!("capture_failed: {message}")
    }
}

#[cfg(target_os = "macos")]
const MAX_AX_WINDOWS: usize = 64;
#[cfg(target_os = "macos")]
const MAX_AX_CHILD_COUNT: usize = 1_000_000;
#[cfg(target_os = "macos")]
const MAX_AX_ACTION_NAMES: usize = 64;

#[cfg(target_os = "macos")]
fn accessibility_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else {
        format!(
            "accessibility_failed: {operation} failed with AXError({})",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn checked_surface_pid(surface: &SurfaceRecord) -> Result<libc::pid_t, String> {
    libc::pid_t::try_from(surface.pid)
        .map_err(|_| "stale_surface: surface PID exceeds native range".to_string())
}

#[cfg(target_os = "macos")]
fn key_code(key: &str) -> Result<CGKeyCode, String> {
    match key {
        "enter" => Ok(0x24),
        "tab" => Ok(0x30),
        "escape" => Ok(0x35),
        "home" => Ok(0x73),
        "page_up" => Ok(0x74),
        "end" => Ok(0x77),
        "page_down" => Ok(0x79),
        "arrow_left" => Ok(0x7b),
        "arrow_right" => Ok(0x7c),
        "arrow_down" => Ok(0x7d),
        "arrow_up" => Ok(0x7e),
        _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn key_modifier_flags(modifiers: &[String]) -> Result<CGEventFlags, String> {
    validate_key_modifiers(modifiers)?;
    let mut flags = CGEventFlags::empty();
    for modifier in modifiers {
        flags |= match modifier.as_str() {
            "shift" => CGEventFlags::MaskShift,
            "control" => CGEventFlags::MaskControl,
            "option" => CGEventFlags::MaskAlternate,
            "command" => CGEventFlags::MaskCommand,
            _ => unreachable!("validate_key_input closed modifier vocabulary"),
        };
    }
    Ok(flags)
}

#[cfg(all(test, target_os = "macos"))]
mod key_input_native_contract_tests {
    use super::*;

    #[test]
    fn closed_key_codes_and_modifier_flags_are_stable() {
        for (key, expected) in [
            ("enter", 0x24),
            ("tab", 0x30),
            ("escape", 0x35),
            ("home", 0x73),
            ("page_up", 0x74),
            ("end", 0x77),
            ("page_down", 0x79),
            ("arrow_left", 0x7b),
            ("arrow_right", 0x7c),
            ("arrow_down", 0x7d),
            ("arrow_up", 0x7e),
        ] {
            assert_eq!(key_code(key).unwrap(), expected, "{key}");
        }
        assert!(key_code("a").is_err());

        let flags = key_modifier_flags(&["shift".to_string(), "command".to_string()]).unwrap();
        assert!(flags.contains(CGEventFlags::MaskShift));
        assert!(flags.contains(CGEventFlags::MaskCommand));
        assert!(!flags.contains(CGEventFlags::MaskAlternate));

        let mut surface = SurfaceRecord {
            native_id: 1,
            pid: 1,
            identity_hash: [0; 32],
            application: "test".to_string(),
            title: "test".to_string(),
            width: 1,
            height: 1,
        };
        assert_eq!(checked_surface_pid(&surface).unwrap(), 1);
        surface.pid = u32::MAX;
        assert!(checked_surface_pid(&surface)
            .unwrap_err()
            .starts_with("stale_surface:"));
    }
}

#[cfg(target_os = "macos")]
fn control_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "control_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn scroll_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "scroll_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(all(test, target_os = "macos"))]
mod scroll_attempt_tests {
    use super::*;

    #[test]
    fn rejected_scroll_action_is_definite_but_unclassified_native_error_is_unknown() {
        let rejected = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::ActionUnsupported,
        );
        assert!(rejected.starts_with("scroll_failed:"), "{rejected}");

        let uncertain = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::NoValue,
        );
        assert!(uncertain.starts_with("outcome_unknown:"), "{uncertain}");
    }
}

#[cfg(target_os = "macos")]
fn window_activation_attempt_error(
    operation: &str,
    error: AXError,
    prior_effect_succeeded: bool,
) -> String {
    if prior_effect_succeeded {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after application activation had already succeeded",
            error.0
        )
    } else {
        control_attempt_error(operation, error)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod window_activation_tests {
    use super::*;

    #[test]
    fn partial_window_activation_failure_is_always_outcome_unknown() {
        let partial = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            true,
        );
        assert!(partial.starts_with("outcome_unknown:"), "{partial}");

        let not_started = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            false,
        );
        assert!(not_started.starts_with("control_failed:"), "{not_started}");
    }
}

#[cfg(target_os = "macos")]
fn text_input_attempt_error(error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "input_failed: AXUIElementSetAttributeValue(AXValue) was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: AXUIElementSetAttributeValue(AXValue) returned AXError({}) after the native text write was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn prepare_ax_call(deadline: &AxObservationDeadline, element: &AXUIElement) -> Result<(), String> {
    let timeout_secs = deadline.remaining_timeout_secs()?;
    let error = unsafe { element.set_messaging_timeout(timeout_secs) };
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementSetMessagingTimeout", error));
    }
    deadline.ensure_remaining()
}

#[cfg(target_os = "macos")]
fn optional_ax_value(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CFRetained<CFType>>, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut raw: *const CFType = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
                "accessibility_failed: AX attribute succeeded with null value".to_string()
            })?;
            Ok(Some(unsafe { CFRetained::from_raw(raw) }))
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(None),
        error => Err(accessibility_error("AXUIElementCopyAttributeValue", error)),
    }
}

#[cfg(target_os = "macos")]
fn optional_ax_string(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<String>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFString>()
        .ok()
        .map(|value| bounded_text(&value.to_string())))
}

#[cfg(target_os = "macos")]
fn element_fingerprint(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    inherited_protected: bool,
) -> Result<ElementFingerprint, String> {
    let role = optional_ax_string(deadline, element, "AXRole")?
        .ok_or_else(|| "accessibility_failed: AX element is missing a string role".to_string())?;
    let protected = inherited_protected
        || optional_ax_bool(deadline, element, "AXProtectedContent")?.unwrap_or(false);
    Ok(ElementFingerprint {
        role,
        subrole: optional_ax_string(deadline, element, "AXSubrole")?,
        identifier: optional_ax_string(deadline, element, "AXIdentifier")?,
        title: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXTitle")?
        },
        description: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXDescription")?
        },
        placeholder: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXPlaceholderValue")?
        },
        protected,
    })
}

#[cfg(target_os = "macos")]
fn optional_ax_bool(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<bool>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFBoolean>()
        .ok()
        .map(|value| value.value()))
}

#[cfg(target_os = "macos")]
fn optional_ax_point(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGPoint>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGPoint {
        return Ok(None);
    }
    let mut point = CGPoint::ZERO;
    let copied = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    Ok(copied.then_some(point))
}

#[cfg(target_os = "macos")]
fn optional_ax_size(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGSize>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGSize {
        return Ok(None);
    }
    let mut size = CGSize::ZERO;
    let copied = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    Ok(copied.then_some(size))
}

#[cfg(target_os = "macos")]
fn ax_window_geometry_matches(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<bool, String> {
    const TOLERANCE: f64 = 2.0;
    let Some(position) = optional_ax_point(deadline, element, "AXPosition")? else {
        return Ok(false);
    };
    let Some(size) = optional_ax_size(deadline, element, "AXSize")? else {
        return Ok(false);
    };
    Ok((position.x - f64::from(x)).abs() <= TOLERANCE
        && (position.y - f64::from(y)).abs() <= TOLERANCE
        && (size.width - f64::from(width)).abs() <= TOLERANCE
        && (size.height - f64::from(height)).abs() <= TOLERANCE)
}

#[cfg(target_os = "macos")]
fn ax_array_count(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<usize, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut count: CFIndex = 0;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.attribute_value_count(&attribute, NonNull::from(&mut count)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let count = usize::try_from(count).map_err(|_| {
                "accessibility_failed: AX array count is negative or too large".to_string()
            })?;
            if count > MAX_AX_CHILD_COUNT {
                return Err(
                    "accessibility_failed: AX child count exceeds bounded inspection limit"
                        .to_string(),
                );
            }
            Ok(count)
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(0),
        error => Err(accessibility_error(
            "AXUIElementGetAttributeValueCount",
            error,
        )),
    }
}

#[cfg(target_os = "macos")]
fn ax_elements(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    count: usize,
) -> Result<Vec<CFRetained<AXUIElement>>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let attribute = CFString::from_static_str(attribute_name);
    let max_values = CFIndex::try_from(count)
        .map_err(|_| "accessibility_failed: AX array request exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe {
        element.copy_attribute_values(&attribute, 0, max_values, NonNull::from(&mut raw))
    };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX array copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    let mut output = Vec::with_capacity(array.len());
    for value in array.iter() {
        let element = value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AX element array contained a non-element value".to_string()
        })?;
        output.push(element);
    }
    Ok(output)
}

#[cfg(target_os = "macos")]
fn ax_supports_action(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    expected_action: &'static str,
) -> Result<bool, String> {
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_action_names(NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyActionNames", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX action-name copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() > MAX_AX_ACTION_NAMES {
        return Err(
            "accessibility_failed: AX action-name list exceeds bounded inspection limit"
                .to_string(),
        );
    }
    let expected_utf16_len = expected_action.encode_utf16().count();
    for value in array.iter() {
        let action = value.downcast::<CFString>().map_err(|_| {
            "accessibility_failed: AX action-name array contained a non-string value".to_string()
        })?;
        let action_len = usize::try_from(action.length()).map_err(|_| {
            "accessibility_failed: AX action name has invalid string length".to_string()
        })?;
        if action_len == expected_utf16_len && action.to_string() == expected_action {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn ax_attribute_settable(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
) -> Result<bool, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let mut settable = 0u8;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.is_attribute_settable(&attribute, NonNull::from(&mut settable)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => Ok(settable != 0),
        AXError::AttributeUnsupported => Ok(false),
        error => Err(accessibility_error("AXUIElementIsAttributeSettable", error)),
    }
}

#[cfg(target_os = "macos")]
fn ax_element_at(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    index: usize,
) -> Result<CFRetained<AXUIElement>, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let index = CFIndex::try_from(index)
        .map_err(|_| "stale_element: AX child index exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error =
        unsafe { element.copy_attribute_values(&attribute, index, 1, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut())
        .ok_or_else(|| "stale_element: AX child lookup returned null".to_string())?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() != 1 {
        return Err("stale_element: AX child path no longer resolves exactly".to_string());
    }
    array
        .iter()
        .next()
        .expect("single AX child")
        .downcast::<AXUIElement>()
        .map_err(|_| "stale_element: AX child path resolved to a non-element value".to_string())
}

#[cfg(any(target_os = "macos", windows))]
fn resolve_surface_window(surface: &SurfaceRecord) -> Result<Window, String> {
    let window = Window::all()
        .map_err(map_error)?
        .into_iter()
        .find(|window| window.id().ok() == Some(surface.native_id))
        .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
    let width = window.width().map_err(map_error)?;
    let height = window.height().map_err(map_error)?;
    let application = window.app_name().map_err(map_error)?;
    let title = window.title().map_err(map_error)?;
    let pid = window.pid().map_err(map_error)?;
    let current_identity = identity_hash(&window, &application, &title, width, height)?;
    if pid != surface.pid || current_identity != surface.identity_hash {
        return Err("stale_surface: window identity changed since enumeration".to_string());
    }
    if bounded_text(&application) != surface.application || bounded_text(&title) != surface.title {
        return Err("stale_surface: window metadata changed since enumeration".to_string());
    }
    Ok(window)
}

#[cfg(target_os = "macos")]
fn exact_ax_window(
    surface: &SurfaceRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    let native_window = resolve_surface_window(surface)?;
    deadline.ensure_remaining()?;
    let x = native_window.x().map_err(map_error)?;
    let y = native_window.y().map_err(map_error)?;
    let width = native_window.width().map_err(map_error)?;
    let height = native_window.height().map_err(map_error)?;
    let pid = checked_surface_pid(surface)?;
    let application = unsafe { AXUIElement::new_application(pid) };
    let window_count = ax_array_count(deadline, &application, "AXWindows")?;
    if window_count == 0 || window_count > MAX_AX_WINDOWS {
        return Err(
            "accessibility_failed: exact AX window cannot be resolved within the bounded window set"
                .to_string(),
        );
    }
    let mut windows = ax_elements(deadline, &application, "AXWindows", window_count)?;
    let mut geometry_matches = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        if ax_window_geometry_matches(deadline, window, x, y, width, height)? {
            geometry_matches.push(index);
        }
    }
    if !geometry_matches.is_empty() {
        let index = select_exact_ax_window_index(&geometry_matches, &[], windows.len())?;
        return Ok(windows.swap_remove(index));
    }

    let mut title_matches = Vec::new();
    if !surface.title.is_empty() {
        for (index, window) in windows.iter().enumerate() {
            if optional_ax_string(deadline, window, "AXTitle")?
                .is_some_and(|title| bounded_text(&title) == surface.title)
            {
                title_matches.push(index);
            }
        }
    }
    let index = select_exact_ax_window_index(&geometry_matches, &title_matches, windows.len())?;
    Ok(windows.swap_remove(index))
}

#[cfg(target_os = "macos")]
pub(super) fn accessibility_status() -> Result<Value, String> {
    Ok(json!({
        "platform": "macos",
        "trusted": unsafe { AXIsProcessTrusted() },
    }))
}

#[cfg(windows)]
const UIA_CONNECTION_TIMEOUT_MS: u32 = 2_000;
#[cfg(windows)]
const UIA_TRANSACTION_TIMEOUT_MS: u32 = 2_000;
#[cfg(windows)]
const UIA_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(windows)]
const UIA_OBSERVATION_TIMEOUT_ERROR: &str =
    "accessibility_failed: Windows UI Automation observation deadline exceeded";
#[cfg(windows)]
const MAX_UIA_RUNTIME_ID_ELEMENTS: usize = 64;
#[cfg(windows)]
const MAX_UIA_FOCUS_ANCESTORS: usize = 64;

#[cfg(windows)]
struct UiaObservationDeadline {
    expires_at: Instant,
}

#[cfg(windows)]
impl UiaObservationDeadline {
    fn new() -> Self {
        Self {
            expires_at: Instant::now() + UIA_OBSERVATION_TIMEOUT,
        }
    }

    fn ensure_remaining(&self) -> Result<(), String> {
        self.expires_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .map(|_| ())
            .ok_or_else(|| UIA_OBSERVATION_TIMEOUT_ERROR.to_string())
    }
}

#[cfg(windows)]
struct ComInitialization {
    uninitialize: bool,
}

#[cfg(windows)]
impl ComInitialization {
    fn new() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self { uninitialize: true })
        } else if result == RPC_E_CHANGED_MODE {
            // The Runner thread is already initialized in another apartment.
            // UI Automation supports either apartment; do not uninitialize an
            // apartment established by another subsystem.
            Ok(Self {
                uninitialize: false,
            })
        } else {
            Err(format!(
                "accessibility_failed: CoInitializeEx failed with HRESULT(0x{:08X})",
                result.0 as u32
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
struct UiaContext {
    // COM interfaces must be released before the matching CoUninitialize.
    // Rust drops struct fields in declaration order, so keep the guard last.
    automation: IUIAutomation2,
    walker: IUIAutomationTreeWalker,
    deadline: UiaObservationDeadline,
    _com: ComInitialization,
}

#[cfg(windows)]
impl UiaContext {
    fn new() -> Result<Self, String> {
        let com = ComInitialization::new()?;
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .map_err(|error| uia_error("CoCreateInstance(CUIAutomation8)", &error))?;
        unsafe { automation.SetConnectionTimeout(UIA_CONNECTION_TIMEOUT_MS) }
            .map_err(|error| uia_error("IUIAutomation2::SetConnectionTimeout", &error))?;
        unsafe { automation.SetTransactionTimeout(UIA_TRANSACTION_TIMEOUT_MS) }
            .map_err(|error| uia_error("IUIAutomation2::SetTransactionTimeout", &error))?;
        let walker = unsafe { automation.ControlViewWalker() }
            .map_err(|error| uia_error("IUIAutomation::ControlViewWalker", &error))?;
        Ok(Self {
            automation,
            walker,
            deadline: UiaObservationDeadline::new(),
            _com: com,
        })
    }
}

#[cfg(windows)]
struct OwnedSafeArray(NonNull<SAFEARRAY>);

#[cfg(windows)]
impl OwnedSafeArray {
    fn new(array: *mut SAFEARRAY) -> Result<Self, String> {
        NonNull::new(array)
            .map(Self)
            .ok_or_else(|| "stale_element: UI Automation runtime id is missing".to_string())
    }

    fn as_ptr(&self) -> *mut SAFEARRAY {
        self.0.as_ptr()
    }
}

#[cfg(windows)]
impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.as_ptr());
        }
    }
}

#[cfg(windows)]
fn uia_error(operation: &str, error: &windows::core::Error) -> String {
    format!(
        "accessibility_failed: {operation} failed with HRESULT(0x{:08X})",
        error.code().0 as u32
    )
}

#[cfg(windows)]
fn uia_error_code(error: &windows::core::Error) -> u32 {
    error.code().0 as u32
}

#[cfg(windows)]
fn optional_uia_element(
    result: windows::core::Result<IUIAutomationElement>,
    operation: &str,
) -> Result<Option<IUIAutomationElement>, String> {
    match result {
        Ok(element) => Ok(Some(element)),
        // UIA tree-walker APIs use S_OK + a null interface for end-of-list.
        // windows-rs represents that nullable success as Error::empty() (S_OK).
        Err(error) if error.code().is_ok() || error.code() == E_POINTER => Ok(None),
        Err(error) => Err(uia_error(operation, &error)),
    }
}

#[cfg(windows)]
fn optional_uia_pattern<T: Interface>(
    context: &UiaContext,
    element: &IUIAutomationElement,
    pattern: UIA_PATTERN_ID,
) -> Result<Option<T>, String> {
    context.deadline.ensure_remaining()?;
    match unsafe { element.GetCurrentPatternAs::<T>(pattern) } {
        Ok(pattern) => Ok(Some(pattern)),
        Err(error)
            if error.code().is_ok()
                || error.code() == E_NOINTERFACE
                || error.code() == E_POINTER
                || uia_error_code(&error) == UIA_E_NOTSUPPORTED =>
        {
            Ok(None)
        }
        Err(error) if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE => {
            Err("stale_element: UI Automation element is no longer available".to_string())
        }
        Err(error) => Err(uia_error(
            "IUIAutomationElement::GetCurrentPatternAs",
            &error,
        )),
    }
}

#[cfg(windows)]
fn windows_key_mapping(key: &str) -> Result<(VIRTUAL_KEY, bool), String> {
    match key {
        "enter" => Ok((VK_RETURN, false)),
        "escape" => Ok((VK_ESCAPE, false)),
        "tab" => Ok((VK_TAB, false)),
        "arrow_up" => Ok((VK_UP, true)),
        "arrow_down" => Ok((VK_DOWN, true)),
        "arrow_left" => Ok((VK_LEFT, true)),
        "arrow_right" => Ok((VK_RIGHT, true)),
        "page_up" => Ok((VK_PRIOR, true)),
        "page_down" => Ok((VK_NEXT, true)),
        "home" => Ok((VK_HOME, true)),
        "end" => Ok((VK_END, true)),
        _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
    }
}

#[cfg(windows)]
fn windows_modifier_key(modifier: &str) -> Result<VIRTUAL_KEY, String> {
    match modifier {
        "shift" => Ok(VK_SHIFT),
        "control" => Ok(VK_CONTROL),
        "option" => Ok(VK_MENU),
        "command" => Err(
            "key_input_failed: command modifier has no safe Windows mapping in this closed input slice"
                .to_string(),
        ),
        _ => Err("invalid_request: computer key input modifier is outside the closed vocabulary".to_string()),
    }
}

#[cfg(windows)]
fn validate_windows_key_input_chord(key: &str, modifiers: &[String]) -> Result<(), String> {
    let has_option = modifiers.iter().any(|modifier| modifier == "option");
    let has_control = modifiers.iter().any(|modifier| modifier == "control");
    let escapes_exact_surface =
        (has_option && matches!(key, "tab" | "escape")) || (has_control && key == "escape");
    if escapes_exact_surface {
        return Err(
            "key_input_failed: Windows system-level key chord is outside the exact-surface input contract"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_keyboard_state_with<F>(key: &str, mut is_down: F) -> Result<(), String>
where
    F: FnMut(VIRTUAL_KEY) -> bool,
{
    let (key_code, _) = windows_key_mapping(key)?;
    for candidate in [
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
        key_code,
    ] {
        if is_down(candidate) {
            return Err(
                "key_input_failed: Windows keyboard state is not neutral; release held Shift/Control/Alt/Windows/target keys and re-observe before retrying"
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_keyboard_state(key: &str) -> Result<(), String> {
    validate_windows_keyboard_state_with(key, |candidate| unsafe {
        GetAsyncKeyState(i32::from(candidate.0)) < 0
    })
}

#[cfg(windows)]
fn windows_keyboard_input(key: VIRTUAL_KEY, key_up: bool, extended: bool) -> INPUT {
    let flags = KEYBD_EVENT_FLAGS(
        if extended { KEYEVENTF_EXTENDEDKEY.0 } else { 0 }
            | if key_up { KEYEVENTF_KEYUP.0 } else { 0 },
    );
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn windows_key_input_plan(key: &str, modifiers: &[String]) -> Result<Vec<INPUT>, String> {
    validate_key_input(key, modifiers)?;
    let (key_code, key_extended) = windows_key_mapping(key)?;
    let mut modifier_keys = Vec::with_capacity(modifiers.len());
    for modifier in modifiers {
        modifier_keys.push(windows_modifier_key(modifier)?);
    }
    validate_windows_key_input_chord(key, modifiers)?;

    let mut inputs = Vec::with_capacity(modifier_keys.len() * 2 + 2);
    for modifier in &modifier_keys {
        inputs.push(windows_keyboard_input(*modifier, false, false));
    }
    inputs.push(windows_keyboard_input(key_code, false, key_extended));
    inputs.push(windows_keyboard_input(key_code, true, key_extended));
    for modifier in modifier_keys.iter().rev() {
        inputs.push(windows_keyboard_input(*modifier, true, false));
    }
    Ok(inputs)
}

#[cfg(windows)]
fn uia_element_has_exact_focus(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<bool, String> {
    context.deadline.ensure_remaining()?;
    let Some(focused) = optional_uia_element(
        unsafe { context.automation.GetFocusedElement() },
        "IUIAutomation::GetFocusedElement",
    )?
    else {
        return Ok(false);
    };
    context.deadline.ensure_remaining()?;
    unsafe { context.automation.CompareElements(element, &focused) }
        .map(|same| same.as_bool())
        .map_err(|error| uia_error("IUIAutomation::CompareElements", &error))
}

#[cfg(windows)]
fn uia_string(
    result: windows::core::Result<windows::core::BSTR>,
    operation: &str,
) -> Result<Option<String>, String> {
    let value = result.map_err(|error| uia_error(operation, &error))?;
    let value = bounded_text(&value.to_string());
    Ok((!value.is_empty()).then_some(value))
}

#[cfg(windows)]
pub(super) fn uia_control_role(control_type: UIA_CONTROLTYPE_ID) -> String {
    let role = if control_type == UIA_WindowControlTypeId {
        Some("AXWindow")
    } else if control_type == UIA_ButtonControlTypeId {
        Some("AXButton")
    } else if control_type == UIA_EditControlTypeId {
        Some("AXTextField")
    } else if control_type == UIA_DocumentControlTypeId {
        Some("AXTextArea")
    } else if control_type == UIA_HyperlinkControlTypeId {
        Some("AXLink")
    } else if control_type == UIA_CheckBoxControlTypeId {
        Some("AXCheckBox")
    } else if control_type == UIA_RadioButtonControlTypeId {
        Some("AXRadioButton")
    } else if control_type == UIA_ComboBoxControlTypeId {
        Some("AXComboBox")
    } else if control_type == UIA_ListControlTypeId {
        Some("AXList")
    } else if control_type == UIA_ListItemControlTypeId
        || control_type == UIA_TreeItemControlTypeId
        || control_type == UIA_DataItemControlTypeId
    {
        Some("AXRow")
    } else if control_type == UIA_MenuControlTypeId {
        Some("AXMenu")
    } else if control_type == UIA_MenuItemControlTypeId {
        Some("AXMenuItem")
    } else if control_type == UIA_TreeControlTypeId {
        Some("AXOutline")
    } else if control_type == UIA_TabControlTypeId {
        Some("AXTabGroup")
    } else if control_type == UIA_TabItemControlTypeId {
        Some("AXRadioButton")
    } else if control_type == UIA_TextControlTypeId {
        Some("AXStaticText")
    } else if control_type == UIA_TableControlTypeId || control_type == UIA_DataGridControlTypeId {
        Some("AXTable")
    } else if control_type == UIA_ToolBarControlTypeId {
        Some("AXToolbar")
    } else if control_type == UIA_ScrollBarControlTypeId {
        Some("AXScrollBar")
    } else if control_type == UIA_SliderControlTypeId {
        Some("AXSlider")
    } else if control_type == UIA_SpinnerControlTypeId {
        Some("AXIncrementor")
    } else if control_type == UIA_ProgressBarControlTypeId {
        Some("AXProgressIndicator")
    } else if control_type == UIA_HeaderItemControlTypeId {
        Some("AXColumn")
    } else if control_type == UIA_PaneControlTypeId
        || control_type == UIA_GroupControlTypeId
        || control_type == UIA_CustomControlTypeId
        || control_type == UIA_HeaderControlTypeId
        || control_type == UIA_StatusBarControlTypeId
        || control_type == UIA_SeparatorControlTypeId
        || control_type == UIA_ToolTipControlTypeId
    {
        Some("AXGroup")
    } else {
        None
    };
    role.map(str::to_string)
        .unwrap_or_else(|| format!("UIAControlType({})", control_type.0))
}

#[cfg(windows)]
pub(super) fn uia_semantic_focus_role(role: &str) -> bool {
    role == "AXTextField"
}

#[cfg(windows)]
pub(super) fn uia_semantic_text_input_role(role: &str) -> bool {
    role == "AXTextField"
}

#[cfg(windows)]
fn uia_runtime_id(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Vec<i32>, String> {
    context.deadline.ensure_remaining()?;
    let array = unsafe { element.GetRuntimeId() }.map_err(|error| {
        if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE {
            "stale_element: UI Automation element is no longer available".to_string()
        } else {
            uia_error("IUIAutomationElement::GetRuntimeId", &error)
        }
    })?;
    let array = OwnedSafeArray::new(array)?;
    if unsafe { SafeArrayGetDim(array.as_ptr()) } != 1
        || unsafe { SafeArrayGetElemsize(array.as_ptr()) } != std::mem::size_of::<i32>() as u32
    {
        return Err(
            "stale_element: UI Automation runtime id has an invalid SAFEARRAY shape".to_string(),
        );
    }
    let lower = unsafe { SafeArrayGetLBound(array.as_ptr(), 1) }
        .map_err(|error| uia_error("SafeArrayGetLBound(runtime id)", &error))?;
    let upper = unsafe { SafeArrayGetUBound(array.as_ptr(), 1) }
        .map_err(|error| uia_error("SafeArrayGetUBound(runtime id)", &error))?;
    let length = upper
        .checked_sub(lower)
        .and_then(|span| span.checked_add(1))
        .and_then(|length| usize::try_from(length).ok())
        .filter(|length| (1..=MAX_UIA_RUNTIME_ID_ELEMENTS).contains(length))
        .ok_or_else(|| {
            "stale_element: UI Automation runtime id length is invalid or exceeds the bound"
                .to_string()
        })?;
    let mut runtime_id = Vec::with_capacity(length);
    for offset in 0..length {
        context.deadline.ensure_remaining()?;
        let index = lower
            .checked_add(i32::try_from(offset).map_err(|_| {
                "stale_element: UI Automation runtime id index exceeds the bound".to_string()
            })?)
            .ok_or_else(|| "stale_element: UI Automation runtime id index overflow".to_string())?;
        let mut value = 0i32;
        unsafe {
            SafeArrayGetElement(
                array.as_ptr(),
                &index,
                (&mut value as *mut i32).cast::<std::ffi::c_void>(),
            )
        }
        .map_err(|error| uia_error("SafeArrayGetElement(runtime id)", &error))?;
        runtime_id.push(value);
    }
    Ok(runtime_id)
}

#[cfg(windows)]
fn uia_fingerprint(
    context: &UiaContext,
    element: &IUIAutomationElement,
    inherited_protected: bool,
) -> Result<ElementFingerprint, String> {
    context.deadline.ensure_remaining()?;
    let control_type = unsafe { element.CurrentControlType() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
    context.deadline.ensure_remaining()?;
    let is_password = unsafe { element.CurrentIsPassword() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsPassword", &error))?
        .as_bool();
    let protected = inherited_protected || is_password;
    let native_runtime_id = uia_runtime_id(context, element)?;
    context.deadline.ensure_remaining()?;
    let identifier = uia_string(
        unsafe { element.CurrentAutomationId() },
        "IUIAutomationElement::CurrentAutomationId",
    )?;
    let title = if protected {
        None
    } else {
        context.deadline.ensure_remaining()?;
        uia_string(
            unsafe { element.CurrentName() },
            "IUIAutomationElement::CurrentName",
        )?
    };
    let description = if protected {
        None
    } else {
        context.deadline.ensure_remaining()?;
        uia_string(
            unsafe { element.CurrentHelpText() },
            "IUIAutomationElement::CurrentHelpText",
        )?
    };
    Ok(ElementFingerprint {
        role: uia_control_role(control_type),
        native_runtime_id,
        subrole: None,
        identifier,
        title,
        description,
        placeholder: None,
        protected,
    })
}

#[cfg(windows)]
fn uia_text_pattern(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Option<IUIAutomationValuePattern>, String> {
    optional_uia_pattern::<IUIAutomationValuePattern>(context, element, UIA_ValuePatternId)
}

#[cfg(windows)]
fn uia_value_pattern_current_value(
    context: &UiaContext,
    pattern: &IUIAutomationValuePattern,
) -> Result<String, String> {
    context.deadline.ensure_remaining()?;
    uia_string(
        unsafe { pattern.CurrentValue() },
        "IUIAutomationValuePattern::CurrentValue",
    )
    .map(|value| value.unwrap_or_default())
}

#[cfg(windows)]
fn uia_value_pattern_writable(
    context: &UiaContext,
    pattern: &IUIAutomationValuePattern,
) -> Result<bool, String> {
    context.deadline.ensure_remaining()?;
    unsafe { pattern.CurrentIsReadOnly() }
        .map(|read_only| !read_only.as_bool())
        .map_err(|error| uia_error("IUIAutomationValuePattern::CurrentIsReadOnly", &error))
}

#[cfg(windows)]
fn uia_text_value(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Option<String>, String> {
    let Some(pattern) = uia_text_pattern(context, element)? else {
        return Ok(None);
    };
    uia_value_pattern_current_value(context, &pattern).map(Some)
}

#[cfg(windows)]
fn uia_children(
    context: &UiaContext,
    element: &IUIAutomationElement,
    limit: usize,
) -> Result<(Vec<IUIAutomationElement>, bool), String> {
    let mut output = Vec::with_capacity(limit.min(32));
    context.deadline.ensure_remaining()?;
    let mut current = optional_uia_element(
        unsafe { context.walker.GetFirstChildElement(element) },
        "IUIAutomationTreeWalker::GetFirstChildElement",
    )?;
    while let Some(element) = current {
        if output.len() >= limit {
            return Ok((output, true));
        }
        context.deadline.ensure_remaining()?;
        current = optional_uia_element(
            unsafe { context.walker.GetNextSiblingElement(&element) },
            "IUIAutomationTreeWalker::GetNextSiblingElement",
        )?;
        output.push(element);
    }
    Ok((output, false))
}

#[cfg(windows)]
pub(super) fn win_hwnd(native_id: u32) -> Result<WinHwnd, String> {
    let hwnd = WinHwnd(native_id as i32 as isize as *mut std::ffi::c_void);
    if hwnd.0.is_null() {
        Err("stale_surface: window handle is invalid".to_string())
    } else {
        Ok(hwnd)
    }
}

#[cfg(windows)]
fn exact_uia_window(
    context: &UiaContext,
    surface: &SurfaceRecord,
) -> Result<IUIAutomationElement, String> {
    let _window = resolve_surface_window(surface)?;
    let hwnd = win_hwnd(surface.native_id)?;
    context.deadline.ensure_remaining()?;
    let root = unsafe { context.automation.ElementFromHandle(hwnd) }
        .map_err(|error| uia_error("IUIAutomation::ElementFromHandle", &error))?;
    context.deadline.ensure_remaining()?;
    let current_hwnd = unsafe { root.CurrentNativeWindowHandle() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentNativeWindowHandle", &error))?;
    context.deadline.ensure_remaining()?;
    let current_pid = unsafe { root.CurrentProcessId() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentProcessId", &error))?;
    let current_pid = u32::try_from(current_pid)
        .map_err(|_| "stale_surface: UI Automation process id is invalid".to_string())?;
    if current_hwnd != hwnd || current_pid != surface.pid {
        return Err(
            "stale_surface: UI Automation root no longer matches the observed HWND/PID".to_string(),
        );
    }
    Ok(root)
}

#[cfg(windows)]
fn validate_uia_focused_element_root(
    context: &UiaContext,
    root: &IUIAutomationElement,
) -> Result<(), String> {
    context.deadline.ensure_remaining()?;
    let focused = optional_uia_element(
        unsafe { context.automation.GetFocusedElement() },
        "IUIAutomation::GetFocusedElement",
    )?
    .ok_or_else(|| "key_input_failed: Windows UI Automation has no focused element".to_string())?;
    let mut current = focused;

    for depth in 0..=MAX_UIA_FOCUS_ANCESTORS {
        context.deadline.ensure_remaining()?;
        let password = unsafe { current.CurrentIsPassword() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsPassword", &error))?
            .as_bool();
        if password {
            return Err(
                "permission_denied: protected or password UI Automation content cannot receive key input"
                    .to_string(),
            );
        }

        context.deadline.ensure_remaining()?;
        let same_root = unsafe { context.automation.CompareElements(root, &current) }
            .map_err(|error| uia_error("IUIAutomation::CompareElements", &error))?
            .as_bool();
        if same_root {
            return Ok(());
        }
        if depth == MAX_UIA_FOCUS_ANCESTORS {
            break;
        }

        context.deadline.ensure_remaining()?;
        current = optional_uia_element(
            unsafe { context.walker.GetParentElement(&current) },
            "IUIAutomationTreeWalker::GetParentElement",
        )?
        .ok_or_else(|| {
            "key_input_failed: focused UI Automation element is outside the exact window root"
                .to_string()
        })?;
    }

    Err(
        "key_input_failed: focused UI Automation ancestry exceeds the bounded exact-window check"
            .to_string(),
    )
}

#[cfg(windows)]
fn validate_windows_key_input_target(
    context: &UiaContext,
    surface: &SurfaceRecord,
    root: &IUIAutomationElement,
) -> Result<(), String> {
    let hwnd = win_hwnd(surface.native_id)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "key_input_failed: exact Windows surface must already be the foreground window"
                .to_string(),
        );
    }
    validate_uia_focused_element_root(context, root)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "key_input_failed: exact Windows surface lost foreground during UI Automation focus preflight"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn resolve_uia_element(
    context: &UiaContext,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<IUIAutomationElement, String> {
    if element.lineage.len() != element.path.len() + 1 {
        return Err("stale_element: UIA element correlation lineage is incomplete".to_string());
    }
    let mut current = exact_uia_window(context, surface)?;
    let current_root = uia_fingerprint(context, &current, false)?;
    if current_root != element.lineage[0] {
        return Err("stale_element: UIA root identity changed since observation".to_string());
    }
    for (depth, &index) in element.path.iter().enumerate() {
        let (children, has_more) = uia_children(context, &current, index + 1)?;
        if children.len() <= index {
            return Err("stale_element: UIA child path no longer exists".to_string());
        }
        if has_more && index >= super::MAX_ACCESSIBILITY_NODES {
            return Err("stale_element: UIA child path exceeds bounded correlation".to_string());
        }
        current = children[index].clone();
        let current_fingerprint =
            uia_fingerprint(context, &current, element.lineage[depth].protected)?;
        if current_fingerprint != element.lineage[depth + 1] {
            return Err("stale_element: UIA element lineage changed since observation".to_string());
        }
    }
    Ok(current)
}

#[cfg(windows)]
pub(super) fn accessibility_status() -> Result<Value, String> {
    let _context = UiaContext::new()?;
    Ok(json!({"platform": "windows", "trusted": true}))
}

#[cfg(target_os = "macos")]
pub(super) fn accessibility_tree(
    surface_id: &str,
    surface: &SurfaceRecord,
    max_depth: usize,
    max_nodes: usize,
) -> Result<AccessibilityTreeResult, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let deadline = AxObservationDeadline::new();
    let root = exact_ax_window(surface, &deadline)?;
    let mut queue = VecDeque::from([(
        root,
        None::<String>,
        0usize,
        Vec::<usize>::new(),
        Vec::<ElementFingerprint>::new(),
        false,
    )]);
    let mut nodes = Vec::with_capacity(max_nodes.min(64));
    let mut elements = Vec::with_capacity(max_nodes.min(64));
    let mut truncated = false;
    while let Some((element, parent_element_id, depth, path, mut lineage, inherited_protected)) =
        queue.pop_front()
    {
        deadline.ensure_remaining()?;
        if nodes.len() >= max_nodes {
            truncated = true;
            break;
        }
        let element_id = format!("element_{}", Uuid::new_v4().simple());
        let fingerprint = element_fingerprint(&deadline, &element, inherited_protected)?;
        let role = fingerprint.role.clone();
        let subrole = fingerprint.subrole.clone();
        let title = fingerprint.title.clone();
        let description = fingerprint.description.clone();
        let placeholder = fingerprint.placeholder.clone();
        let protected = fingerprint.protected;
        lineage.push(fingerprint);
        let sensitive = role == "AXSecureTextField"
            || subrole
                .as_deref()
                .is_some_and(|value| value.contains("Secure"));
        let value = if sensitive || protected {
            None
        } else {
            optional_ax_string(&deadline, &element, "AXValue")?
        };
        let enabled = optional_ax_bool(&deadline, &element, "AXEnabled")?;
        let focused = optional_ax_bool(&deadline, &element, "AXFocused")?;
        let child_count = ax_array_count(&deadline, &element, "AXChildren")?;
        if depth < max_depth && child_count > 0 {
            let reserved = nodes.len() + queue.len() + 1;
            let remaining = max_nodes.saturating_sub(reserved);
            let take = child_count.min(remaining);
            if take < child_count {
                truncated = true;
            }
            for (index, child) in ax_elements(&deadline, &element, "AXChildren", take)?
                .into_iter()
                .enumerate()
            {
                let mut child_path = path.clone();
                child_path.push(index);
                queue.push_back((
                    child,
                    Some(element_id.clone()),
                    depth + 1,
                    child_path,
                    lineage.clone(),
                    protected,
                ));
            }
        } else if child_count > 0 {
            truncated = true;
        }
        elements.push((
            element_id.clone(),
            ElementRecord {
                surface_id: surface_id.to_string(),
                path,
                lineage,
            },
        ));
        nodes.push(json!({
            "element_id": element_id,
            "parent_element_id": parent_element_id,
            "depth": depth,
            "role": role,
            "subrole": subrole,
            "title": title,
            "description": description,
            "value": value,
            "placeholder": placeholder,
            "enabled": enabled,
            "focused": focused,
            "child_count": child_count,
        }));
    }
    if !queue.is_empty() {
        truncated = true;
    }
    deadline.ensure_remaining()?;
    let node_count = nodes.len();
    Ok(AccessibilityTreeResult {
        output: json!({
            "platform": "macos",
            "surface_id": surface_id,
            "nodes": nodes,
            "node_count": node_count,
            "truncated": truncated,
            "max_depth": max_depth,
            "max_nodes": max_nodes,
        }),
        elements,
    })
}

#[cfg(target_os = "macos")]
fn resolve_correlated_element(
    surface: &SurfaceRecord,
    element: &ElementRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    if element.lineage.len() != element.path.len() + 1 {
        return Err("stale_element: AX element correlation lineage is incomplete".to_string());
    }
    let mut current = exact_ax_window(surface, deadline)?;
    let current_root_fingerprint = element_fingerprint(deadline, &current, false)?;
    ensure_correlated_fingerprint(&element.lineage[0], &current_root_fingerprint, true)?;
    for (depth, &index) in element.path.iter().enumerate() {
        let child_count = ax_array_count(deadline, &current, "AXChildren")?;
        if index >= child_count {
            return Err("stale_element: AX child path no longer exists".to_string());
        }
        current = ax_element_at(deadline, &current, "AXChildren", index)?;
        let current_fingerprint =
            element_fingerprint(deadline, &current, element.lineage[depth].protected)?;
        ensure_correlated_fingerprint(&element.lineage[depth + 1], &current_fingerprint, false)?;
    }
    Ok(current)
}

#[cfg(target_os = "macos")]
pub(super) fn element_state(
    surface_id: &str,
    element_id: &str,
    observation_generation: u32,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target = validate_element_state_target(element)?;
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    let protected = element.contains_protected_content()
        || element.lineage.iter().any(is_secure_text_fingerprint);
    let enabled_for_effect = enabled != Some(false);
    let can_press =
        !protected && enabled_for_effect && ax_supports_action(&deadline, &current, "AXPress")?;
    let can_focus = !protected
        && enabled_for_effect
        && ax_attribute_settable(&deadline, &current, "AXFocused")?;

    let supported_text = !protected && is_supported_text_input_fingerprint(target);
    let (value_empty, can_input_text) = if supported_text {
        let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
        let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
        let value_empty = current_value.as_deref().map(str::is_empty);
        let can_input_text = enabled != Some(false)
            && focused == Some(true)
            && value_settable
            && value_empty == Some(true);
        (value_empty, can_input_text)
    } else {
        (None, false)
    };
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "observation_generation": observation_generation,
        "enabled": enabled,
        "focused": focused,
        "protected": protected,
        "value_empty": value_empty,
        "can_press": can_press,
        "can_focus": can_focus,
        "can_input_text": can_input_text,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn activate_window(surface_id: &str, surface: &SurfaceRecord) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }

    // Re-resolve the native surface and exact AX window immediately before
    // any effect so an opaque stale surface cannot drift to another window.
    let deadline = AxObservationDeadline::new();
    let window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(surface.pid as _) };
    let frontmost = optional_ax_bool(&deadline, &application, "AXFrontmost")?;
    if frontmost != Some(true) && !ax_attribute_settable(&deadline, &application, "AXFrontmost")? {
        return Err(
            "control_failed: AX application does not allow AXFrontmost to be set".to_string(),
        );
    }
    if !ax_supports_action(&deadline, &window, "AXRaise")? {
        return Err("control_failed: exact AX window does not support AXRaise".to_string());
    }

    // Prepare both native call sites before the first mutation. After the
    // application becomes frontmost, any later failure is a partial effect.
    prepare_ax_call(&deadline, &application)?;
    prepare_ax_call(&deadline, &window)?;
    let mut application_activated = false;
    if frontmost != Some(true) {
        let error = unsafe {
            application.set_attribute_value(
                &CFString::from_static_str("AXFrontmost"),
                CFBoolean::new(true),
            )
        };
        if error != AXError::Success {
            return Err(window_activation_attempt_error(
                "AXUIElementSetAttributeValue(AXFrontmost)",
                error,
                false,
            ));
        }
        application_activated = true;
    }

    let error = unsafe { window.perform_action(&CFString::from_static_str("AXRaise")) };
    if error != AXError::Success {
        return Err(window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            error,
            application_activated,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn control(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    action: ComputerAction,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be controlled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for control".to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;

    match action {
        ComputerAction::Press if !ax_supports_action(&deadline, &current, "AXPress")? => {
            return Err(
                "control_failed: AX element does not support the AXPress action".to_string(),
            );
        }
        ComputerAction::Focus if !ax_attribute_settable(&deadline, &current, "AXFocused")? => {
            return Err(
                "control_failed: AX element does not allow AXFocused to be set".to_string(),
            );
        }
        _ => {}
    }

    prepare_ax_call(&deadline, &current)?;
    let error = match action {
        ComputerAction::Press => unsafe {
            current.perform_action(&CFString::from_static_str("AXPress"))
        },
        ComputerAction::Focus => unsafe {
            current.set_attribute_value(
                &CFString::from_static_str("AXFocused"),
                CFBoolean::new(true),
            )
        },
    };
    if error != AXError::Success {
        return Err(control_attempt_error(
            match action {
                ComputerAction::Press => "AXUIElementPerformAction(AXPress)",
                ComputerAction::Focus => "AXUIElementSetAttributeValue(AXFocused)",
            },
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "action": action.as_str(),
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn scroll_to_element(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be scrolled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for scrolling"
                .to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    if !ax_supports_action(&deadline, &current, "AXScrollToVisible")? {
        return Err(
            "scroll_failed: AX element does not support the AXScrollToVisible action".to_string(),
        );
    }
    prepare_ax_call(&deadline, &current)?;
    let error = unsafe { current.perform_action(&CFString::from_static_str("AXScrollToVisible")) };
    if error != AXError::Success {
        return Err(scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
fn validate_key_input_target(
    deadline: &AxObservationDeadline,
    application: &AXUIElement,
    exact_window: &CFRetained<AXUIElement>,
) -> Result<(), String> {
    if optional_ax_bool(deadline, application, "AXFrontmost")? != Some(true) {
        return Err(
            "key_input_failed: exact surface application must already be frontmost".to_string(),
        );
    }
    let focused_window = optional_ax_value(deadline, application, "AXFocusedWindow")?
        .ok_or_else(|| {
            "key_input_failed: exact surface application has no focused window".to_string()
        })?
        .downcast::<AXUIElement>()
        .map_err(|_| "accessibility_failed: AXFocusedWindow is not an AXUIElement".to_string())?;
    if &focused_window != exact_window {
        return Err(
            "key_input_failed: exact surface must already be the focused window".to_string(),
        );
    }

    if let Some(focused_value) = optional_ax_value(deadline, application, "AXFocusedUIElement")? {
        let focused_element = focused_value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AXFocusedUIElement is not an AXUIElement".to_string()
        })?;
        let fingerprint = element_fingerprint(deadline, &focused_element, false)?;
        if fingerprint.protected || is_secure_text_fingerprint(&fingerprint) {
            return Err(
                "permission_denied: protected or secure Accessibility content cannot receive key input"
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn key_input(
    surface_id: &str,
    surface: &SurfaceRecord,
    key: &str,
    modifiers: &[String],
) -> Result<Value, String> {
    validate_key_input(key, modifiers)?;
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    if !CGPreflightPostEventAccess() {
        return Err("permission_denied: macOS event-posting permission is not granted".to_string());
    }

    let pid = checked_surface_pid(surface)?;
    let deadline = AxObservationDeadline::new();
    let exact_window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(pid) };

    let key_code = key_code(key)?;
    let flags = key_modifier_flags(modifiers)?;
    let key_down = CGEvent::new_keyboard_event(None, key_code, true)
        .ok_or_else(|| "key_input_failed: could not create native key-down event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(None, key_code, false)
        .ok_or_else(|| "key_input_failed: could not create native key-up event".to_string())?;
    CGEvent::set_flags(Some(&key_down), flags);
    CGEvent::set_flags(Some(&key_up), flags);

    // This is the final authority/privacy check before the first effect. Quartz
    // posts keyboard events to a process rather than a specific window, so keep
    // the exact focused-window check as close to dispatch as possible.
    validate_key_input_target(&deadline, &application, &exact_window)?;
    deadline.ensure_remaining()?;

    CGEvent::post_to_pid(pid, Some(&key_down));
    CGEvent::post_to_pid(pid, Some(&key_up));
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "key": key,
        "modifiers": modifiers,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn input_text(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    text: &str,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let text_bytes = validate_input_text(text)?;
    validate_text_input_target(element)?;

    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    // This is deliberately the final read before the effect. The helper may
    // truncate a non-empty string for bounded observation, but emptiness is
    // preserved exactly and caller text is never transformed or normalized.
    let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
    validate_text_input_preflight(enabled, focused, value_settable, current_value.as_deref())?;

    let text_value = CFString::from_str(text);
    prepare_ax_call(&deadline, &current)?;
    let error =
        unsafe { current.set_attribute_value(&CFString::from_static_str("AXValue"), &text_value) };
    if error != AXError::Success {
        return Err(text_input_attempt_error(error));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "text_bytes": text_bytes,
        "success": true,
    }))
}

#[cfg(windows)]
pub(super) fn accessibility_tree(
    surface_id: &str,
    surface: &SurfaceRecord,
    max_depth: usize,
    max_nodes: usize,
) -> Result<AccessibilityTreeResult, String> {
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
    let mut queue = VecDeque::from([(
        root,
        None::<String>,
        0usize,
        Vec::<usize>::new(),
        Vec::<ElementFingerprint>::new(),
        false,
    )]);
    let mut nodes = Vec::with_capacity(max_nodes.min(64));
    let mut elements = Vec::with_capacity(max_nodes.min(64));
    let mut truncated = false;

    while let Some((current, parent_element_id, depth, path, mut lineage, inherited_protected)) =
        queue.pop_front()
    {
        if nodes.len() >= max_nodes {
            truncated = true;
            break;
        }

        let element_id = format!("element_{}", Uuid::new_v4().simple());
        let fingerprint = uia_fingerprint(&context, &current, inherited_protected)?;
        let role = fingerprint.role.clone();
        let subrole = fingerprint.subrole.clone();
        let title = fingerprint.title.clone();
        let description = fingerprint.description.clone();
        let placeholder = fingerprint.placeholder.clone();
        let protected = fingerprint.protected;
        let value = if protected || !is_supported_text_input_fingerprint(&fingerprint) {
            None
        } else {
            uia_text_value(&context, &current)?
        };
        context.deadline.ensure_remaining()?;
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
            .as_bool();
        context.deadline.ensure_remaining()?;
        let focused = unsafe { current.CurrentHasKeyboardFocus() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentHasKeyboardFocus", &error))?
            .as_bool();
        lineage.push(fingerprint);

        let reserved = nodes.len() + queue.len() + 1;
        let remaining = max_nodes.saturating_sub(reserved);
        let inspect_limit = if depth < max_depth {
            remaining.saturating_add(1).max(1)
        } else {
            1
        };
        let (children, has_more_children) = uia_children(&context, &current, inspect_limit)?;
        let child_count = children.len() + usize::from(has_more_children);

        if depth < max_depth {
            if children.len() > remaining || has_more_children {
                truncated = true;
            }
            for (index, child) in children.into_iter().take(remaining).enumerate() {
                let mut child_path = path.clone();
                child_path.push(index);
                queue.push_back((
                    child,
                    Some(element_id.clone()),
                    depth + 1,
                    child_path,
                    lineage.clone(),
                    protected,
                ));
            }
        } else if child_count > 0 {
            truncated = true;
        }

        elements.push((
            element_id.clone(),
            ElementRecord {
                surface_id: surface_id.to_string(),
                path,
                lineage,
            },
        ));
        nodes.push(json!({
            "element_id": element_id,
            "parent_element_id": parent_element_id,
            "depth": depth,
            "role": role,
            "subrole": subrole,
            "title": title,
            "description": description,
            "value": value,
            "placeholder": placeholder,
            "enabled": enabled,
            "focused": focused,
            "child_count": child_count,
        }));
    }

    if !queue.is_empty() {
        truncated = true;
    }
    let node_count = nodes.len();
    Ok(AccessibilityTreeResult {
        output: json!({
            "platform": "windows",
            "surface_id": surface_id,
            "nodes": nodes,
            "node_count": node_count,
            "truncated": truncated,
            "max_depth": max_depth,
            "max_nodes": max_nodes,
        }),
        elements,
    })
}

#[cfg(windows)]
pub(super) fn element_state(
    surface_id: &str,
    element_id: &str,
    observation_generation: u32,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for state".to_string(),
        );
    }
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    let enabled = unsafe { current.CurrentIsEnabled() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
        .as_bool();
    let focused = uia_element_has_exact_focus(&context, &current)?;
    let protected = element.contains_protected_content();
    let (value_empty, value_writable) = if !protected && is_supported_text_input_fingerprint(target)
    {
        match uia_text_pattern(&context, &current)? {
            Some(pattern) => {
                let value = uia_value_pattern_current_value(&context, &pattern)?;
                let writable = uia_value_pattern_writable(&context, &pattern)?;
                (Some(value.is_empty()), writable)
            }
            None => (None, false),
        }
    } else {
        (None, false)
    };
    let hwnd = win_hwnd(surface.native_id)?;
    let surface_foreground = unsafe { GetForegroundWindow() == hwnd };
    let can_press = if protected || !enabled {
        false
    } else {
        optional_uia_pattern::<IUIAutomationInvokePattern>(&context, &current, UIA_InvokePatternId)?
            .is_some()
    };
    let can_focus =
        if protected || !enabled || !surface_foreground || !uia_semantic_focus_role(&target.role) {
            false
        } else {
            context.deadline.ensure_remaining()?;
            unsafe { current.CurrentIsKeyboardFocusable() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                })?
                .as_bool()
        };
    let can_input_text = !protected
        && enabled
        && surface_foreground
        && focused
        && uia_semantic_text_input_role(&target.role)
        && value_writable
        && value_empty == Some(true);

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "observation_generation": observation_generation,
        "enabled": enabled,
        "focused": focused,
        "protected": protected,
        "value_empty": value_empty,
        "can_press": can_press,
        "can_focus": can_focus,
        "can_input_text": can_input_text,
    }))
}

#[cfg(windows)]
pub(super) fn windows_window_activation_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} did not establish the exact foreground-window postcondition after the native activation attempt"
    )
}

#[cfg(windows)]
pub(super) fn windows_control_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation control effect was attempted"
    )
}

#[cfg(windows)]
pub(super) fn windows_scroll_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation scroll effect was attempted"
    )
}

#[cfg(windows)]
pub(super) fn windows_key_input_attempt_error(operation: &str) -> String {
    format!("outcome_unknown: {operation} returned after Windows native key input was attempted")
}

#[cfg(windows)]
fn validate_windows_send_input_count(inserted: u32, expected: u32) -> Result<(), String> {
    if inserted == expected {
        Ok(())
    } else if inserted == 0 {
        Err(format!(
            "key_input_failed: SendInput inserted 0 of {expected} prepared keyboard events; no keyboard event was inserted"
        ))
    } else {
        Err(windows_key_input_attempt_error(&format!(
            "SendInput inserted {inserted} of {expected} prepared keyboard events"
        )))
    }
}

#[cfg(windows)]
pub(super) fn windows_text_input_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation text write was attempted"
    )
}

#[cfg(windows)]
pub(super) fn activate_window(surface_id: &str, surface: &SurfaceRecord) -> Result<Value, String> {
    // Resolve the exact xcap identity immediately before the first native
    // effect. This never falls back to an application name, PID, or title.
    let _window = resolve_surface_window(surface)?;
    let hwnd = win_hwnd(surface.native_id)?;
    let already_foreground = unsafe { GetForegroundWindow() == hwnd };
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    if already_foreground && !minimized {
        return Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "success": true,
        }));
    }

    // Obtain the exact UIA root before the first effect. This revalidates
    // the same HWND/PID lineage used by read-only Windows observation.
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
    context.deadline.ensure_remaining()?;
    let control_type = unsafe { root.CurrentControlType() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
    if control_type != UIA_WindowControlTypeId {
        return Err(
            "control_failed: exact Windows UIA root is not an activatable Window control"
                .to_string(),
        );
    }
    let mut prior_effect = false;
    if minimized {
        // Restoring a foreign UI thread must not synchronously wait on a
        // stalled target. Queue the exact restore request asynchronously,
        // then observe only the local window-state predicate for a bounded
        // interval before proceeding.
        let restore_expires_at = Instant::now() + Duration::from_secs(2);
        let _ = unsafe { ShowWindowAsync(hwnd, SW_RESTORE) };
        prior_effect = true;
        while unsafe { IsIconic(hwnd).as_bool() } {
            if let Err(error) = context.deadline.ensure_remaining() {
                return Err(windows_window_activation_attempt_error(&error));
            }
            if Instant::now() >= restore_expires_at {
                return Err(windows_window_activation_attempt_error(
                    "ShowWindowAsync(SW_RESTORE) timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // A background Runner is normally denied by SetForegroundWindow.
    // UI Automation's exact SetFocus is the native automation primitive;
    // once attempted, any error or mismatched postcondition is uncertain.
    if let Err(error) = context.deadline.ensure_remaining() {
        if prior_effect {
            return Err(windows_window_activation_attempt_error(&error));
        }
        return Err(error);
    }
    if let Err(error) = unsafe { root.SetFocus() } {
        return Err(windows_window_activation_attempt_error(&format!(
            "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(windows_window_activation_attempt_error(
            "IUIAutomationElement::SetFocus postcondition",
        ));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "success": true,
    }))
}

#[cfg(windows)]
pub(super) fn control(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    action: ComputerAction,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot be controlled"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for control"
                .to_string(),
        );
    }

    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    let enabled = unsafe { current.CurrentIsEnabled() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
        .as_bool();
    if !enabled {
        return Err("control_failed: UI Automation element is disabled".to_string());
    }

    match action {
        ComputerAction::Press => {
            let pattern = optional_uia_pattern::<IUIAutomationInvokePattern>(
                &context,
                &current,
                UIA_InvokePatternId,
            )?
            .ok_or_else(|| {
                "control_failed: UI Automation element does not support InvokePattern".to_string()
            })?;
            context.deadline.ensure_remaining()?;
            if let Err(error) = unsafe { pattern.Invoke() } {
                return Err(windows_control_attempt_error(&format!(
                    "IUIAutomationInvokePattern::Invoke HRESULT(0x{:08X})",
                    error.code().0 as u32
                )));
            }
        }
        ComputerAction::Focus => {
            if !uia_semantic_focus_role(&target.role) {
                return Err(
                    "control_failed: UI Automation element role is outside the bounded semantic focus set"
                        .to_string(),
                );
            }
            let hwnd = win_hwnd(surface.native_id)?;
            if unsafe { GetForegroundWindow() != hwnd } {
                return Err(
                    "control_failed: exact Windows surface must already be foreground before element focus"
                        .to_string(),
                );
            }
            context.deadline.ensure_remaining()?;
            let focusable = unsafe { current.CurrentIsKeyboardFocusable() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                })?
                .as_bool();
            if !focusable {
                return Err(
                    "control_failed: UI Automation element is not keyboard-focusable".to_string(),
                );
            }
            let already_focused = uia_element_has_exact_focus(&context, &current)?;
            if !already_focused {
                context.deadline.ensure_remaining()?;
                if let Err(error) = unsafe { current.SetFocus() } {
                    return Err(windows_control_attempt_error(&format!(
                        "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
                        error.code().0 as u32
                    )));
                }
                let focus_expires_at = Instant::now() + Duration::from_secs(1);
                loop {
                    if let Err(error) = context.deadline.ensure_remaining() {
                        return Err(windows_control_attempt_error(&error));
                    }
                    let focused = match uia_element_has_exact_focus(&context, &current) {
                        Ok(focused) => focused,
                        Err(error) => return Err(windows_control_attempt_error(&error)),
                    };
                    if focused {
                        break;
                    }
                    if Instant::now() >= focus_expires_at {
                        return Err(windows_control_attempt_error(
                            "IUIAutomationElement::SetFocus postcondition timeout",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "action": action.as_str(),
        "success": true,
    }))
}

#[cfg(windows)]
pub(super) fn scroll_to_element(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot be scrolled"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for scrolling"
                .to_string(),
        );
    }

    // Revalidate the exact xcap surface, HWND/PID UIA root, and complete
    // RuntimeId-bearing root -> ancestor -> target lineage before the effect.
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    let pattern = optional_uia_pattern::<IUIAutomationScrollItemPattern>(
        &context,
        &current,
        UIA_ScrollItemPatternId,
    )?
    .ok_or_else(|| {
        "scroll_failed: UI Automation element does not support ScrollItemPattern".to_string()
    })?;
    context.deadline.ensure_remaining()?;
    if let Err(error) = unsafe { pattern.ScrollIntoView() } {
        return Err(windows_scroll_attempt_error(&format!(
            "IUIAutomationScrollItemPattern::ScrollIntoView HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    if let Err(error) = context.deadline.ensure_remaining() {
        return Err(windows_scroll_attempt_error(&error));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "success": true,
    }))
}

#[cfg(windows)]
pub(super) fn key_input(
    surface_id: &str,
    surface: &SurfaceRecord,
    key: &str,
    modifiers: &[String],
) -> Result<Value, String> {
    validate_key_input(key, modifiers)?;
    let context = UiaContext::new()?;

    // Prove exact foreground/focus ownership before preparing the native input.
    let root = exact_uia_window(&context, surface)?;
    validate_windows_key_input_target(&context, surface, &root)?;

    // Prepare the complete bounded input sequence before the first native effect.
    // `command` deliberately fails here instead of being mapped to the Windows key.
    let inputs = windows_key_input_plan(key, modifiers)?;
    let expected_count = u32::try_from(inputs.len())
        .map_err(|_| "key_input_failed: Windows key input sequence is too large".to_string())?;
    let input_size = i32::try_from(std::mem::size_of::<INPUT>())
        .map_err(|_| "key_input_failed: Windows INPUT size is invalid".to_string())?;

    // Revalidate the exact surface/root and focus as close to SendInput as practical.
    let root = exact_uia_window(&context, surface)?;
    validate_windows_key_input_target(&context, surface, &root)?;
    context.deadline.ensure_remaining()?;
    // SendInput shares the interactive desktop's keyboard state. Reject the
    // physical modifier/target states that can turn this closed request into
    // a different chord or leave the model racing an already-held key.
    validate_windows_keyboard_state(key)?;

    let inserted = unsafe { SendInput(&inputs, input_size) };
    validate_windows_send_input_count(inserted, expected_count)?;
    if let Err(error) = context.deadline.ensure_remaining() {
        return Err(windows_key_input_attempt_error(&error));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "key": key,
        "modifiers": modifiers,
        "success": true,
    }))
}

#[cfg(windows)]
pub(super) fn input_text(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    text: &str,
) -> Result<Value, String> {
    let text_bytes = validate_input_text(text)?;
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot receive text input"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for text input"
                .to_string(),
        );
    }
    if !uia_semantic_text_input_role(&target.role) {
        return Err(
            "input_failed: UI Automation element is outside the bounded Windows text-entry role set"
                .to_string(),
        );
    }

    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    let enabled = unsafe { current.CurrentIsEnabled() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
        .as_bool();
    if !enabled {
        return Err("input_failed: UI Automation text element is disabled".to_string());
    }
    let pattern = uia_text_pattern(&context, &current)?.ok_or_else(|| {
        "input_failed: UI Automation text element does not expose ValuePattern".to_string()
    })?;
    if !uia_value_pattern_writable(&context, &pattern)? {
        return Err("input_failed: UI Automation ValuePattern is read-only".to_string());
    }

    let hwnd = win_hwnd(surface.native_id)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "input_failed: exact Windows surface must already be foreground before text input"
                .to_string(),
        );
    }
    if !uia_element_has_exact_focus(&context, &current)? {
        return Err(
            "input_failed: exact Windows text element must already have keyboard focus".to_string(),
        );
    }

    // Keep emptiness as the final state read before the native write. The
    // value never leaves the Runner; only the empty/non-empty affordance is
    // exposed through element_state.
    let current_value = uia_value_pattern_current_value(&context, &pattern)?;
    if !current_value.is_empty() {
        return Err(
            "input_failed: UI Automation ValuePattern must be empty before bounded text input; observe and reconcile before retrying"
                .to_string(),
        );
    }

    let value = windows::core::BSTR::from(text);
    context.deadline.ensure_remaining()?;
    if let Err(error) = unsafe { pattern.SetValue(&value) } {
        return Err(windows_text_input_attempt_error(&format!(
            "IUIAutomationValuePattern::SetValue HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "text_bytes": text_bytes,
        "success": true,
    }))
}

#[cfg(windows)]
fn win32_capture_error(operation: &str) -> String {
    format!(
        "capture_failed: {operation} failed: {}",
        std::io::Error::last_os_error()
    )
}

#[cfg(windows)]
struct WindowDc {
    hwnd: SysHwnd,
    hdc: HDC,
}

#[cfg(windows)]
impl WindowDc {
    fn acquire(hwnd: SysHwnd) -> Result<Self, String> {
        let hdc = unsafe { GetWindowDC(hwnd) };
        if hdc.is_null() {
            Err(win32_capture_error("GetWindowDC"))
        } else {
            Ok(Self { hwnd, hdc })
        }
    }
}

#[cfg(windows)]
impl Drop for WindowDc {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseDC(self.hwnd, self.hdc);
        }
    }
}

#[cfg(windows)]
struct MemoryDc(HDC);

#[cfg(windows)]
impl MemoryDc {
    fn create(source: HDC) -> Result<Self, String> {
        let hdc = unsafe { CreateCompatibleDC(source) };
        if hdc.is_null() {
            Err(win32_capture_error("CreateCompatibleDC"))
        } else {
            Ok(Self(hdc))
        }
    }
}

#[cfg(windows)]
impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

#[cfg(windows)]
struct OwnedBitmap(HBITMAP);

#[cfg(windows)]
impl OwnedBitmap {
    fn create(source: HDC, plan: super::RawCapturePlan) -> Result<Self, String> {
        let bitmap =
            unsafe { CreateCompatibleBitmap(source, plan.native_width, plan.native_height) };
        if bitmap.is_null() {
            Err(win32_capture_error("CreateCompatibleBitmap"))
        } else {
            Ok(Self(bitmap))
        }
    }
}

#[cfg(windows)]
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0 as HGDIOBJ);
        }
    }
}

#[cfg(windows)]
struct SelectedBitmap {
    hdc: HDC,
    previous: HGDIOBJ,
}

#[cfg(windows)]
impl SelectedBitmap {
    fn select(hdc: HDC, bitmap: HBITMAP) -> Result<Self, String> {
        let previous = unsafe { SelectObject(hdc, bitmap as HGDIOBJ) };
        if previous.is_null() {
            Err(win32_capture_error("SelectObject"))
        } else {
            Ok(Self { hdc, previous })
        }
    }
}

#[cfg(windows)]
impl Drop for SelectedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.previous);
        }
    }
}

#[cfg(windows)]
fn create_bounded_bitmap(
    source: HDC,
    fallback: super::RawCapturePlan,
) -> Result<(OwnedBitmap, super::RawCapturePlan), String> {
    let mut native_width = fallback.native_width;
    let mut native_height = fallback.native_height;
    let current = unsafe { GetCurrentObject(source, OBJ_BITMAP as u32) };
    if !current.is_null() {
        let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
        let copied = unsafe {
            GetObjectW(
                current,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut bitmap as *mut BITMAP).cast(),
            )
        };
        if copied != 0 {
            native_width = bitmap.bmWidth;
            native_height = bitmap.bmHeight;
        }
    }

    let plan = super::raw_capture_plan(native_width, native_height)?;
    let bitmap = OwnedBitmap::create(source, plan)?;
    Ok((bitmap, plan))
}

#[cfg(windows)]
fn capture_window_gdi(
    native_id: u32,
    output_width: u32,
    output_height: u32,
) -> Result<image::RgbaImage, String> {
    let output_plan = super::raw_capture_plan(
        super::native_capture_dimension(output_width, "window width")?,
        super::native_capture_dimension(output_height, "window height")?,
    )?;
    let hwnd = native_id as i32 as isize as SysHwnd;
    if hwnd.is_null() {
        return Err("capture_failed: window handle is invalid".to_string());
    }

    let window_dc = WindowDc::acquire(hwnd)?;
    let (bitmap, capture_plan) = create_bounded_bitmap(window_dc.hdc, output_plan)?;
    let memory_dc = MemoryDc::create(window_dc.hdc)?;
    let selected = SelectedBitmap::select(memory_dc.0, bitmap.0)?;

    let mut captured = unsafe { PrintWindow(hwnd, memory_dc.0, 2) } != 0;
    if !captured {
        captured = unsafe { PrintWindow(hwnd, memory_dc.0, 0) } != 0;
    }
    if !captured {
        captured = unsafe { PrintWindow(hwnd, memory_dc.0, 4) } != 0;
    }
    if !captured {
        captured = unsafe {
            BitBlt(
                memory_dc.0,
                0,
                0,
                capture_plan.native_width,
                capture_plan.native_height,
                window_dc.hdc,
                0,
                0,
                SRCCOPY,
            )
        } != 0;
    }
    if !captured {
        return Err(win32_capture_error("PrintWindow/BitBlt"));
    }
    drop(selected);

    let raw_size = u32::try_from(capture_plan.byte_len)
        .map_err(|_| "image_too_large: raw RGBA capture size exceeds Win32".to_string())?;
    let mut bitmap_info: BITMAPINFO = unsafe { std::mem::zeroed() };
    bitmap_info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: capture_plan.native_width,
        biHeight: -capture_plan.native_height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: raw_size,
        ..unsafe { std::mem::zeroed() }
    };
    let mut pixels = vec![0u8; capture_plan.byte_len];
    let scan_lines = unsafe {
        GetDIBits(
            memory_dc.0,
            bitmap.0,
            0,
            capture_plan.height,
            pixels.as_mut_ptr().cast(),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };
    if scan_lines != capture_plan.native_height {
        return Err(win32_capture_error("GetDIBits"));
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(capture_plan.width, capture_plan.height, pixels)
        .ok_or_else(|| "capture_failed: RGBA image dimensions are inconsistent".to_string())?;
    if image.width() == output_plan.width && image.height() == output_plan.height {
        Ok(image)
    } else {
        Ok(image::imageops::resize(
            &image,
            output_plan.width,
            output_plan.height,
            image::imageops::FilterType::CatmullRom,
        ))
    }
}

#[cfg(target_os = "macos")]
fn ensure_platform_capture_bound(_window: &Window, width: u32, height: u32) -> Result<(), String> {
    // xcap's macOS window bounds are display-space dimensions while
    // CGWindowListCreateImage can produce backing-pixel images. Use the
    // largest live display scale as a conservative bound, including windows
    // spanning displays; missing/invalid scale information fails closed.
    let monitors = xcap::Monitor::all().map_err(map_error)?;
    let mut max_scale: Option<f32> = None;
    for monitor in monitors {
        let scale = monitor.scale_factor().map_err(map_error)?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(
                "image_too_large: cannot establish a bounded macOS capture scale".to_string(),
            );
        }
        max_scale = Some(max_scale.map_or(scale, |current| current.max(scale)));
    }
    let scale = max_scale
        .ok_or_else(|| {
            "image_too_large: cannot establish a bounded macOS capture scale".to_string()
        })?
        .max(1.0);
    let scaled = |value: u32| -> Result<u32, String> {
        let pixels = (f64::from(value) * f64::from(scale)).ceil();
        if !pixels.is_finite() || pixels > f64::from(u32::MAX) {
            return Err("image_too_large: macOS capture dimensions overflow".to_string());
        }
        Ok(pixels as u32)
    };
    ensure_raw_capture_bound(scaled(width)?, scaled(height)?)?;
    Ok(())
}

#[cfg(windows)]
fn ensure_platform_capture_bound(window: &Window, width: u32, height: u32) -> Result<(), String> {
    // xcap 0.9.8 is built here without WGC, so its GDI path can allocate a
    // display-backed bitmap before cropping to the requested window. Bound
    // both the current window and current monitor; if monitor metrics cannot
    // be obtained we fail closed instead of pretending the window size alone
    // bounds the native allocation.
    ensure_raw_capture_bound(width, height)?;
    let monitor = window.current_monitor().map_err(map_error)?;
    let monitor_width = monitor.width().map_err(map_error)?;
    let monitor_height = monitor.height().map_err(map_error)?;
    ensure_raw_capture_bound(monitor_width, monitor_height)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
    // xcap 0.9.8 reports frontmost-application state on macOS, not exact
    // window focus. Preserve that reliable signal as `active` only.
    (None, window.is_focused().ok())
}

#[cfg(windows)]
fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
    // Windows xcap compares this HWND with GetForegroundWindow(), which is
    // an exact-window signal suitable for both CU-1 fields.
    let focused = window.is_focused().ok();
    (focused, focused)
}

fn identity_hash(
    window: &Window,
    application: &str,
    title: &str,
    width: u32,
    height: u32,
) -> Result<[u8; 32], String> {
    let pid = window.pid().map_err(map_error)?;
    let mut hash = Sha256::new();
    hash.update(pid.to_le_bytes());
    hash.update((application.len() as u64).to_le_bytes());
    hash.update(application.as_bytes());
    hash.update((title.len() as u64).to_le_bytes());
    hash.update(title.as_bytes());
    hash.update(width.to_le_bytes());
    hash.update(height.to_le_bytes());
    Ok(hash.finalize().into())
}

pub(super) fn list_windows(limit: usize) -> Result<Vec<PlatformWindow>, String> {
    ensure_capture_permission()?;
    let mut output = Vec::new();
    for window in Window::all().map_err(map_error)? {
        if output.len() >= limit {
            break;
        }
        if window.is_minimized().unwrap_or(false) {
            continue;
        }
        let width = window.width().map_err(map_error)?;
        let height = window.height().map_err(map_error)?;
        if width == 0 || height == 0 {
            continue;
        }
        let application = window.app_name().map_err(map_error)?;
        let title = window.title().map_err(map_error)?;
        let identity_hash = identity_hash(&window, &application, &title, width, height)?;
        let (focused, active) = focus_state(&window);
        output.push(PlatformWindow {
            native_id: window.id().map_err(map_error)?,
            pid: window.pid().map_err(map_error)?,
            identity_hash,
            application,
            title,
            width,
            height,
            focused,
            active,
        });
    }
    Ok(output)
}

pub(super) fn capture_window(surface: &SurfaceRecord) -> Result<image::RgbaImage, String> {
    ensure_capture_permission()?;
    let window = Window::all()
        .map_err(map_error)?
        .into_iter()
        .find(|window| window.id().ok() == Some(surface.native_id))
        .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
    let width = window.width().map_err(map_error)?;
    let height = window.height().map_err(map_error)?;
    let application = window.app_name().map_err(map_error)?;
    let title = window.title().map_err(map_error)?;
    let current_identity = identity_hash(&window, &application, &title, width, height)?;
    if current_identity != surface.identity_hash {
        return Err("stale_surface: window identity changed since enumeration".to_string());
    }
    if bounded_text(&application) != surface.application || bounded_text(&title) != surface.title {
        return Err("stale_surface: window metadata changed since enumeration".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        ensure_platform_capture_bound(&window, width, height)?;
        window.capture_image().map_err(map_error)
    }
    #[cfg(windows)]
    {
        ensure_platform_capture_bound(&window, width, height)?;
        capture_window_gdi(surface.native_id, width, height)
    }
}
#[cfg(all(test, windows))]
pub(super) fn test_uia_is_offscreen(
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<bool, String> {
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    unsafe { current.CurrentIsOffscreen() }
        .map(|value| value.as_bool())
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsOffscreen", &error))
}
#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_map(
    monitor_x: i32,
    monitor_y: i32,
    source_width: u32,
    source_height: u32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: u32,
    virtual_height: u32,
    x: u32,
    y: u32,
) -> Result<(i32, i32, i32, i32), String> {
    map_windows_pointer_coordinate(
        monitor_x,
        monitor_y,
        source_width,
        source_height,
        virtual_left,
        virtual_top,
        virtual_width,
        virtual_height,
        x,
        y,
    )
    .map(|plan| {
        (
            plan.global_x,
            plan.global_y,
            plan.normalized_x,
            plan.normalized_y,
        )
    })
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_dpi_context_metrics() -> Result<
    (
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
    ),
    String,
> {
    let before = windows_virtual_desktop_metrics()?;
    let (during, xcap_bounds) = {
        let _context = enter_pointer_coordinate_context()?;
        (
            windows_virtual_desktop_metrics()?,
            windows_xcap_virtual_bounds()?,
        )
    };
    let after = windows_virtual_desktop_metrics()?;
    Ok((before, during, xcap_bounds, after))
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_coordinate_spaces(
    virtual_metrics: (i32, i32, u32, u32),
    xcap_bounds: (i32, i32, u32, u32),
) -> Result<(), String> {
    validate_windows_pointer_coordinate_spaces(virtual_metrics, xcap_bounds)
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_state_guard(
    action: PointerAction,
    down_virtual_key: Option<u16>,
) -> Result<(), String> {
    validate_windows_pointer_state_with(action, |candidate| down_virtual_key == Some(candidate.0))
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_move_send_input_count(inserted: u32) -> Result<(), String> {
    validate_windows_pointer_move_send_input_count(inserted)
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_button_send_input_count(inserted: u32) -> Result<(), String> {
    validate_windows_pointer_button_send_input_count(inserted)
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_postcondition(
    global_x: i32,
    global_y: i32,
    cursor_x: i32,
    cursor_y: i32,
    action: PointerAction,
    left_button_down: bool,
) -> Result<(), String> {
    validate_windows_pointer_postcondition(
        PointerPlan {
            global_x,
            global_y,
            normalized_x: 0,
            normalized_y: 0,
        },
        action,
        cursor_x,
        cursor_y,
        left_button_down,
    )
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_input_flags(action: PointerAction) -> Vec<u32> {
    let plan = PointerPlan {
        global_x: 10,
        global_y: 20,
        normalized_x: 100,
        normalized_y: 200,
    };
    let mut flags: Vec<u32> = windows_pointer_move_inputs(plan)
        .iter()
        .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 })
        .collect();
    if action == PointerAction::Click {
        flags.extend(
            windows_pointer_click_button_inputs(plan)
                .iter()
                .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 }),
        );
    }
    flags
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_pointer_dispatch_trace(
    action: PointerAction,
    move_inserted: u32,
    first_cursor: (i32, i32),
    click_state_down_virtual_key: Option<u16>,
    button_inserted: u32,
    final_cursor: (i32, i32),
    final_left_button_down: bool,
) -> (Result<bool, String>, Vec<Vec<u32>>, usize) {
    let plan = PointerPlan {
        global_x: 10,
        global_y: 20,
        normalized_x: 100,
        normalized_y: 200,
    };
    let mut send_calls = 0usize;
    let mut sent_flags = Vec::new();
    let mut cursor_calls = 0usize;
    let mut click_state_checks = 0usize;
    let result = dispatch_windows_pointer_with(
        plan,
        action,
        |inputs| {
            sent_flags.push(
                inputs
                    .iter()
                    .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 })
                    .collect(),
            );
            let inserted = if send_calls == 0 {
                move_inserted
            } else {
                button_inserted
            };
            send_calls += 1;
            inserted
        },
        || {
            let point = if cursor_calls == 0 {
                first_cursor
            } else {
                final_cursor
            };
            cursor_calls += 1;
            Ok(point)
        },
        || {
            click_state_checks += 1;
            validate_windows_pointer_state_with(PointerAction::Click, |candidate| {
                click_state_down_virtual_key == Some(candidate.0)
            })
        },
        || final_left_button_down,
    );
    (result, sent_flags, click_state_checks)
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_key_input_plan(
    key: &str,
    modifiers: &[String],
) -> Result<Vec<(u16, bool, bool)>, String> {
    windows_key_input_plan(key, modifiers).map(|inputs| {
        inputs
            .iter()
            .map(|input| {
                let keyboard = unsafe { input.Anonymous.ki };
                (
                    keyboard.wVk.0,
                    keyboard.dwFlags.contains(KEYEVENTF_KEYUP),
                    keyboard.dwFlags.contains(KEYEVENTF_EXTENDEDKEY),
                )
            })
            .collect()
    })
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_send_input_count(inserted: u32, expected: u32) -> Result<(), String> {
    validate_windows_send_input_count(inserted, expected)
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_keyboard_state_guard(
    key: &str,
    down_virtual_key: Option<u16>,
) -> Result<(), String> {
    validate_windows_keyboard_state_with(key, |candidate| down_virtual_key == Some(candidate.0))
}

#[cfg(all(test, windows))]
pub(super) fn test_windows_focused_element_belongs_to_surface(
    surface: &SurfaceRecord,
) -> Result<(), String> {
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
    validate_uia_focused_element_root(&context, &root)
}
