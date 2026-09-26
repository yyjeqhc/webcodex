//! Native SCM backend. Requires windows-sys features Win32_Foundation,
//! Win32_System_Services, Win32_Security, Win32_Security_Authorization.
use super::*;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSidToSidW, GetEffectiveRightsFromAclW,
    GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
    GRANT_ACCESS, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, LookupAccountSidW, TokenUser, CONTAINER_INHERIT_ACE,
    DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ALL_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT,
};
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

pub(super) fn current_account() -> Result<CurrentAccount, ServiceError> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "OpenProcessToken",
        ));
    }
    struct Token(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let token = Token(token);
    let mut needed = 0u32;
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
    }
    if needed == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "GetTokenInformation size",
        ));
    }
    let mut buf = vec![0u64; needed.div_ceil(8) as usize];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buf.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "GetTokenInformation",
        ));
    }
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut sid_string: *mut u16 = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "ConvertSidToStringSid",
        ));
    }
    let identity = read_wide(sid_string);
    unsafe {
        LocalFree(sid_string.cast());
    }
    let mut name_len = 0u32;
    let mut domain_len = 0u32;
    let mut use_kind: SID_NAME_USE = 0;
    unsafe {
        LookupAccountSidW(
            null(),
            sid,
            null_mut(),
            &mut name_len,
            null_mut(),
            &mut domain_len,
            &mut use_kind,
        );
    }
    if name_len == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "LookupAccountSid size",
        ));
    }
    let mut name = vec![0u16; name_len as usize];
    let mut domain = vec![0u16; domain_len as usize];
    if unsafe {
        LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut use_kind,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "LookupAccountSid",
        ));
    }
    let principal = format!(
        "{}\\{}",
        read_wide(domain.as_ptr()),
        read_wide(name.as_ptr())
    );
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default();
    if home.as_os_str().is_empty() || !home.is_absolute() {
        return Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "current Windows user profile path is unavailable",
        ));
    }
    Ok(CurrentAccount {
        name: principal,
        identity,
        home,
    })
}

pub(super) fn grant_service_directory(spec: &ServiceSpec, path: &Path) -> Result<(), ServiceError> {
    let ServiceAccount::WindowsVirtual { name } = &spec.account else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "directory ACL helper is only for a Server/Tunnel virtual account",
        ));
    };
    if spec.component == Component::Runner {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner project directory ACL must not be rewritten",
        ));
    }
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running != Some(false) {
        return Err(ServiceError::new(
            ServiceErrorCode::Busy,
            "stop service before changing its data ACL",
        ));
    }
    let root = path.canonicalize().map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service data directory is absent",
        )
    })?;
    let working = spec.working_directory.canonicalize().map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service working directory is absent",
        )
    })?;
    if !root.starts_with(&working) {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "ACL target must be within the dedicated service working directory",
        ));
    }
    let mut paths = Vec::new();
    collect_safe_tree(&root, &mut paths)?;
    let service_sid = account_sid(name)?;
    let sid_text = wide(&service_sid)?;
    let mut service_sid_ptr = null_mut();
    if unsafe { ConvertStringSidToSidW(sid_text.as_ptr(), &mut service_sid_ptr) } == 0 {
        return Err(error(
            ServiceErrorCode::OperationFailed,
            "ConvertStringSidToSid",
        ));
    }
    struct Sid(*mut c_void);
    impl Drop for Sid {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let service_sid = Sid(service_sid_ptr);
    for item in paths {
        let item_meta = std::fs::symlink_metadata(&item).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "service data entry disappeared during ACL update",
            )
        })?;
        if item_meta.file_type().is_symlink()
            || item_meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "service data entry became a reparse point during ACL update",
            ));
        }
        let name = wide_path(&item)?;
        let mut old_acl = null_mut();
        let mut old_descriptor = null_mut();
        let get = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut old_acl,
                null_mut(),
                &mut old_descriptor,
            )
        };
        if get != 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                format!(
                    "cannot read service data ACL (Windows error {get}); inspect before retrying"
                ),
            ));
        }
        struct Descriptor(*mut c_void);
        impl Drop for Descriptor {
            fn drop(&mut self) {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
        let _descriptor = Descriptor(old_descriptor);
        let trustee = TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: service_sid.0.cast(),
        };
        if old_acl.is_null() {
            // A null DACL grants all access; do not turn it into a narrower ACL.
            continue;
        }
        let mut effective = 0u32;
        let rights = unsafe { GetEffectiveRightsFromAclW(old_acl, &trustee, &mut effective) };
        if rights == 0 && effective & FILE_ALL_ACCESS == FILE_ALL_ACCESS {
            continue;
        }
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: if item_meta.is_dir() {
                OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
            } else {
                0
            },
            Trustee: trustee,
        };
        let mut new_acl = null_mut();
        let merge = unsafe { SetEntriesInAclW(1, &entry, old_acl, &mut new_acl) };
        if merge != 0 {
            return Err(ServiceError::new(ServiceErrorCode::OutcomeUnknown,
                format!("cannot extend service data ACL (Windows error {merge}); inspect before retrying")));
        }
        struct Acl(*mut c_void);
        impl Drop for Acl {
            fn drop(&mut self) {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
        let new_acl = Acl(new_acl.cast());
        let code = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                new_acl.0.cast(),
                null(),
            )
        };
        if code != 0 {
            return Err(ServiceError::new(ServiceErrorCode::OutcomeUnknown,
                format!("service data ACL update failed (Windows error {code}); inspect access before retrying")));
        }
    }
    Ok(())
}

