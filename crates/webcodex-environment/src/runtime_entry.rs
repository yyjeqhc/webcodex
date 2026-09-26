//! Parsing and file checks for private Windows SCM entrypoints.
use std::path::Path;

/// Consume the internal prefix before any ordinary command parser runs.
/// An embedded flag is always an error, including on non-Windows platforms.
pub fn split_windows_service_args(
    args: &[String],
) -> Result<Option<(String, Vec<String>)>, String> {
    if args.first().is_some_and(|arg| arg == "--windows-service") {
        if !cfg!(windows) {
            return Err("--windows-service is only available on Windows".into());
        }
        let name = args
            .get(1)
            .ok_or("--windows-service requires a service name")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("invalid Windows service name".into());
        }
        let remaining = args[2..].to_vec();
        if remaining.iter().any(|arg| arg == "--windows-service") {
            return Err("duplicate --windows-service flag".into());
        }
        return Ok(Some((name.clone(), remaining)));
    }
    if args.iter().any(|arg| arg == "--windows-service") {
        return Err("--windows-service must be the first argument".into());
    }
    Ok(None)
}

/// Validate the installer-managed env file before its values enter a process.
/// SCM virtual accounts need access through their service SID, not file ownership.
pub fn validate_service_env_file(path: &Path) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "service env file is unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err("service env file is not a bounded regular file".into());
    }
    #[cfg(windows)]
    validate_windows_env_acl(path)?;
    let file = std::fs::File::open(path).map_err(|_| "service env file is unreadable")?;
    // Ensure an ACL change or broken file is detected before any env mutation.
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "service env file is unreadable")?;
    if bytes.len() > 1024 * 1024 || std::str::from_utf8(&bytes).is_err() {
        return Err("service env file is invalid".into());
    }
    Ok(())
}

#[cfg(windows)]
pub fn validate_windows_env_acl(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{LocalFree, GENERIC_ALL, GENERIC_READ};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSidToSidW, GetEffectiveRightsFromAclW, GetNamedSecurityInfoW,
        NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_GROUP, TRUSTEE_IS_SID, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        EqualSid, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_READ_DATA};

    if std::fs::symlink_metadata(path)
        .map_err(|_| "service env file is unavailable")?
        .file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err("service env file must not be a reparse point".into());
    }
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = null_mut();
    let mut acl = null_mut();
    let mut descriptor = null_mut();
    let result = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err("service env file security could not be inspected".into());
    }
    let mut valid = !owner.is_null() && !acl.is_null();
    if valid {
        // World, Authenticated Users and local Users must not read or own a
        // service credential file. The installer grants the service SID read
        // access without requiring the virtual account to own this file.
        for sid_text in ["S-1-1-0", "S-1-5-11", "S-1-5-32-545"] {
            let wide: Vec<u16> = sid_text.encode_utf16().chain(Some(0)).collect();
            let mut sid = null_mut();
            if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 {
                valid = false;
                break;
            }
            let trustee = TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                ptstrName: sid.cast(),
            };
            let mut rights = 0;
            let status = unsafe { GetEffectiveRightsFromAclW(acl, &trustee, &mut rights) };
            if status != 0
                || rights & (FILE_READ_DATA | GENERIC_READ | GENERIC_ALL) != 0
                || unsafe { EqualSid(owner, sid) } != 0
            {
                valid = false;
            }
            unsafe {
                LocalFree(sid);
            }
            if !valid {
                break;
            }
        }
    }
    unsafe {
        LocalFree(descriptor);
    }
    if !valid {
        return Err("service env file ACL permits broad access or has no valid owner".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn misplaced_or_incomplete_service_prefix_fails_closed() {
        assert!(split_windows_service_args(&[
            "run".into(),
            "--windows-service".into(),
            "Service".into()
        ])
        .is_err());
        assert!(split_windows_service_args(&["--windows-service".into()]).is_err());
        assert!(
            split_windows_service_args(&["--windows-service".into(), "bad/name".into()]).is_err()
        );
    }
    #[test]
    fn service_env_file_rejects_directory_and_oversize() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_service_env_file(dir.path()).is_err());
        let file = dir.path().join("env");
        std::fs::write(&file, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        assert!(validate_service_env_file(&file).is_err());
    }
}
