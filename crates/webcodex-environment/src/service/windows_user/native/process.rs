//! Query-only Windows process/SID observations. Handles pin the observed object;
//! no termination, VM-write, impersonation, or debug privileges are requested.
use super::{Action, MAX_TEXT};
use std::ffi::c_void;
use std::mem::{size_of, size_of_val, transmute};
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_FILES, FILETIME,
    HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Globalization::CompareStringOrdinal;
use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows_sys::Win32::Security::{
    GetTokenInformation, GetWindowsAccountDomainSid, IsValidSid, LookupAccountNameW, TokenUser,
    SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::SystemInformation::{ComputerNameNetBIOS, GetComputerNameExW};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

#[cfg(test)]
mod tests;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub(super) struct Process {
    handle: Handle,
    pid: u32,
}

#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub pid: u32,
    pub created: u64,
    owner: String,
    image: String,
    argv: Vec<String>,
}

impl Observation {
    fn matches_action(&self, action: &Action) -> bool {
        let Some(expected) = parse_command(&format!("webcodex {}", action.arguments)) else {
            return false;
        };
        self.argv.len() == expected.len()
            && executable_path_matches(&self.image, &action.program)
            && self
                .argv
                .first()
                .is_some_and(|path| executable_path_matches(path, &action.program))
            && self.argv[1..] == expected[1..]
    }

    pub(super) fn matches(&self, action: &Action, sid: &str) -> bool {
        self.owner == sid && self.created != 0 && self.matches_action(action)
    }
}

impl Process {
    pub(super) fn open(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            None
        } else {
            Some(Self {
                handle: Handle(handle),
                pid,
            })
        }
    }

    pub(super) fn is_alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.handle.0, 0) == WAIT_TIMEOUT }
    }

    pub(super) fn read(&self) -> Option<Observation> {
        if !self.is_alive() {
            return None;
        }
        let mut birth: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit = birth;
        let mut kernel = birth;
        let mut user = birth;
        if unsafe { GetProcessTimes(self.handle.0, &mut birth, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return None;
        }
        let created = (u64::from(birth.dwHighDateTime) << 32) | u64::from(birth.dwLowDateTime);
        if created == 0 {
            return None;
        }
        let mut image = vec![0u16; MAX_TEXT + 1];
        let mut size = image.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.handle.0, 0, image.as_mut_ptr(), &mut size) }
            == 0
            || size == 0
            || size as usize > MAX_TEXT
        {
            return None;
        }
        let image = String::from_utf16(&image[..size as usize]).ok()?;
        let argv = parse_command(&command_line(self.handle.0)?)?;
        let owner = process_owner(self.handle.0)?;
        if !self.is_alive() {
            return None;
        }
        Some(Observation {
            pid: self.pid,
            created,
            owner,
            image,
            argv,
        })
    }
}

fn process_owner(process: HANDLE) -> Option<String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = Handle(token);
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
    }
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
        || needed < size_of::<TOKEN_USER>() as u32
        || needed > 4096
    {
        return None;
    }
    // TOKEN_USER contains a pointer, so keep the buffer naturally aligned.
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    let capacity = needed;
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            capacity,
            &mut needed,
        )
    } == 0
        || needed > capacity
    {
        return None;
    }
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    sid_text(sid)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

type QueryProcess = unsafe extern "system" fn(HANDLE, i32, *mut c_void, u32, *mut u32) -> i32;

