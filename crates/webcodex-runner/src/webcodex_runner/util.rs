//! Small transport-agnostic helpers shared across the runner crate.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A program name resolved to a concrete file, with the launch mode made
/// explicit.
///
/// On Windows, `.cmd`/`.bat` batch scripts cannot be launched like native PE
/// executables: they require command-interpreter/script semantics. Callers
/// must preserve this distinction and choose an execution contract explicitly;
/// in particular, the native-argv `run_process` path accepts only
/// [`ResolvedProgram::Native`]. An extensionless POSIX shim (npm-style) must
/// never be selected in place of a valid native program or batch script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedProgram {
    /// Native executable (`.exe`, `.com`, or an extensionless PE image).
    Native(PathBuf),
    /// Batch script (`.cmd` / `.bat`), which requires shell/script semantics.
    Batch(PathBuf),
}

impl ResolvedProgram {
    pub(crate) fn path(&self) -> &Path {
        match self {
            ResolvedProgram::Native(path) | ResolvedProgram::Batch(path) => path,
        }
    }

    pub(crate) fn is_batch(&self) -> bool {
        matches!(self, ResolvedProgram::Batch(_))
    }
}

/// Resolve `name` against a `PATH`-style `OsStr` into a concrete program
/// file, honoring Windows executable semantics.
///
/// - Unix: unchanged historical behavior — first PATH directory containing an
///   executable file named `name`.
/// - Windows: a bare name that already carries a supported extension
///   (`.exe`/`.com`/`.cmd`/`.bat`, case-insensitive) is matched *exactly* in
///   each PATH directory — `PATHEXT` is never appended a second time, so
///   `foo.cmd` finds `foo.cmd`, not `foo.cmd.exe`. A bare name without an
///   extension is searched through `PATHEXT` (in `PATHEXT` order,
///   case-insensitively), filtered to the four supported extensions — `.vbs`,
///   `.js`, and other script-interpreter extensions fail closed instead of
///   being misclassified as native PE programs; an extensionless file is
///   accepted only when it is a real PE image (MZ header), so npm-style POSIX
///   shims are never selected and later fail with `ERROR_BAD_EXE_FORMAT`.
///   `.cmd` / `.bat` resolve to [`ResolvedProgram::Batch`]; unsupported
///   extensions never resolve.
pub(crate) fn resolve_program_in_path(name: &str, path: &OsStr) -> Option<ResolvedProgram> {
    resolve_program_in_path_with_pathext(name, path, None)
}