fn collect_safe_tree(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), ServiceError> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "cannot inspect service data path",
        )
    })?;
    if meta.file_type().is_symlink()
        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || !(meta.is_file() || meta.is_dir())
    {
        return Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "service data tree contains a reparse point or special file",
        ));
    }
    out.push(path.to_path_buf());
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "cannot enumerate service data directory",
            )
        })? {
            let entry = entry.map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "cannot enumerate service data entry",
                )
            })?;
            collect_safe_tree(&entry.path(), out)?;
        }
    }
    Ok(())
}

struct Handle(SC_HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

fn wide(value: &str) -> Result<Vec<u16>, ServiceError> {
    if value.contains('\0') {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Windows service value contains NUL",
        ));
    }
    Ok(value.encode_utf16().chain(std::iter::once(0)).collect())
}

fn wide_path(path: &Path) -> Result<Vec<u16>, ServiceError> {
    if !path.is_absolute() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Windows service path must be absolute",
        ));
    }
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Windows service path contains NUL",
        ));
    }
    units.push(0);
    Ok(units)
}

fn error(code: ServiceErrorCode, action: &str) -> ServiceError {
    ServiceError::new(
        code,
        format!("{action} failed (Windows error {})", unsafe {
            GetLastError()
        }),
    )
}

fn manager(access: u32) -> Result<Handle, ServiceError> {
    let handle = unsafe { OpenSCManagerW(null(), null(), access) };
    if handle.is_null() {
        return Err(error(ServiceErrorCode::PermissionDenied, "OpenSCManager"));
    }
    Ok(Handle(handle))
}

fn open(scm: &Handle, spec: &ServiceSpec, access: u32) -> Result<Option<Handle>, ServiceError> {
    let name = wide(&spec.id)?;
    let handle = unsafe { OpenServiceW(scm.0, name.as_ptr(), access) };
    if !handle.is_null() {
        return Ok(Some(Handle(handle)));
    }
    if unsafe { GetLastError() } == ERROR_SERVICE_DOES_NOT_EXIST {
        return Ok(None);
    }
    Err(error(ServiceErrorCode::OwnershipUnknown, "OpenService"))
}

