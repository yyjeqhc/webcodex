use super::*;
#[cfg(windows)]
const MAX_NATIVE_APPLICATION_IDENTITY_BYTES: usize = 64 * 1024;

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
pub(crate) fn list_applications(limit: usize) -> Result<Vec<PlatformApplication>, String> {
    enumerate_native_applications(limit).map(|applications| {
        let mut applications = applications
            .into_iter()
            .map(|application| PlatformApplication {
                display_name: application.display_name,
                native_identity: application.native_identity,
            })
            .collect::<Vec<_>>();
        crate::sort_application_candidates(&mut applications);
        applications
    })
}

#[cfg(windows)]
fn revalidate_application_identity(native_identity: &[u8]) -> Result<OwnedPidl, String> {
    let applications = enumerate_native_applications(crate::MAX_APPLICATION_SCAN)?;
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
pub(crate) fn launch_application(
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
pub(crate) fn application_identity_revalidates_for_test(
    native_identity: &[u8],
) -> Result<(), String> {
    revalidate_application_identity(native_identity).map(drop)
}

#[cfg(all(test, windows))]
pub(crate) fn application_shell_execute_contract_for_test(
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
