use super::*;
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_IDENTITY_BYTES: usize = 32 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_PATH_BYTES: usize = 8 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_BUNDLE_IDENTIFIER_BYTES: usize = 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_BUNDLE_METADATA_BYTES: usize = 4 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_SCAN_DEPTH: usize = 2;
#[cfg(target_os = "macos")]
const MACOS_APPLICATION_LAUNCH_WAIT: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MacApplicationFileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MacApplicationIdentity {
    canonical_path: String,
    bundle_identifier: String,
    bundle_display_name: String,
    bundle_executable: String,
    bundle: MacApplicationFileIdentity,
    info_plist: MacApplicationFileIdentity,
    executable: MacApplicationFileIdentity,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacApplicationLaunchCompletion {
    Success,
    Failed,
    Ambiguous,
}

#[cfg(target_os = "macos")]
fn mac_application_file_identity(metadata: &fs::Metadata) -> MacApplicationFileIdentity {
    MacApplicationFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        mode: metadata.mode(),
        links: metadata.nlink(),
        size: metadata.size(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
    }
}

#[cfg(target_os = "macos")]
fn bounded_ns_string(value: &NSString, max_bytes: usize) -> Option<String> {
    if value.is_empty() || value.len() > max_bytes {
        return None;
    }
    let value = value.to_string();
    (!value.contains('\0')).then_some(value)
}

#[cfg(target_os = "macos")]
fn bounded_bundle_string(bundle: &NSBundle, key: &str, max_bytes: usize) -> Option<String> {
    let key = NSString::from_str(key);
    let value = bundle.objectForInfoDictionaryKey(&key)?;
    let value = value.downcast::<NSString>().ok()?;
    bounded_ns_string(&value, max_bytes)
}

#[cfg(target_os = "macos")]
fn path_from_file_url(url: &NSURL) -> Option<PathBuf> {
    let path = url.path()?;
    if path.is_empty() || path.len() > MAX_MACOS_APPLICATION_PATH_BYTES {
        return None;
    }
    let path = path.to_string();
    (!path.contains('\0')).then(|| PathBuf::from(path))
}

#[cfg(target_os = "macos")]
fn normalize_macos_application_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots = roots
        .into_iter()
        .filter_map(|root| {
            let canonical = fs::canonicalize(root).ok()?;
            let metadata = fs::symlink_metadata(&canonical).ok()?;
            metadata.is_dir().then_some(canonical)
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut non_overlapping = Vec::<PathBuf>::with_capacity(roots.len());
    for root in roots {
        if non_overlapping
            .iter()
            .any(|parent| root.starts_with(parent))
        {
            continue;
        }
        non_overlapping.push(root);
    }
    non_overlapping.sort();
    non_overlapping
}

#[cfg(target_os = "macos")]
fn macos_application_roots() -> Vec<PathBuf> {
    let manager = NSFileManager::defaultManager();
    let domains = NSSearchPathDomainMask::UserDomainMask
        | NSSearchPathDomainMask::LocalDomainMask
        | NSSearchPathDomainMask::SystemDomainMask;
    let mut roots = Vec::new();
    for directory in [
        NSSearchPathDirectory::ApplicationDirectory,
        NSSearchPathDirectory::AdminApplicationDirectory,
    ] {
        let urls = manager.URLsForDirectory_inDomains(directory, domains);
        roots.extend(urls.iter().filter_map(|url| path_from_file_url(&url)));
    }
    normalize_macos_application_roots(roots)
}

#[cfg(target_os = "macos")]
fn path_is_symlink_free_under_root(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    true
}

#[cfg(target_os = "macos")]
fn mac_application_candidate_at_path(
    path: &Path,
    allowed_roots: &[PathBuf],
) -> Option<PlatformApplication> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let root = allowed_roots
        .iter()
        .find(|root| path.starts_with(root) && path_is_symlink_free_under_root(path, root))?;
    let canonical_path = fs::canonicalize(path).ok()?;
    if canonical_path != path || !canonical_path.starts_with(root) {
        return None;
    }
    let bundle_metadata = fs::symlink_metadata(&canonical_path).ok()?;
    if !bundle_metadata.is_dir() || bundle_metadata.file_type().is_symlink() {
        return None;
    }

    let canonical_path_text = canonical_path.to_str()?;
    if canonical_path_text.is_empty()
        || canonical_path_text.len() > MAX_MACOS_APPLICATION_PATH_BYTES
        || canonical_path_text.contains('\0')
    {
        return None;
    }
    let native_path = NSString::from_str(canonical_path_text);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_path, true);
    let bundle = NSBundle::bundleWithURL(&application_url)?;
    let bundle_url = path_from_file_url(&bundle.bundleURL())?;
    if fs::canonicalize(bundle_url).ok()? != canonical_path {
        return None;
    }

    let package_type = bounded_bundle_string(
        &bundle,
        "CFBundlePackageType",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )?;
    if package_type != "APPL" {
        return None;
    }
    let native_bundle_identifier = bundle.bundleIdentifier()?;
    let bundle_identifier =
        bounded_ns_string(&native_bundle_identifier, MAX_MACOS_BUNDLE_IDENTIFIER_BYTES)?;
    let display_name = bounded_bundle_string(
        &bundle,
        "CFBundleDisplayName",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )
    .or_else(|| bounded_bundle_string(&bundle, "CFBundleName", MAX_MACOS_BUNDLE_METADATA_BYTES))?;
    let bundle_executable = bounded_bundle_string(
        &bundle,
        "CFBundleExecutable",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )?;

    let executable_url = bundle.executableURL()?;
    let executable_path = path_from_file_url(&executable_url)?;
    let expected_executable = canonical_path
        .join("Contents/MacOS")
        .join(&bundle_executable);
    if !path_is_symlink_free_under_root(&expected_executable, &canonical_path) {
        return None;
    }
    let canonical_executable = fs::canonicalize(&executable_path).ok()?;
    if canonical_executable != expected_executable
        || !canonical_executable.starts_with(&canonical_path)
        || canonical_executable.file_name()? != std::ffi::OsStr::new(&bundle_executable)
    {
        return None;
    }
    let executable_metadata = fs::symlink_metadata(&canonical_executable).ok()?;
    if !executable_metadata.is_file()
        || executable_metadata.file_type().is_symlink()
        || executable_metadata.mode() & 0o111 == 0
    {
        return None;
    }

    let info_plist_path = canonical_path.join("Contents/Info.plist");
    if !path_is_symlink_free_under_root(&info_plist_path, &canonical_path) {
        return None;
    }
    let canonical_info_plist = fs::canonicalize(&info_plist_path).ok()?;
    if canonical_info_plist != info_plist_path || !canonical_info_plist.starts_with(&canonical_path)
    {
        return None;
    }
    let info_plist_metadata = fs::symlink_metadata(&canonical_info_plist).ok()?;
    if !info_plist_metadata.is_file() || info_plist_metadata.file_type().is_symlink() {
        return None;
    }

    let identity = MacApplicationIdentity {
        canonical_path: canonical_path_text.to_string(),
        bundle_identifier,
        bundle_display_name: display_name.clone(),
        bundle_executable,
        bundle: mac_application_file_identity(&bundle_metadata),
        info_plist: mac_application_file_identity(&info_plist_metadata),
        executable: mac_application_file_identity(&executable_metadata),
    };
    let native_identity = serde_json::to_vec(&identity).ok()?;
    if native_identity.is_empty() || native_identity.len() > MAX_MACOS_APPLICATION_IDENTITY_BYTES {
        return None;
    }
    Some(PlatformApplication {
        display_name,
        native_identity,
    })
}

