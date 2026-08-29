//! Narrow Windows private-state protection for the project/share product path.
//!
//! This module is deliberately scoped to project-entry state and managed tunnel
//! caches. It is not a generic Windows filesystem-security abstraction.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, GENERIC_READ, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;

pub(super) fn protect_private_directory(path: &Path) -> Result<(), String> {
    let directory = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "could not open the private Windows directory for protection".to_string())?;
    reject_reparse(&directory, "private Windows directory")?;
    protect_handle(directory.as_raw_handle() as _, true)
}

pub(super) fn protect_private_file(path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "could not open the private Windows file for protection".to_string())?;
    reject_reparse(&file, "private Windows file")?;
    protect_handle(file.as_raw_handle() as _, false)
}

pub(super) fn read_private_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .access_mode(GENERIC_READ | READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "private authentication material is unreadable".to_string())?;
    reject_reparse(&file, "private authentication material")?;
    protect_handle(file.as_raw_handle() as _, false)?;
    let metadata = file
        .metadata()
        .map_err(|_| "private authentication material is unreadable".to_string())?;
    if !metadata.is_file() {
        return Err("private authentication material is not a regular file".to_string());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| "private authentication material is unreadable".to_string())?;
    Ok(bytes)
}

pub(super) fn write_new_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_WRITE | READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "refused to overwrite existing private Windows state".to_string())?;
    let result = (|| {
        reject_reparse(&file, "private Windows file")?;
        protect_handle(file.as_raw_handle() as _, false)?;
        file.write_all(content)
            .map_err(|_| "could not write private Windows state".to_string())?;
        file.sync_all()
            .map_err(|_| "could not sync private Windows state".to_string())
    })();
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn reject_reparse(file: &File, label: &str) -> Result<(), String> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return Err(format!("could not inspect {label}"));
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!("refusing to use {label} through a reparse point"));
    }
    Ok(())
}

fn current_user_sid() -> Result<String, String> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err("could not inspect the current Windows user identity".to_string());
    }
    let result = (|| {
        let mut required = 0u32;
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required) };
        if required == 0 {
            return Err("could not size the current Windows user identity".to_string());
        }
        let words = (required as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0usize; words];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err("could not read the current Windows user identity".to_string());
        }
        let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        let mut sid_text_ptr = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_text_ptr) } == 0 {
            return Err("could not encode the current Windows user identity".to_string());
        }
        let sid = unsafe {
            let mut len = 0usize;
            while *sid_text_ptr.add(len) != 0 {
                len += 1;
            }
            let value = String::from_utf16_lossy(std::slice::from_raw_parts(sid_text_ptr, len));
            LocalFree(sid_text_ptr as _);
            value
        };
        Ok(sid)
    })();
    unsafe { CloseHandle(token) };
    result
}

fn install_protected_dacl(
    handle: windows_sys::Win32::Foundation::HANDLE,
    sddl: &str,
) -> Result<(), String> {
    let sddl_wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("could not construct the protected Windows DACL".to_string());
    }
    let result = (|| {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = std::ptr::null_mut();
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            == 0
            || present == 0
            || dacl.is_null()
        {
            return Err("could not inspect the protected Windows DACL".to_string());
        }
        let status = unsafe {
            SetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(format!(
                "could not install the protected Windows DACL: OS error {status}"
            ));
        }
        Ok(())
    })();
    unsafe { LocalFree(descriptor as _) };
    result
}

fn protect_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
    inherit_children: bool,
) -> Result<(), String> {
    let sid = current_user_sid()?;
    let ace_flags = if inherit_children { "OICI" } else { "" };
    install_protected_dacl(
        handle,
        &format!("D:P(A;{ace_flags};FA;;;{sid})(A;{ace_flags};FA;;;SY)"),
    )
}

#[cfg(test)]
pub(super) fn current_user_sid_for_test() -> Result<String, String> {
    current_user_sid()
}

#[cfg(test)]
pub(super) fn set_broad_test_file_dacl(path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "could not open Windows test file for ACL setup".to_string())?;
    reject_reparse(&file, "Windows test file")?;
    let sid = current_user_sid()?;
    install_protected_dacl(
        file.as_raw_handle() as _,
        &format!("D:P(A;;FA;;;{sid})(A;;FA;;;SY)(A;;FA;;;WD)(A;;FA;;;BU)(A;;FA;;;BA)"),
    )
}

#[cfg(test)]
pub(super) fn set_broad_test_directory_dacl(path: &Path) -> Result<(), String> {
    let directory = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "could not open Windows test directory for ACL setup".to_string())?;
    reject_reparse(&directory, "Windows test directory")?;
    let sid = current_user_sid()?;
    install_protected_dacl(
        directory.as_raw_handle() as _,
        &format!(
            "D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;WD)(A;OICI;FA;;;BU)(A;OICI;FA;;;BA)"
        ),
    )
}

#[cfg(test)]
pub(super) fn dacl_sddl(path: &Path, directory: bool) -> Result<String, String> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo,
    };

    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL)
        .custom_flags(flags)
        .open(path)
        .map_err(|_| "could not open Windows private state for ACL inspection".to_string())?;
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as _,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(format!(
            "could not read Windows private-state DACL: OS error {status}"
        ));
    }
    let result = (|| {
        let mut text = std::ptr::null_mut();
        let mut text_len = 0u32;
        if unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                &mut text_len,
            )
        } == 0
        {
            return Err("could not render Windows private-state DACL".to_string());
        }
        let value = unsafe {
            let value =
                String::from_utf16_lossy(std::slice::from_raw_parts(text, text_len as usize));
            LocalFree(text as _);
            value
        };
        Ok(value)
    })();
    unsafe { LocalFree(descriptor as _) };
    result
}