fn command_line(handle: HANDLE) -> Option<String> {
    // ProcessCommandLineInformation (60) is supported on our Windows 10 target,
    // but is an internal information class, not a stable Win32 contract. Follow
    // Microsoft's dynamic-link guidance. Unavailable/changed ABI means an
    // unconfirmed PID; never read arbitrary PEB layouts or launch a fallback.
    // https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntqueryinformationprocess
    let module = unsafe { GetModuleHandleW(wide("ntdll.dll")?.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let address = unsafe { GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast()) }?;
    // The exported NT ABI has the fixed signature above on both 32/64-bit Windows.
    let query = unsafe { transmute::<unsafe extern "system" fn() -> isize, QueryProcess>(address) };
    let mut needed = 0u32;
    let status = unsafe { query(handle, 60, null_mut(), 0, &mut needed) };
    const LENGTH_MISMATCH: i32 = 0xc0000004u32 as i32;
    const BUFFER_TOO_SMALL: i32 = 0xc0000023u32 as i32;
    let maximum = size_of::<UnicodeString>() + (MAX_TEXT + 1) * 2;
    if !matches!(status, LENGTH_MISMATCH | BUFFER_TOO_SMALL)
        || needed < size_of::<UnicodeString>() as u32
        || needed as usize > maximum
    {
        return None;
    }
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    let mut returned = 0;
    if unsafe {
        query(
            handle,
            60,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut returned,
        )
    } < 0
        || returned < size_of::<UnicodeString>() as u32
        || returned > needed
    {
        return None;
    }
    // Returned pointers must refer into this output buffer, not target memory.
    let descriptor = unsafe { *buffer.as_ptr().cast::<UnicodeString>() };
    let start = buffer.as_ptr() as usize;
    let pointer = descriptor.buffer as usize;
    let length = usize::from(descriptor.length);
    if length == 0
        || length % 2 != 0
        || length / 2 > MAX_TEXT
        || descriptor.maximum_length < descriptor.length
        || pointer % std::mem::align_of::<u16>() != 0
        || pointer < start.checked_add(size_of::<UnicodeString>())?
        || pointer.checked_add(length)? > start.checked_add(returned as usize)?
    {
        return None;
    }
    let value = unsafe { std::slice::from_raw_parts(descriptor.buffer, length / 2) };
    if value.contains(&0) {
        return None;
    }
    String::from_utf16(value).ok()
}

pub(super) fn parse_command(command: &str) -> Option<Vec<String>> {
    if command.is_empty() || command.encode_utf16().count() > MAX_TEXT {
        return None;
    }
    let command = wide(command)?;
    let mut count = 0;
    let values = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
    if values.is_null() {
        return None;
    }
    let _allocation = Local(values.cast());
    if !(1..=4096).contains(&count) {
        return None;
    }
    let mut total = 0;
    let mut result = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        // CommandLineToArgvW owns these NUL-terminated strings until LocalFree.
        let pointer = unsafe { *values.add(index) };
        if pointer.is_null() {
            return None;
        }
        let mut len = 0;
        while len <= MAX_TEXT && unsafe { *pointer.add(len) } != 0 {
            len += 1;
        }
        total += len;
        if total > MAX_TEXT {
            return None;
        }
        result.push(String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, len) }).ok()?);
    }
    Some(result)
}

pub(super) fn matching_child(parent: u32, action: &Action, deadline: Instant) -> Option<Process> {
    let children = child_pids(parent, deadline)?;
    let mut matched = None;
    for pid in children {
        if Instant::now() >= deadline {
            return None;
        }
        // An unreadable direct child can hide another match; fail closed rather
        // than treating inaccessible command lines as evidence of non-identity.
        let child = Process::open(pid)?;
        if child.read()?.matches_action(action) {
            if matched.is_some() {
                return None;
            }
            matched = Some(child);
        }
    }
    matched
}

fn child_pids(parent: u32, deadline: Instant) -> Option<Vec<u32>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return None;
    }
    let snapshot = Handle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut result = Vec::new();
    let mut available = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    let mut visited = 0;
    while available != 0 {
        visited += 1;
        if visited > 65536 || Instant::now() >= deadline {
            return None;
        }
        if entry.th32ParentProcessID == parent {
            if entry.th32ProcessID == 0 || result.len() >= 4096 {
                return None;
            }
            result.push(entry.th32ProcessID);
        }
        available = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
        return None;
    }
    Some(result)
}

fn wide(value: &str) -> Option<Vec<u16>> {
    if value.contains('\0') || value.encode_utf16().count() > MAX_TEXT {
        return None;
    }
    Some(value.encode_utf16().chain(Some(0)).collect())
}

pub(super) fn equal_ignore_case(left: &str, right: &str) -> bool {
    let left: Vec<u16> = left.encode_utf16().collect();
    let right: Vec<u16> = right.encode_utf16().collect();
    if left.len() > MAX_TEXT || right.len() > MAX_TEXT {
        return false;
    }
    // Match .NET OrdinalIgnoreCase, including non-ASCII executable names.
    unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        ) == 2
    }
}

pub(super) fn executable_path_matches(observed: &str, expected: &str) -> bool {
    if observed.is_empty() || expected.is_empty() {
        return false;
    }
    if equal_ignore_case(observed, expected) {
        return true;
    }
    let prefix = r"\\?\";
    let (normal, extended) = match (observed.strip_prefix(prefix), expected.strip_prefix(prefix)) {
        (Some(extended), None) => (expected, extended),
        (None, Some(extended)) => (observed, extended),
        _ => return false,
    };
    equal_ignore_case(normal, extended) && unambiguous_dos_path(normal)
}