fn read_wide(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 && len < 32768 {
            len += 1;
        }
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

fn query_config(handle: &Handle) -> Result<(String, String, u32), ServiceError> {
    let mut required = 0u32;
    unsafe {
        QueryServiceConfigW(handle.0, null_mut(), 0, &mut required);
    }
    if required == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(error(
            ServiceErrorCode::OwnershipUnknown,
            "QueryServiceConfig size",
        ));
    }
    let mut buf = vec![0u64; required.div_ceil(8) as usize];
    let config = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    if unsafe { QueryServiceConfigW(handle.0, config, required, &mut required) } == 0 {
        return Err(error(
            ServiceErrorCode::OwnershipUnknown,
            "QueryServiceConfig",
        ));
    }
    let config = unsafe { &*config };
    Ok((
        read_wide(config.lpBinaryPathName),
        read_wide(config.lpServiceStartName),
        config.dwStartType,
    ))
}

fn query_description(handle: &Handle) -> Result<String, ServiceError> {
    let mut required = 0u32;
    unsafe {
        QueryServiceConfig2W(
            handle.0,
            SERVICE_CONFIG_DESCRIPTION,
            null_mut(),
            0,
            &mut required,
        );
    }
    if required == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(error(
            ServiceErrorCode::OwnershipUnknown,
            "QueryServiceConfig2 size",
        ));
    }
    let mut buf = vec![0u64; required.div_ceil(8) as usize];
    if unsafe {
        QueryServiceConfig2W(
            handle.0,
            SERVICE_CONFIG_DESCRIPTION,
            buf.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::OwnershipUnknown,
            "QueryServiceConfig2",
        ));
    }
    Ok(read_wide(unsafe {
        (*buf.as_ptr().cast::<SERVICE_DESCRIPTIONW>()).lpDescription
    }))
}

fn query_state(handle: &Handle) -> Result<u32, ServiceError> {
    let mut status = std::mem::MaybeUninit::<SERVICE_STATUS_PROCESS>::zeroed();
    let mut required = 0u32;
    if unsafe {
        QueryServiceStatusEx(
            handle.0,
            SC_STATUS_PROCESS_INFO,
            status.as_mut_ptr().cast(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut required,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::OwnershipUnknown,
            "QueryServiceStatusEx",
        ));
    }
    Ok(unsafe { status.assume_init().dwCurrentState })
}

fn wait_for_state(handle: &Handle, desired: u32) -> Result<(), ServiceError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let state = query_state(handle)?;
        if state == desired {
            return Ok(());
        }
        if state == SERVICE_STOPPED && desired == SERVICE_RUNNING
            || std::time::Instant::now() >= deadline
        {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "SCM control did not reach the requested state; inspect before retrying",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        match ch {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(slashes));
                slashes = 0;
                out.push(ch);
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

fn command_line(spec: &ServiceSpec) -> Result<String, ServiceError> {
    let program = spec.program.to_str().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Windows executable path is not UTF-8",
        )
    })?;
    let mut command = quote(program);
    for arg in &spec.args {
        command.push(' ');
        command.push_str(&quote(arg));
    }
    Ok(command)
}

fn account_name(spec: &ServiceSpec) -> Result<&str, ServiceError> {
    match &spec.account {
        ServiceAccount::SystemUser { name, .. } => Ok(name),
        ServiceAccount::WindowsVirtual { name } => {
            let expected = format!("NT SERVICE\\{}", spec.id);
            if !name.eq_ignore_ascii_case(&expected) || spec.component == Component::Runner {
                return Err(ServiceError::new(
                    ServiceErrorCode::InvalidSpec,
                    "virtual account must match non-Runner service ID",
                ));
            }
            Ok(name)
        }
    }
}

fn account_sid(name: &str) -> Result<String, ServiceError> {
    let name = wide(name)?;
    let mut sid_len = 0u32;
    let mut domain_len = 0u32;
    let mut use_kind: SID_NAME_USE = 0;
    unsafe {
        LookupAccountNameW(
            null(),
            name.as_ptr(),
            null_mut(),
            &mut sid_len,
            null_mut(),
            &mut domain_len,
            &mut use_kind,
        );
    }
    if sid_len == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "LookupAccountName",
        ));
    }
    let mut sid = vec![0u64; sid_len.div_ceil(8) as usize];
    let mut domain = vec![0u16; domain_len as usize];
    if unsafe {
        LookupAccountNameW(
            null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast::<c_void>(),
            &mut sid_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut use_kind,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "LookupAccountName",
        ));
    }
    let mut sid_string: *mut u16 = null_mut();
    if unsafe { ConvertSidToStringSidW(sid.as_mut_ptr().cast::<c_void>(), &mut sid_string) } == 0 {
        return Err(error(
            ServiceErrorCode::MissingPrerequisite,
            "ConvertSidToStringSid",
        ));
    }
    let value = read_wide(sid_string);
    unsafe {
        LocalFree(sid_string.cast());
    }
    Ok(value)
}