/// Test seam over [`resolve_program_in_path`]: `pathext_override` supplies
/// the raw `PATHEXT` string instead of the process environment, so ordering
/// and filtering rules can be verified without mutating process-global state.
/// Production always passes `None`.
fn resolve_program_in_path_with_pathext(
    name: &str,
    path: &OsStr,
    pathext_override: Option<&str>,
) -> Option<ResolvedProgram> {
    #[cfg(windows)]
    {
        let candidate = Path::new(name);
        if candidate.components().count() > 1 || candidate.is_absolute() {
            // Path-qualified program: the user named this exact file. If it
            // exists it is used as-is (spawn surfaces real errors); PATHEXT
            // variants are not appended for explicit paths.
            return resolve_absolute_candidate(candidate);
        }
        if supported_extension(candidate) {
            // The caller already named a supported extension: find the exact
            // filename in PATH. PATHEXT is not consulted, so `foo.cmd` never
            // resolves through `foo.cmd.exe` or `foo.cmd.cmd`.
            for directory in std::env::split_paths(path) {
                if let Some(program) = classify_candidate(&directory.join(name)) {
                    return Some(program);
                }
            }
            return None;
        }
        if candidate.extension().is_some() {
            // A name that already carries an unsupported extension fails
            // closed: it is never re-searched through PATHEXT (which would
            // look for `foo.vbs.exe`), and its own extension is not a
            // launchable program.
            return None;
        }
        for directory in std::env::split_paths(path) {
            // PATHEXT candidates first: `foo.cmd` must win over an
            // extensionless `foo` shim in the same directory.
            for extension in pathext_extensions(pathext_override) {
                let candidate = directory.join(format!("{name}.{extension}"));
                if let Some(program) = classify_candidate(&candidate) {
                    return Some(program);
                }
            }
            // Extensionless file: valid only as a real PE image.
            let bare = directory.join(name);
            if is_executable_file(&bare) && is_pe_image(&bare) {
                return Some(ResolvedProgram::Native(bare));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = pathext_override;
        find_executable_in_path(name, path).map(ResolvedProgram::Native)
    }
}

/// Search `path` (a `PATH`-style `OsStr`) for the first directory containing an
/// executable named `name`, and return its full path.
///
/// Unifies the LSP supervisor's `find_executable_in_path` (used for both env
/// `PATH` lookup and profile-path resolution) with the validation executor's
/// `which_in_path`. Callers that need the ambient `PATH` should read
/// `std::env::var_os("PATH")` and pass it here.
pub(crate) fn find_executable_in_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        resolve_program_in_path(name, path).map(|program| program.path().to_path_buf())
    }
    #[cfg(not(windows))]
    {
        for directory in std::env::split_paths(path) {
            let candidate = directory.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

#[cfg(windows)]
fn resolve_absolute_candidate(name: &Path) -> Option<ResolvedProgram> {
    if is_executable_file(name) {
        classify_candidate(name)
    } else {
        None
    }
}

/// True when `name` ends in one of the extensions WebCodex can execute
/// directly (`.exe`/`.com` as native PE programs, `.cmd`/`.bat` as batch
/// scripts), compared case-insensitively.
#[cfg(windows)]
fn supported_extension(name: &Path) -> bool {
    matches!(
        name.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if matches!(extension.to_ascii_lowercase().as_str(), "exe" | "com" | "cmd" | "bat")
    )
}

/// Classify an existing file by its extension. `.cmd`/`.bat` (case-
/// insensitive) are batch scripts; `.exe`/`.com` and extensionless files are
/// native programs. Any other extension (`.vbs`, `.js`, ...) fails closed:
/// such files are not directly launchable PE programs and are never selected.
#[cfg(windows)]
fn classify_candidate(path: &Path) -> Option<ResolvedProgram> {
    if !is_executable_file(path) {
        return None;
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("cmd") | Some("bat") => Some(ResolvedProgram::Batch(path.to_path_buf())),
        Some("exe") | Some("com") | None => Some(ResolvedProgram::Native(path.to_path_buf())),
        Some(_) => None,
    }
}

/// The `PATHEXT` extension list, lowercased, without leading dots, in
/// declared order, filtered to the extensions WebCodex can actually execute:
/// `.com`, `.exe`, `.bat`, `.cmd`. Script-interpreter entries (`.vbs`,
/// `.js`, `.wsf`, ...) are not launchable native programs and are ignored
/// rather than misclassified. Missing, empty, or entirely-unsupported
/// `PATHEXT` falls back to the Windows default order (`com;exe;bat;cmd`).
///
/// `override_raw` is a test seam; production passes `None` and reads the
/// process environment.
#[cfg(windows)]
fn pathext_extensions(override_raw: Option<&str>) -> Vec<String> {
    const SUPPORTED: &[&str] = &["com", "exe", "bat", "cmd"];
    let raw = override_raw
        .map(str::to_owned)
        .unwrap_or_else(|| std::env::var("PATHEXT").unwrap_or_default());
    let mut extensions: Vec<String> = raw
        .split(';')
        .map(|entry| entry.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|entry| SUPPORTED.contains(&entry.as_str()))
        .collect();
    if extensions.is_empty() {
        extensions = SUPPORTED.iter().map(|entry| entry.to_string()).collect();
    }
    extensions
}

/// True when the file starts with the `MZ` DOS header, i.e. it is a PE
/// image that `CreateProcess` can execute directly. Extensionless POSIX shims
/// (npm-style) fail this check and are skipped during resolution.
#[cfg(windows)]
fn is_pe_image(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).is_ok() && magic == *b"MZ"
}

/// Return `true` if `haystack` contains any of `needles` as a substring.
///
/// Used by the error-classification helpers in [`crate::main`] (proxy/gateway
/// detection, connection-refused detection, TLS/auth failure detection) and
/// by the agent-transport error classifier in [`crate::webcodex_runner::transport`].
/// Both sites previously carried a byte-identical private copy of this one
/// liner; it has no behavioral coupling to either caller, so it lives here.
pub(crate) fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// True when `path` is a regular file that is executable.
///
/// On Unix this requires any execute bit (`& 0o111`); on other platforms any
/// regular file counts as executable (matching the platform's `Command`
/// semantics). The LSP supervisor (executable resolution + rustup-proxy
/// detection) and the validation executor (`resolve_executable`) previously
/// each carried a private copy that differed only in `path.metadata()` vs
/// `std::fs::metadata()` — `Path::metadata` is a thin wrapper over
/// `fs::metadata`, so the two were observationally identical.
pub(crate) fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn temp_dir_with(name: &str, contents: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(name);
        if !contents.is_empty() {
            std::fs::write(&file, contents).unwrap();
        }
        (temp, file)
    }

    fn pe_bytes() -> &'static [u8] {
        b"MZ\x90\x00fake"
    }

    #[test]
    fn resolves_pe_native_executable() {
        let (temp, executable) = temp_dir_with("foo.exe", pe_bytes());
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            resolve_program_in_path("foo", &path),
            Some(ResolvedProgram::Native(executable))
        );
    }

    #[test]
    fn resolves_batch_script_as_batch() {
        let (temp, script) = temp_dir_with("foo.cmd", b"@echo off\r\n");
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo", &path).unwrap();
        assert_eq!(resolved.path(), script);
        assert!(resolved.is_batch());
    }

    #[test]
    fn cmd_wins_over_extensionless_shim_in_same_directory() {
        let temp = tempfile::tempdir().unwrap();
        // npm-style extensionless POSIX shim next to the real .cmd shim.
        std::fs::write(temp.path().join("foo"), b"#!/bin/sh\nexec foo.cmd \"$@\"\n").unwrap();
        std::fs::write(temp.path().join("foo.cmd"), b"@echo off\r\n").unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo", &path).unwrap();
        assert_eq!(
            resolved.path(),
            temp.path().join("foo.cmd"),
            "the .cmd shim must win over the extensionless shim"
        );
        assert!(resolved.is_batch());
    }

    #[test]
    fn extensionless_shim_alone_is_never_selected() {
        let (temp, shim) = temp_dir_with("foo", b"#!/bin/sh\nexit 0\n");
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            resolve_program_in_path("foo", &path),
            None,
            "a non-PE extensionless file must not resolve (it would fail with os error 193)"
        );
        let _ = shim;
    }

    #[test]
    fn extensionless_pe_file_is_accepted() {
        let (temp, program) = temp_dir_with("tool", pe_bytes());
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            resolve_program_in_path("tool", &path),
            Some(ResolvedProgram::Native(program))
        );
    }

    #[test]
    fn pathext_case_variations_are_insensitive() {
        let (temp, _script) = temp_dir_with("foo.CMD", b"@echo off\r\n");
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo", &path).unwrap();
        // The candidate name is built from the lowercased PATHEXT entry; on a
        // case-insensitive filesystem it resolves to the same `foo.CMD` file.
        assert_eq!(
            resolved.path().to_string_lossy().to_lowercase(),
            temp.path().join("foo.cmd").to_string_lossy().to_lowercase()
        );
        assert!(resolved.is_batch());
    }

    #[test]
    fn com_executable_is_native() {
        let (temp, program) = temp_dir_with("foo.com", pe_bytes());
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            resolve_program_in_path("foo", &path),
            Some(ResolvedProgram::Native(program))
        );
    }

    #[test]
    fn path_entries_with_spaces_work() {
        let temp = tempfile::tempdir().unwrap();
        let spaced = temp.path().join("dir with spaces");
        std::fs::create_dir(&spaced).unwrap();
        let executable = spaced.join("foo.exe");
        std::fs::write(&executable, pe_bytes()).unwrap();
        let path = std::env::join_paths([&spaced]).unwrap();
        assert_eq!(
            resolve_program_in_path("foo", &path),
            Some(ResolvedProgram::Native(executable))
        );
    }

    #[test]
    fn missing_program_resolves_to_none() {
        let temp = tempfile::tempdir().unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(resolve_program_in_path("no-such-tool", &path), None);
    }

    #[test]
    fn absolute_path_with_spaces_resolves_directly() {
        let temp = tempfile::tempdir().unwrap();
        let spaced = temp.path().join("tool dir");
        std::fs::create_dir(&spaced).unwrap();
        let executable = spaced.join("probe.exe");
        std::fs::write(&executable, pe_bytes()).unwrap();
        assert_eq!(
            resolve_program_in_path(&executable.to_string_lossy(), &OsStr::new("")),
            Some(ResolvedProgram::Native(executable))
        );
    }

    #[test]
    fn absolute_batch_path_resolves_as_batch() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("run.cmd");
        std::fs::write(&script, b"@echo off\r\n").unwrap();
        assert_eq!(
            resolve_program_in_path(&script.to_string_lossy(), &OsStr::new("")),
            Some(ResolvedProgram::Batch(script))
        );
    }

    #[test]
    fn executable_lookup_matches_windows_exe_suffix_resolution() {
        let (temp, executable) = temp_dir_with("webcodex-path-probe.exe", pe_bytes());
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            find_executable_in_path("webcodex-path-probe", &path),
            Some(executable)
        );
    }

    #[test]
    fn explicit_cmd_bare_name_matches_exactly_without_pathext_appending() {
        let temp = tempfile::tempdir().unwrap();
        // A `foo.cmd.exe` sibling must not shadow the exact `foo.cmd`.
        std::fs::write(temp.path().join("foo.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(temp.path().join("foo.cmd.exe"), pe_bytes()).unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo.cmd", &path).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
        assert!(resolved.is_batch());
    }

    #[test]
    fn explicit_bat_bare_name_matches_exactly() {
        let (temp, script) = temp_dir_with("foo.bat", b"@echo off\r\n");
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo.bat", &path).unwrap();
        assert_eq!(resolved.path(), script);
        assert!(resolved.is_batch());
    }

    #[test]
    fn explicit_exe_bare_name_matches_exactly() {
        let (temp, executable) = temp_dir_with("foo.exe", pe_bytes());
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(
            resolve_program_in_path("foo.exe", &path),
            Some(ResolvedProgram::Native(executable))
        );
    }

    #[test]
    fn explicit_uppercase_extension_matches_case_insensitively() {
        let (temp, script) = temp_dir_with("foo.CMD", b"@echo off\r\n");
        let path = std::env::join_paths([temp.path()]).unwrap();
        let resolved = resolve_program_in_path("foo.CMD", &path).unwrap();
        assert_eq!(resolved.path(), script);
        assert!(resolved.is_batch());
    }

    #[test]
    fn explicit_supported_extension_never_appends_pathext() {
        let temp = tempfile::tempdir().unwrap();
        // Only `foo.cmd.cmd` and `foo.cmd.exe` exist, never the exact name:
        // explicit-extension resolution must not fall back to PATHEXT
        // appending.
        std::fs::write(temp.path().join("foo.cmd.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(temp.path().join("foo.cmd.exe"), pe_bytes()).unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(resolve_program_in_path("foo.cmd", &path), None);
    }

    #[test]
    fn unknown_extension_bare_name_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        // A real file with a non-executable extension must never resolve,
        // even though it exists and is a valid PE image.
        std::fs::write(temp.path().join("foo.vbs"), pe_bytes()).unwrap();
        std::fs::write(temp.path().join("foo.xyz"), pe_bytes()).unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();
        assert_eq!(resolve_program_in_path("foo.vbs", &path), None);
        assert_eq!(resolve_program_in_path("foo.xyz", &path), None);
    }

    #[test]
    fn pathext_ordering_decides_between_cmd_and_exe() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("foo.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(temp.path().join("foo.exe"), pe_bytes()).unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();

        // bat;cmd → the .cmd shim wins.
        let resolved =
            resolve_program_in_path_with_pathext("foo", &path, Some(".BAT;.CMD")).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
        assert!(resolved.is_batch());

        // exe;cmd → the native program wins.
        let resolved =
            resolve_program_in_path_with_pathext("foo", &path, Some(".EXE;.CMD")).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.exe"));
        assert!(!resolved.is_batch());
    }

    #[test]
    fn unsupported_pathext_entries_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("foo.cmd"), b"@echo off\r\n").unwrap();
        std::fs::write(temp.path().join("foo.vbs"), pe_bytes()).unwrap();
        std::fs::write(temp.path().join("foo.js"), pe_bytes()).unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();

        // .vbs/.js appear before .cmd in PATHEXT but must never be selected.
        let resolved =
            resolve_program_in_path_with_pathext("foo", &path, Some(".VBS;.JS;.CMD")).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
        assert!(resolved.is_batch());

        // A PATHEXT of only unsupported entries falls back to the default
        // order, which still finds the .cmd shim.
        let resolved =
            resolve_program_in_path_with_pathext("foo", &path, Some(".VBS;.JS;.WSF")).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
    }

    #[test]
    fn npm_style_shim_regression_bare_and_explicit_names() {
        let temp = tempfile::tempdir().unwrap();
        // npm-style extensionless POSIX shim next to the real .cmd shim.
        std::fs::write(temp.path().join("foo"), b"#!/bin/sh\nexec foo.cmd \"$@\"\n").unwrap();
        std::fs::write(temp.path().join("foo.cmd"), b"@echo off\r\n").unwrap();
        let path = std::env::join_paths([temp.path()]).unwrap();

        // Bare `foo`: the .cmd shim wins over the extensionless shim.
        let resolved = resolve_program_in_path("foo", &path).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
        assert!(resolved.is_batch());

        // Explicit `foo.cmd`: exact match, still the .cmd shim.
        let resolved = resolve_program_in_path("foo.cmd", &path).unwrap();
        assert_eq!(resolved.path(), temp.path().join("foo.cmd"));
        assert!(resolved.is_batch());
    }

    #[test]
    fn absolute_cmd_still_resolves_directly() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("tools dir").join("run.cmd");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, b"@echo off\r\n").unwrap();
        assert_eq!(
            resolve_program_in_path(&script.to_string_lossy(), &OsStr::new("")),
            Some(ResolvedProgram::Batch(script))
        );
    }
}