#[cfg(target_os = "macos")]
fn enumerate_macos_applications(roots: &[PathBuf], scan_limit: usize) -> Vec<PlatformApplication> {
    let roots = normalize_macos_application_roots(roots.iter().cloned());
    let mut queue = roots
        .iter()
        .cloned()
        .map(|root| (root, 0usize))
        .collect::<VecDeque<_>>();
    let mut scanned = 0usize;
    let mut applications = Vec::new();

    'scan: while let Some((directory, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            if scanned >= scan_limit {
                break 'scan;
            }
            scanned += 1;
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let is_app_bundle = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"));
            if is_app_bundle {
                // A .app bundle candidate is always a leaf, including when its
                // metadata cannot prove that it is safely launchable.
                if let Some(application) = mac_application_candidate_at_path(&path, &roots) {
                    applications.push(application);
                }
            } else if depth < MAX_MACOS_APPLICATION_SCAN_DEPTH {
                queue.push_back((path, depth + 1));
            }
        }
    }
    crate::sort_application_candidates(&mut applications);
    applications
}

#[cfg(target_os = "macos")]
pub(crate) fn list_applications(limit: usize) -> Result<Vec<PlatformApplication>, String> {
    if limit == 0 || limit > crate::MAX_APPLICATION_SCAN {
        return Err("invalid_request: macOS application scan bound is invalid".to_string());
    }
    Ok(enumerate_macos_applications(
        &macos_application_roots(),
        limit,
    ))
}