fn configuration_owned_by_spec(
    spec: &ServiceSpec,
    binary: &str,
    account: &str,
    description: &str,
) -> Result<bool, ServiceError> {
    let expected_account = account_name(spec)?;
    let mut owned = binary == command_line(spec)?
        && account.eq_ignore_ascii_case(expected_account)
        && description == ownership_marker(spec);
    if let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &spec.account
    {
        owned &= account_sid(&account)?.eq_ignore_ascii_case(expected_identity);
    }
    Ok(owned)
}

pub(super) fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let scm = manager(SC_MANAGER_CONNECT)?;
    let Some(handle) = open(&scm, spec, SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS)? else {
        return Ok(ServiceStatus::absent(&spec.id));
    };
    let (binary, account, start_type) = query_config(&handle)?;
    let description = query_description(&handle)?;
    // A prepared Runner intentionally has SERVICE_DISABLED. Startup policy is
    // reported separately; it is not service ownership.
    let owned = configuration_owned_by_spec(spec, &binary, &account, &description)?;
    Ok(ServiceStatus {
        id: spec.id.clone(),
        ownership: if owned {
            Ownership::Owned
        } else {
            Ownership::Foreign
        },
        installed: true,
        enabled: Some(start_type == SERVICE_AUTO_START),
        running: Some(query_state(&handle)? == SERVICE_RUNNING),
        detail: None,
    })
}

pub(super) fn preflight(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    check_files(spec)?;
    if !spec.environment.is_empty() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Windows SCM cannot set per-service environment; use runtime configuration",
        ));
    }
    if let Some(env_file) = &spec.env_file {
        if !spec
            .args
            .iter()
            .any(|arg| arg == &env_file.to_string_lossy())
        {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Windows SCM does not load env files; runtime arguments must name the env file",
            ));
        }
    }
    let account = account_name(spec)?;
    if let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &spec.account
    {
        if !account_sid(account)?.eq_ignore_ascii_case(expected_identity) {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "service account SID differs from selected project user",
            ));
        }
    }
    let status = inspect(spec)?;
    match status.ownership {
        Ownership::Absent | Ownership::Owned => Ok(status),
        Ownership::Foreign => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "SCM service exists but owner/configuration does not match",
        )),
        Ownership::Unknown => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "SCM service ownership is unknown",
        )),
    }
}

pub(super) fn install(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
) -> Result<ServiceStatus, ServiceError> {
    install_inner(spec, credential, false)
}

pub(super) fn prepare_runner(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
) -> Result<ServiceStatus, ServiceError> {
    if spec.component != Component::Runner {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "only a Runner can reserve service credentials before pairing",
        ));
    }
    install_inner(spec, credential, true)
}

