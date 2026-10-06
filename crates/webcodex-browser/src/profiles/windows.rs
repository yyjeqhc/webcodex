//! Private Browser profiles/bridge credentials use a protected user DACL.
use super::{state_error, BrowserResult};
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{Authorization::*, *},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

// ACE_HEADER.AceType values from winnt.h. windows-sys does not consistently
// project these SDK macros as Security constants across supported versions.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;

fn sid_text(sid: *mut c_void) -> BrowserResult<String> {
    if sid.is_null() {
        return Err(state_error());
    }
    let mut text = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(state_error());
    }
    let mut length = 0;
    unsafe {
        while length < 192 && *text.add(length) != 0 {
            length += 1;
        }
    }
    let result = if length < 192 {
        String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| state_error())
    } else {
        Err(state_error())
    };
    unsafe {
        LocalFree(text.cast());
    }
    result
}
fn current_sid() -> BrowserResult<String> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(state_error());
    }
    let result = (|| {
        let mut length = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length);
        }
        if length == 0 || length > 16384 {
            return Err(state_error());
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(state_error());
        }
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        sid_text(user.User.Sid)
    })();
    unsafe {
        CloseHandle(token);
    }
    result
}

pub(super) fn private_path(path: &Path, created: bool) -> BrowserResult<()> {
    let expected = current_sid()?;
    let mut name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = ptr::null_mut();
    let mut dacl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    if unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    } != 0
    {
        return Err(state_error());
    }
    let result = (|| {
        if sid_text(owner)? != expected {
            return Err(state_error());
        }
        if created {
            // Change only a directory this call created. Never take ownership or
            // silently rewrite permissions on pre-existing state.
            let sddl: Vec<u16> =
                format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{expected})\0")
                    .encode_utf16()
                    .collect();
            let mut replacement = ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut replacement,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(state_error());
            }
            let mut acl = ptr::null_mut();
            let mut present = 0;
            let mut defaulted = 0;
            let ok = unsafe {
                GetSecurityDescriptorDacl(replacement, &mut present, &mut acl, &mut defaulted)
            } != 0;
            let status = if ok && present != 0 && !acl.is_null() {
                unsafe {
                    SetNamedSecurityInfoW(
                        name.as_mut_ptr(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        acl,
                        ptr::null_mut(),
                    )
                }
            } else {
                1
            };
            unsafe {
                LocalFree(replacement);
            }
            return if status == 0 {
                Ok(())
            } else {
                Err(state_error())
            };
        }
        if dacl.is_null() || unsafe { IsValidAcl(dacl) } == 0 || unsafe { (*dacl).AceCount } > 256 {
            return Err(state_error());
        }
        for index in 0..unsafe { (*dacl).AceCount } {
            let mut ace = ptr::null_mut();
            if unsafe { GetAce(dacl, index as u32, &mut ace) } == 0 || ace.is_null() {
                return Err(state_error());
            }
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            if header.AceType == ACCESS_DENIED_ACE_TYPE as u8 {
                continue;
            }
            if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8 {
                return Err(state_error());
            }
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let sid = sid_text((&allowed.SidStart as *const u32).cast_mut().cast())?;
            if sid != expected && !matches!(sid.as_str(), "S-1-5-18" | "S-1-5-32-544") {
                return Err(state_error());
            }
        }
        Ok(())
    })();
    unsafe {
        LocalFree(descriptor);
    }
    result
}