#[cfg(target_os = "macos")]
fn decode_macos_application_identity(
    native_identity: &[u8],
) -> Result<MacApplicationIdentity, String> {
    if native_identity.is_empty() || native_identity.len() > MAX_MACOS_APPLICATION_IDENTITY_BYTES {
        return Err("stale_application: macOS application identity is invalid".to_string());
    }
    let identity: MacApplicationIdentity = serde_json::from_slice(native_identity)
        .map_err(|_| "stale_application: macOS application identity is invalid".to_string())?;
    if identity.canonical_path.is_empty()
        || identity.canonical_path.len() > MAX_MACOS_APPLICATION_PATH_BYTES
        || identity.canonical_path.contains('\0')
    {
        return Err("stale_application: macOS application identity is invalid".to_string());
    }
    Ok(identity)
}

#[cfg(target_os = "macos")]
fn revalidate_macos_application_identity_with_roots(
    native_identity: &[u8],
    roots: &[PathBuf],
) -> Result<String, String> {
    let identity = decode_macos_application_identity(native_identity)?;
    let roots = normalize_macos_application_roots(roots.iter().cloned());
    let path = PathBuf::from(&identity.canonical_path);
    let fresh = mac_application_candidate_at_path(&path, &roots)
        .ok_or_else(|| "stale_application: macOS application changed or disappeared".to_string())?;
    if fresh.native_identity != native_identity {
        return Err("stale_application: macOS application identity changed".to_string());
    }
    Ok(identity.canonical_path)
}

#[cfg(target_os = "macos")]
fn configure_macos_application_launch(configuration: &NSWorkspaceOpenConfiguration) {
    configuration.setPromptsUserIfNeeded(false);
    configuration.setAddsToRecentItems(false);
    configuration.setActivates(false);
    configuration.setHides(false);
    configuration.setHidesOthers(false);
    configuration.setForPrinting(false);
    configuration.setCreatesNewApplicationInstance(false);
    configuration.setAllowsRunningApplicationSubstitution(false);
    configuration.setRequiresUniversalLinks(false);
    configuration.setAppleEvent(None);
    let arguments = NSArray::<NSString>::from_slice(&[]);
    let environment = NSDictionary::<NSString, NSString>::from_slices::<NSString>(&[], &[]);
    configuration.setArguments(&arguments);
    configuration.setEnvironment(&environment);
}

#[cfg(target_os = "macos")]
fn classify_macos_application_launch_completion(
    has_application: bool,
    has_error: bool,
) -> MacApplicationLaunchCompletion {
    match (has_application, has_error) {
        (true, false) => MacApplicationLaunchCompletion::Success,
        (false, true) => MacApplicationLaunchCompletion::Failed,
        _ => MacApplicationLaunchCompletion::Ambiguous,
    }
}

#[cfg(target_os = "macos")]
fn macos_application_launch_completion_channel() -> (
    Receiver<MacApplicationLaunchCompletion>,
    RcBlock<dyn Fn(*mut NSRunningApplication, *mut NSError)>,
) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let completion: RcBlock<dyn Fn(*mut NSRunningApplication, *mut NSError)> = RcBlock::new(
        move |running_application: *mut NSRunningApplication, error: *mut NSError| {
            let completion = classify_macos_application_launch_completion(
                !running_application.is_null(),
                !error.is_null(),
            );
            let _ = sender.try_send(completion);
        },
    );
    (receiver, completion)
}

#[cfg(target_os = "macos")]
fn dispatch_revalidated_macos_application<T>(
    native_identity: &[u8],
    roots: &[PathBuf],
    prepared_path: &str,
    dispatch: impl FnOnce() -> T,
) -> Result<T, String> {
    let application_path =
        revalidate_macos_application_identity_with_roots(native_identity, roots)?;
    if application_path != prepared_path {
        return Err("stale_application: macOS application launch path changed".to_string());
    }
    Ok(dispatch())
}