fn install_inner(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
    prepare: bool,
) -> Result<ServiceStatus, ServiceError> {
    let current = preflight(spec)?;
    if current.ownership == Ownership::Owned {
        if !prepare && current.enabled != Some(true) {
            check_install_files(spec)?;
            let scm = manager(SC_MANAGER_CONNECT)?;
            let handle = open(&scm, spec, SERVICE_CHANGE_CONFIG)?.ok_or_else(|| {
                ServiceError::new(
                    ServiceErrorCode::OutcomeUnknown,
                    "service disappeared before enabling",
                )
            })?;
            if unsafe {
                ChangeServiceConfigW(
                    handle.0,
                    SERVICE_NO_CHANGE,
                    SERVICE_AUTO_START,
                    SERVICE_NO_CHANGE,
                    null(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                )
            } == 0
            {
                return Err(error(ServiceErrorCode::OutcomeUnknown, "enable service"));
            }
            return inspect(spec);
        }
        return Ok(current);
    }
    if !prepare {
        check_install_files(spec)?;
    }
    if matches!(spec.account, ServiceAccount::SystemUser { .. }) && credential.is_none() {
        return Err(ServiceError::new(
            ServiceErrorCode::CredentialRequired,
            "real Windows service account password is required for SCM",
        ));
    }
    if matches!(spec.account, ServiceAccount::WindowsVirtual { .. }) && credential.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "virtual service accounts must not receive a password",
        ));
    }
    let scm = manager(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)?;
    // A second existence check closes the common preflight/create race. SCM's
    // CreateService still atomically rejects another installer winning later.
    if open(&scm, spec, SERVICE_QUERY_CONFIG)?.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "service appeared during install; inspect before retrying",
        ));
    }
    let name = wide(&spec.id)?;
    let binary = wide(&command_line(spec)?)?;
    let account = wide(account_name(spec)?)?;
    let password = credential.map_or(null(), ServiceCredential::as_ptr);
    let raw = unsafe {
        CreateServiceW(
            scm.0,
            name.as_ptr(),
            name.as_ptr(),
            SERVICE_QUERY_CONFIG
                | SERVICE_QUERY_STATUS
                | SERVICE_CHANGE_CONFIG
                | SERVICE_START
                | SERVICE_STOP
                | DELETE,
            SERVICE_WIN32_OWN_PROCESS,
            if prepare {
                SERVICE_DISABLED
            } else {
                SERVICE_AUTO_START
            },
            SERVICE_ERROR_NORMAL,
            binary.as_ptr(),
            null(),
            null_mut(),
            null(),
            account.as_ptr(),
            password,
        )
    };
    if raw.is_null() {
        return Err(error(ServiceErrorCode::OutcomeUnknown, "CreateService"));
    }
    let handle = Handle(raw);
    let mut description = wide(&ownership_marker(spec))?;
    let mut desc = SERVICE_DESCRIPTIONW {
        lpDescription: description.as_mut_ptr(),
    };
    if unsafe {
        ChangeServiceConfig2W(
            handle.0,
            SERVICE_CONFIG_DESCRIPTION,
            (&mut desc as *mut SERVICE_DESCRIPTIONW).cast(),
        )
    } == 0
    {
        unsafe {
            DeleteService(handle.0);
        }
        return Err(ServiceError::new(ServiceErrorCode::OutcomeUnknown, "service created but owner description failed; deletion attempted; inspect before retrying"));
    }
    inspect(spec)
}

fn control(spec: &ServiceSpec, action: &str) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if action == "start" && status.running == Some(true)
        || action == "stop" && status.running == Some(false)
    {
        return Ok(status);
    }
    let scm = manager(SC_MANAGER_CONNECT)?;
    let access = if action == "stop" {
        SERVICE_STOP
    } else {
        SERVICE_START
    };
    let handle = open(&scm, spec, access | SERVICE_QUERY_STATUS)?.ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "service disappeared before control",
        )
    })?;
    let result = if action == "stop" {
        let mut state = std::mem::MaybeUninit::<SERVICE_STATUS>::zeroed();
        unsafe { ControlService(handle.0, SERVICE_CONTROL_STOP, state.as_mut_ptr()) }
    } else {
        unsafe { StartServiceW(handle.0, 0, null()) }
    };
    if result == 0 {
        return Err(error(ServiceErrorCode::OutcomeUnknown, action));
    }
    wait_for_state(
        &handle,
        if action == "stop" {
            SERVICE_STOPPED
        } else {
            SERVICE_RUNNING
        },
    )?;
    inspect(spec)
}

pub(super) fn start(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    control(spec, "start")
}
pub(super) fn stop(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    control(spec, "stop")
}
pub(super) fn restart(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running == Some(true) {
        stop(spec)?;
    }
    start(spec)
}