fn unambiguous_dos_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
        return false;
    }
    path[3..].split('\\').all(|part| {
        if part.is_empty()
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|ch| ch <= '\u{1f}' || "<>:\"/|?*".contains(ch))
        {
            return false;
        }
        let name = part
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        !matches!(
            name.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) && !["COM", "LPT"].iter().any(|prefix| {
            name.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
    })
}

pub(super) fn computer_name() -> Option<String> {
    let mut buffer = [0u16; 256];
    let mut size = buffer.len() as u32;
    if unsafe { GetComputerNameExW(ComputerNameNetBIOS, buffer.as_mut_ptr(), &mut size) } == 0
        || size == 0
        || size as usize >= buffer.len()
    {
        return None;
    }
    String::from_utf16(&buffer[..size as usize]).ok()
}

struct Sid {
    text: String,
    domain: Option<String>,
}

pub(super) fn account_matches(identity: &str, expected: &str, computer: &str) -> bool {
    account_matches_with(identity, expected, computer, resolve_account)
}

fn account_matches_with(
    identity: &str,
    expected: &str,
    computer: &str,
    mut resolve: impl FnMut(&str) -> Option<Sid>,
) -> bool {
    let resolved = resolve(identity);
    if resolved.as_ref().is_some_and(|sid| sid.text == expected) {
        return true;
    }
    // Preserve only the bare computer/local-user name collision exception. No
    // qualified account, UPN, different user/group, or foreign domain is adopted.
    if identity.trim().is_empty()
        || identity.contains(['\\', '/', '@'])
        || identity
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("S-1-"))
        || !equal_ignore_case(identity, computer)
    {
        return false;
    }
    let Some(local) = resolve(&format!("{computer}\\{identity}")) else {
        return false;
    };
    local.text == expected && resolved.is_none_or(|sid| local.domain.as_ref() == Some(&sid.text))
}

fn resolve_account(identity: &str) -> Option<Sid> {
    if identity.trim().is_empty() {
        return None;
    }
    let name = wide(identity)?;
    if identity
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("S-1-"))
    {
        let mut sid = null_mut();
        if unsafe { ConvertStringSidToSidW(name.as_ptr(), &mut sid) } == 0 {
            return None;
        }
        let _allocation = Local(sid);
        return Some(Sid {
            text: sid_text(sid)?,
            domain: domain_sid(sid),
        });
    }
    let mut sid_len = 0;
    let mut domain_len = 0;
    let mut kind: SID_NAME_USE = 0;
    unsafe {
        LookupAccountNameW(
            null(),
            name.as_ptr(),
            null_mut(),
            &mut sid_len,
            null_mut(),
            &mut domain_len,
            &mut kind,
        );
    }
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
        || sid_len == 0
        || sid_len > 68
        || domain_len as usize > MAX_TEXT
    {
        return None;
    }
    let mut sid = vec![0usize; (sid_len as usize).div_ceil(size_of::<usize>())];
    let mut domain = vec![0u16; domain_len as usize];
    if unsafe {
        LookupAccountNameW(
            null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    } == 0
    {
        return None;
    }
    Some(Sid {
        text: sid_text(sid.as_mut_ptr().cast())?,
        domain: domain_sid(sid.as_mut_ptr().cast()),
    })
}

fn domain_sid(sid: *mut c_void) -> Option<String> {
    let mut domain = [0usize; 68usize.div_ceil(size_of::<usize>())];
    let mut size = size_of_val(&domain) as u32;
    if unsafe { GetWindowsAccountDomainSid(sid, domain.as_mut_ptr().cast(), &mut size) } == 0 {
        return None;
    }
    sid_text(domain.as_mut_ptr().cast())
}

fn sid_text(sid: *mut c_void) -> Option<String> {
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return None;
    }
    let mut value = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut value) } == 0 || value.is_null() {
        return None;
    }
    let _allocation = Local(value.cast());
    let mut len = 0;
    // A textual SID is at most 184 UTF-16 code units including its terminator.
    while len < 184 && unsafe { *value.add(len) } != 0 {
        len += 1;
    }
    if len == 0 || len == 184 {
        return None;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(value, len) }).ok()
}