#[cfg(target_os = "macos")]
fn wait_for_macos_application_launch(
    completion: Receiver<MacApplicationLaunchCompletion>,
    wait: Duration,
) -> Result<(), String> {
    match completion.recv_timeout(wait) {
        Ok(MacApplicationLaunchCompletion::Success) => Ok(()),
        Ok(MacApplicationLaunchCompletion::Failed) => Err(
            "outcome_unknown: macOS application launch reported failure after native dispatch"
                .to_string(),
        ),
        Ok(MacApplicationLaunchCompletion::Ambiguous) => Err(
            "outcome_unknown: macOS application launch returned ambiguous completion metadata"
                .to_string(),
        ),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(
            "outcome_unknown: macOS application launch completion deadline exceeded after native dispatch"
                .to_string(),
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(
            "outcome_unknown: macOS application launch completion was lost after native dispatch"
                .to_string(),
        ),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_application(
    application_id: &str,
    application: &ApplicationRecord,
) -> Result<Value, String> {
    // Prepare every launch object that does not depend on the current target identity
    // before the final filesystem/bundle proof. This keeps same-path replacement
    // failures definite pre-effect instead of widening the gap before NSWorkspace dispatch.
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    let workspace = NSWorkspace::sharedWorkspace();
    let (receiver, completion) = macos_application_launch_completion_channel();

    // The stored path may be used to prepare the immutable NSURL, but it does not
    // authorize launch. Full bundle/filesystem identity is revalidated again after
    // this target-dependent object preparation and immediately before native dispatch.
    let prepared_identity = decode_macos_application_identity(&application.native_identity)?;
    let native_application_path = NSString::from_str(&prepared_identity.canonical_path);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_application_path, true);
    dispatch_revalidated_macos_application(
        &application.native_identity,
        &macos_application_roots(),
        &prepared_identity.canonical_path,
        || {
            // This asynchronous call is the native effect boundary. It is submitted
            // exactly once; every timeout, callback loss, or non-success completion is
            // unknown and never causes a second launch request.
            workspace.openApplicationAtURL_configuration_completionHandler(
                &application_url,
                &configuration,
                Some(&completion),
            );
        },
    )?;
    wait_for_macos_application_launch(receiver, MACOS_APPLICATION_LAUNCH_WAIT)?;
    Ok(json!({
        "platform": "macos",
        "application_id": application_id,
        "success": true,
    }))
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn application_identity_revalidates_for_test(
    native_identity: &[u8],
) -> Result<(), String> {
    revalidate_macos_application_identity_with_roots(native_identity, &macos_application_roots())
        .map(drop)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_applications_in_roots_for_test(
    roots: &[PathBuf],
    scan_limit: usize,
) -> Vec<PlatformApplication> {
    enumerate_macos_applications(roots, scan_limit)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_identity_revalidates_in_roots_for_test(
    native_identity: &[u8],
    roots: &[PathBuf],
) -> Result<(), String> {
    revalidate_macos_application_identity_with_roots(native_identity, roots).map(drop)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_preparation_race_for_test(
    native_identity: &[u8],
    roots: &[PathBuf],
    between_preparation_and_dispatch: impl FnOnce(),
) -> (Result<(), String>, usize) {
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    let workspace = NSWorkspace::sharedWorkspace();
    let (receiver, completion) = macos_application_launch_completion_channel();
    let prepared_identity = match decode_macos_application_identity(native_identity) {
        Ok(identity) => identity,
        Err(error) => return (Err(error), 0),
    };
    let native_application_path = NSString::from_str(&prepared_identity.canonical_path);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_application_path, true);

    between_preparation_and_dispatch();

    let mut dispatch_attempts = 0usize;
    let result = dispatch_revalidated_macos_application(
        native_identity,
        roots,
        &prepared_identity.canonical_path,
        || {
            dispatch_attempts += 1;
            let _prepared_objects = (
                &configuration,
                &workspace,
                &receiver,
                &completion,
                &application_url,
            );
        },
    );
    (result, dispatch_attempts)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_configuration_for_test() -> bool {
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    !configuration.promptsUserIfNeeded()
        && !configuration.addsToRecentItems()
        && !configuration.activates()
        && !configuration.hides()
        && !configuration.hidesOthers()
        && !configuration.isForPrinting()
        && !configuration.createsNewApplicationInstance()
        && !configuration.allowsRunningApplicationSubstitution()
        && !configuration.requiresUniversalLinks()
        && configuration.appleEvent().is_none()
        && configuration.arguments().len() == 0
        && configuration.environment().len() == 0
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_completion_for_test(
    has_application: bool,
    has_error: bool,
) -> &'static str {
    match classify_macos_application_launch_completion(has_application, has_error) {
        MacApplicationLaunchCompletion::Success => "success",
        MacApplicationLaunchCompletion::Failed => "outcome_unknown",
        MacApplicationLaunchCompletion::Ambiguous => "outcome_unknown",
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_lost_completion_for_test() -> (bool, bool) {
    let (disconnected_sender, disconnected_receiver) = mpsc::sync_channel(1);
    drop(disconnected_sender);
    let disconnected = wait_for_macos_application_launch(disconnected_receiver, Duration::ZERO)
        .is_err_and(|error| error.starts_with("outcome_unknown:"));

    let (_pending_sender, pending_receiver) = mpsc::sync_channel(1);
    let timed_out = wait_for_macos_application_launch(pending_receiver, Duration::ZERO)
        .is_err_and(|error| error.starts_with("outcome_unknown:"));
    (disconnected, timed_out)
}