pub(super) fn uninstall(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    if status.ownership == Ownership::Absent {
        return Ok(status);
    }
    check_owned(&status)?;
    if status.running != Some(false) {
        return Err(ServiceError::new(
            ServiceErrorCode::Busy,
            "stop service explicitly before uninstall",
        ));
    }
    let scm = manager(SC_MANAGER_CONNECT)?;
    let handle = open(&scm, spec, DELETE)?.ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "service disappeared before uninstall",
        )
    })?;
    if unsafe { DeleteService(handle.0) } == 0 {
        return Err(error(ServiceErrorCode::OutcomeUnknown, "DeleteService"));
    }
    drop(handle);
    inspect(spec)
}

pub(super) fn update_credential(
    spec: &ServiceSpec,
    credential: &ServiceCredential,
) -> Result<ServiceStatus, ServiceError> {
    if !matches!(spec.account, ServiceAccount::SystemUser { .. }) {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "virtual service account has no rotatable password",
        ));
    }
    let status = inspect(spec)?;
    check_owned(&status)?;
    let scm = manager(SC_MANAGER_CONNECT)?;
    let handle = open(&scm, spec, SERVICE_CHANGE_CONFIG)?.ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "service disappeared before credential update",
        )
    })?;
    let account = wide(account_name(spec)?)?;
    if unsafe {
        ChangeServiceConfigW(
            handle.0,
            SERVICE_NO_CHANGE,
            SERVICE_NO_CHANGE,
            SERVICE_NO_CHANGE,
            null(),
            null(),
            null_mut(),
            null(),
            account.as_ptr(),
            credential.as_ptr(),
            null(),
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::OutcomeUnknown,
            "ChangeServiceConfig credential update",
        ));
    }
    inspect(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_command_line_quotes_backslashes_before_quote() {
        assert_eq!(quote(r#"C:\a\"b"#), r#""C:\a\\\"b""#);
    }

    #[test]
    fn prepared_disabled_service_still_has_its_configuration_owner() {
        let spec = ServiceSpec {
            id: "WebCodexServer-test".into(),
            component: Component::Server,
            program: PathBuf::from(r"C:\WebCodex\webcodex-server.exe"),
            args: vec!["--windows-service".into(), "WebCodexServer-test".into()],
            working_directory: PathBuf::from(r"C:\ProgramData\WebCodex"),
            account: ServiceAccount::WindowsVirtual {
                name: r"NT SERVICE\WebCodexServer-test".into(),
            },
            config_identity: "test-environment".into(),
            env_file: None,
            environment: Default::default(),
            linux_socket: None,
        };
        let command = command_line(&spec).unwrap();
        let marker = ownership_marker(&spec);
        assert!(configuration_owned_by_spec(
            &spec,
            &command,
            r"NT SERVICE\WebCodexServer-test",
            &marker
        )
        .unwrap());
        assert!(
            !configuration_owned_by_spec(&spec, &command, r"NT SERVICE\Other", &marker).unwrap()
        );
        // There is intentionally no start-type argument: disabled reservation
        // and auto-start installation have the same owner identity.
    }
}

/// Verify a real service logon before SCM receives the password. The returned
/// token is also used for a project-access probe; display names are never trusted.
pub(super) fn validate_credential(
    spec: &ServiceSpec,
    credential: &ServiceCredential,
    project: Option<&Path>,
) -> Result<(), ServiceError> {
    use windows_sys::Win32::Security::Authentication::Identity::{
        LsaAddAccountRights, LsaClose, LsaOpenPolicy, LSA_OBJECT_ATTRIBUTES, LSA_UNICODE_STRING,
        POLICY_CREATE_ACCOUNT, POLICY_LOOKUP_NAMES,
    };
    use windows_sys::Win32::Security::{
        ImpersonateLoggedOnUser, LogonUserW, RevertToSelf, LOGON32_LOGON_NETWORK,
        LOGON32_LOGON_SERVICE, LOGON32_PROVIDER_DEFAULT,
    };
    let ServiceAccount::SystemUser {
        name,
        expected_identity,
        ..
    } = &spec.account
    else {
        return Ok(());
    };
    let (domain, username) = name
        .split_once('\\')
        .map(|(d, n)| (Some(d), n))
        .unwrap_or((None, name.as_str()));
    let username = wide(username)?;
    let domain = domain.map(wide).transpose()?;
    let login = |kind| {
        let mut token = null_mut();
        let ok = unsafe {
            LogonUserW(
                username.as_ptr(),
                domain.as_ref().map_or(null(), |v| v.as_ptr()),
                credential.as_ptr(),
                kind,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
        };
        if ok != 0 {
            Ok(token)
        } else {
            Err(unsafe { GetLastError() })
        }
    };
    // Network validation checks the password without granting an interactive
    // session or relying on a PIN. Grant only the one requested service right.
    let token = match login(LOGON32_LOGON_SERVICE) {
        Ok(token) => token,
        Err(1385) => {
            let probe = login(LOGON32_LOGON_NETWORK).map_err(|_| ServiceError::new(ServiceErrorCode::CredentialRequired, "account password could not be verified; Windows Hello PIN is not accepted"))?;
            unsafe { CloseHandle(probe); }
            let sid_text = wide(expected_identity)?;
            let mut sid = null_mut();
            if unsafe { ConvertStringSidToSidW(sid_text.as_ptr(), &mut sid) } == 0 { return Err(error(ServiceErrorCode::InvalidSpec, "account SID")); }
            let mut attributes: LSA_OBJECT_ATTRIBUTES = unsafe { std::mem::zeroed() };
            attributes.Length = std::mem::size_of_val(&attributes) as u32;
            let mut policy: isize = 0;
            let opened = unsafe { LsaOpenPolicy(null(), &attributes, (POLICY_CREATE_ACCOUNT | POLICY_LOOKUP_NAMES) as u32, &mut policy) };
            let mut right: Vec<u16> = "SeServiceLogonRight".encode_utf16().collect();
            let right = LSA_UNICODE_STRING { Length: (right.len() * 2) as u16, MaximumLength: (right.len() * 2) as u16, Buffer: right.as_mut_ptr() };
            let assigned = if opened == 0 { unsafe { LsaAddAccountRights(policy, sid, &right, 1) } } else { opened };
            unsafe { if policy != 0 { LsaClose(policy); } LocalFree(sid); }
            if assigned != 0 { return Err(ServiceError::new(ServiceErrorCode::PermissionDenied, "cannot grant the selected user service logon rights")); }
            login(LOGON32_LOGON_SERVICE).map_err(|_| ServiceError::new(ServiceErrorCode::CredentialRequired, "Windows policy denies service logon; review deny-logon policy and the account password"))?
        }
        Err(_) => return Err(ServiceError::new(ServiceErrorCode::CredentialRequired, "service account password was rejected; update SCM credentials, not a Windows Hello PIN")),
    };
    struct Login(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Login {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let token = Login(token);
    let mut required = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut required);
    }
    let mut buffer = vec![0u64; required.div_ceil(8) as usize];
    if required == 0
        || unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
    {
        return Err(error(
            ServiceErrorCode::PermissionDenied,
            "service login identity",
        ));
    }
    let mut actual = null_mut();
    if unsafe {
        ConvertSidToStringSidW(
            (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid,
            &mut actual,
        )
    } == 0
    {
        return Err(error(
            ServiceErrorCode::PermissionDenied,
            "service login SID",
        ));
    }
    let identity = read_wide(actual);
    unsafe {
        LocalFree(actual.cast());
    }
    if !identity.eq_ignore_ascii_case(expected_identity) {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "service login SID differs from the project owner",
        ));
    }
    if let Some(project) = project {
        if unsafe { ImpersonateLoggedOnUser(token.0) } == 0 {
            return Err(error(
                ServiceErrorCode::PermissionDenied,
                "project access probe",
            ));
        }
        let readable = std::fs::read_dir(project).is_ok();
        if unsafe { RevertToSelf() } == 0 {
            // Continuing in another security context is never safe.
            std::process::abort();
        }
        if !readable {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "the service logon cannot access the selected project directory",
            ));
        }
    }
    Ok(())
}
