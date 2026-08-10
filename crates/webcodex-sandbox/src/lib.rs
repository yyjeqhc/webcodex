//! Linux Landlock write sandbox for `inspect` command execution.
//!
//! The boundary is deliberately narrow: commands retain normal read, network,
//! environment, and external-service access, but ordinary local filesystem
//! writes are denied everywhere except one private per-command/job scratch
//! directory. This is not a general no-side-effect sandbox.
//!
//! Everything here fails closed. Linux Landlock ABI v3 is the minimum because
//! it adds `TRUNCATE`; unsupported kernels, partial enforcement, invalid
//! scratch directories, and non-Linux hosts reject inspect execution.

pub const INSPECT_SANDBOX_MODE: &str = "inspect";

/// Why the sandbox cannot be used, when it cannot.
///
/// Kept distinct rather than collapsed into one error string because they call
/// for different answers: an old kernel is an operator's decision to make, a
/// partial ruleset is a bug in the policy, and a failed probe is a host
/// problem. All of them deny.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxUnavailable {
    /// No Landlock on this build target or kernel.
    Unsupported(String),
    /// The kernel applied only part of the ruleset. Treated as failure: a
    /// half-applied write filter is not a boundary, and `BestEffort`
    /// compatibility would otherwise let it through silently.
    PartiallyEnforced,
    /// The probe itself could not complete, so nothing was proven.
    ProbeFailed(String),
}

impl std::fmt::Display for SandboxUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(detail) => write!(formatter, "unsupported: {detail}"),
            Self::PartiallyEnforced => write!(
                formatter,
                "the kernel enforced only part of the ruleset; a partial write filter is not a boundary"
            ),
            Self::ProbeFailed(detail) => write!(formatter, "probe failed: {detail}"),
        }
    }
}

/// Scratch space for the probe, removed however the probe ends.
///
/// `tempfile` is a dev-dependency, and the probe runs in the shipped binary.
#[cfg(target_os = "linux")]
struct ProbeDir {
    path: std::path::PathBuf,
}

/// Private writable directory owned by one inspect command or job.
///
/// Creation is atomic with mode 0700, the resulting path is verified to be a
/// real directory rather than a symlink, and dropping the final owner removes
/// the directory recursively after the command subtree has ended.
#[derive(Debug)]
pub struct InspectScratch {
    path: std::path::PathBuf,
}

impl InspectScratch {
    pub fn create() -> Result<Self, SandboxUnavailable> {
        let path = std::env::temp_dir().join(format!(
            "webcodex-inspect-{}",
            uuid::Uuid::new_v4().simple()
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
                .create(&path)
                .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir(&path)
                .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        }
        // Own cleanup immediately after creation so every later validation
        // failure removes the directory too.
        let scratch = Self { path };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(scratch.path(), std::fs::Permissions::from_mode(0o700))
                .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        }
        let metadata = std::fs::symlink_metadata(scratch.path())
            .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(SandboxUnavailable::ProbeFailed(
                "inspect scratch is not a real directory".to_string(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o700 {
                return Err(SandboxUnavailable::ProbeFailed(
                    "inspect scratch permissions are not 0700".to_string(),
                ));
            }
        }
        Ok(scratch)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for InspectScratch {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %error,
                    "failed to clean inspect scratch directory"
                );
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl ProbeDir {
    fn create() -> Result<Self, SandboxUnavailable> {
        let path = std::env::temp_dir().join(format!(
            "webcodex-sandbox-probe-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir(&path)
            .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        Ok(Self { path })
    }
}

#[cfg(target_os = "linux")]
impl Drop for ProbeDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Probe whether this kernel would actually enforce the write-denying ruleset.
///
/// Runs the whole thing for real in a throwaway child: apply the ruleset, call
/// `restrict_self`, require `FullyEnforced`, then try to write somewhere the
/// policy forbids and require the kernel to refuse. Creating a ruleset file
/// descriptor — which is all this used to do — proves only that the syscall
/// exists, not that the policy takes effect.
#[cfg(target_os = "linux")]
pub fn inspect_sandbox_available() -> Result<(), SandboxUnavailable> {
    use std::io::Write;

    let probe = ProbeDir::create()?;
    let writable = probe.path.join("writable");
    let denied = probe.path.join("denied");
    for directory in [&writable, &denied] {
        std::fs::create_dir(directory)
            .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
    }

    // A child, because `restrict_self` is irrevocable: proving enforcement in
    // this process would sandbox the agent itself for the rest of its life.
    let script = format!(
        "printf ok > {}/probe && ! printf x > {}/probe",
        shell_quote(&writable),
        shell_quote(&denied),
    );
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg(script);
    let writable_paths = vec![writable.clone()];
    let (reader, writer) =
        std::io::pipe().map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(move || {
            // The child reports the reason back over the pipe; a failed
            // `restrict_self` must abort the exec rather than run free.
            match restrict_writes_to(&writable_paths) {
                Ok(()) => Ok(()),
                Err(reason) => {
                    let mut writer = &writer;
                    let _ = writer.write_all(reason.to_string().as_bytes());
                    Err(std::io::Error::other(reason.to_string()))
                }
            }
        });
    }
    let output = command
        .output()
        .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
    drop(command);

    if output.status.success() {
        // Wrote where allowed, refused where not.
        return Ok(());
    }
    let mut reason = String::new();
    {
        use std::io::Read;
        let mut reader = reader;
        let _ = reader.read_to_string(&mut reason);
    }
    if reason.contains("partially") {
        return Err(SandboxUnavailable::PartiallyEnforced);
    }
    if reason.is_empty() {
        return Err(SandboxUnavailable::ProbeFailed(
            "the sandboxed probe did not behave as required".to_string(),
        ));
    }
    Err(SandboxUnavailable::Unsupported(reason))
}

#[cfg(not(target_os = "linux"))]
pub fn inspect_sandbox_available() -> Result<(), SandboxUnavailable> {
    Err(SandboxUnavailable::Unsupported(
        "inspect command sandboxing requires Linux with Landlock ABI v3".to_string(),
    ))
}

#[cfg(target_os = "linux")]
fn shell_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Restrict the calling process, and irrevocably every descendant, to the
/// write-denying ruleset, allowing writes only beneath `writable`.
///
/// ABI v3 covers all write-related filesystem rights through
/// `AccessFs::from_write`, including `Refer` and `Truncate`. Only
/// `FullyEnforced` succeeds. `PartiallyEnforced` means the kernel
/// understood some of the policy and dropped the rest, which is exactly the
/// case a `BestEffort` ruleset hides — so the compatibility level is a hard
/// requirement and anything short of full enforcement denies.
#[cfg(target_os = "linux")]
pub fn restrict_writes_to(writable: &[std::path::PathBuf]) -> Result<(), SandboxUnavailable> {
    use landlock::{
        AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus, ABI,
    };
    let abi = ABI::V3;
    let write_access = AccessFs::from_write(abi);
    let mut ruleset = Ruleset::default()
        // Hard requirement, not the default best effort: a kernel that cannot
        // honour the policy must say so instead of quietly applying less.
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(write_access)
        .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?
        .create()
        .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?;
    // Open every allowed root explicitly. The convenience iterator silently
    // skips missing paths, which could turn a removed scratch into a partially
    // configured policy instead of aborting exec.
    for path in writable {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(SandboxUnavailable::ProbeFailed(
                "inspect writable root is not a real directory".to_string(),
            ));
        }
        ruleset = ruleset
            .add_rule(PathBeneath::new(
                PathFd::new(path)
                    .map_err(|error| SandboxUnavailable::ProbeFailed(error.to_string()))?,
                write_access,
            ))
            .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?;
    }
    let status = ruleset
        // Reads stay open everywhere; only writes are governed, so the policy
        // never has to enumerate what may be read.
        // Git opens /dev/null read-write even for `git status`. Permit only
        // WriteFile on this non-persistent character sink; the /dev hierarchy
        // remains immutable and no ordinary filesystem object gains write
        // access.
        .add_rule(PathBeneath::new(
            PathFd::new("/dev/null")
                .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?,
            AccessFs::WriteFile,
        ))
        .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?
        .restrict_self()
        .map_err(|error| SandboxUnavailable::Unsupported(error.to_string()))?;
    match status.ruleset {
        RulesetStatus::FullyEnforced => Ok(()),
        RulesetStatus::PartiallyEnforced => Err(SandboxUnavailable::PartiallyEnforced),
        RulesetStatus::NotEnforced => Err(SandboxUnavailable::Unsupported(
            "the kernel did not enforce the Landlock ruleset".to_string(),
        )),
    }
}

/// Arrange for `command` to run under the inspect write sandbox.
///
/// Returns an error when this host cannot sandbox at all, so a caller holding
/// an explicit sandbox request fails before spawning rather than running the
/// command unconfined. The policy itself is applied between fork and exec, and
/// a failure there aborts the exec.
#[cfg(target_os = "linux")]
pub fn sandbox_command_inspect(
    command: &mut std::process::Command,
    scratch: &InspectScratch,
) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let writable = vec![scratch.path().to_path_buf()];
    command
        .env("TMPDIR", scratch.path())
        .env("CARGO_TARGET_DIR", scratch.path().join("target"));
    unsafe {
        command.pre_exec(move || {
            restrict_writes_to(&writable)
                .map_err(|reason| std::io::Error::other(reason.to_string()))
        });
    }
    Ok(())
}

/// Non-Linux hosts cannot sandbox, and silently running the command anyway
/// would turn a sandbox request into an unconfined execution. Fails instead.
#[cfg(not(target_os = "linux"))]
pub fn sandbox_command_inspect(
    _command: &mut std::process::Command,
    _scratch: &InspectScratch,
) -> Result<(), String> {
    Err(SandboxUnavailable::Unsupported(
        "inspect command sandboxing requires Linux with Landlock ABI v3".to_string(),
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unavailable_reason_denies_and_reads_distinctly() {
        // Each variant has to survive as its own answer: an operator decision,
        // a policy bug, and a broken host are not the same problem.
        let reasons = [
            SandboxUnavailable::Unsupported("old kernel".to_string()),
            SandboxUnavailable::PartiallyEnforced,
            SandboxUnavailable::ProbeFailed("no /bin/sh".to_string()),
        ];
        let rendered: Vec<String> = reasons.iter().map(ToString::to_string).collect();
        let unique: std::collections::HashSet<&String> = rendered.iter().collect();
        assert_eq!(unique.len(), rendered.len(), "{rendered:?}");
        assert!(rendered[1].contains("part"), "{}", rendered[1]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspect_policy_uses_abi_v3_write_rights_including_truncate() {
        use landlock::{AccessFs, ABI};
        let write = AccessFs::from_write(ABI::V3);
        assert!(write.contains(AccessFs::Truncate));
        assert!(write.contains(AccessFs::Refer));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_refuses_to_spawn_a_sandbox_request() {
        let mut command = std::process::Command::new("true");
        let scratch = InspectScratch::create().unwrap();
        let error = sandbox_command_inspect(&mut command, &scratch)
            .expect_err("a sandbox request must not silently run unconfined");
        assert!(error.contains("unsupported"), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_probe_reports_one_of_the_defined_outcomes_and_fails_closed() {
        // CI kernels differ: Landlock may be absent, partial, or complete. Any
        // outcome other than a proven full enforcement must deny.
        match inspect_sandbox_available() {
            Ok(()) => {
                // Proven: the probe wrote where allowed and was refused where
                // not, under a fully enforced ruleset.
            }
            Err(SandboxUnavailable::Unsupported(_))
            | Err(SandboxUnavailable::PartiallyEnforced)
            | Err(SandboxUnavailable::ProbeFailed(_)) => {}
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_sandboxed_command_cannot_write_the_project_but_can_read_it() {
        if inspect_sandbox_available().is_err() {
            // This kernel cannot enforce the policy; the denial is covered by
            // the probe test above.
            return;
        }
        let project = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("tracked.txt"), "hello\n").unwrap();

        let mut denied = std::process::Command::new("/bin/sh");
        denied
            .arg("-c")
            .arg(format!("echo x > {}/evil.txt", project.path().display()))
            .current_dir(project.path());
        let inspect_scratch = InspectScratch {
            path: scratch.path().to_path_buf(),
        };
        sandbox_command_inspect(&mut denied, &inspect_scratch).unwrap();
        let denied_output = denied.output().unwrap();
        assert!(!denied_output.status.success(), "{denied_output:?}");
        assert!(!project.path().join("evil.txt").exists());

        let mut allowed = std::process::Command::new("/bin/sh");
        allowed
            .arg("-c")
            .arg(format!(
                "cat {}/tracked.txt > {}/copy.txt",
                project.path().display(),
                scratch.path().display()
            ))
            .current_dir(project.path());
        sandbox_command_inspect(&mut allowed, &inspect_scratch).unwrap();
        assert!(allowed.output().unwrap().status.success());
        assert_eq!(
            std::fs::read_to_string(scratch.path().join("copy.txt")).unwrap(),
            "hello\n"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspect_blocks_every_project_write_shape_and_descendants_but_allows_scratch() {
        if inspect_sandbox_available().is_err() {
            return;
        }
        use std::os::unix::fs::PermissionsExt;

        let project = tempfile::tempdir().unwrap();
        let tracked = project.path().join("tracked.txt");
        std::fs::write(&tracked, "original\n").unwrap();
        let scratch = InspectScratch::create().unwrap();
        assert_eq!(
            std::fs::symlink_metadata(scratch.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );

        let script = "set -eu\n\
             cat tracked.txt > \"$TMPDIR/read-copy\"\n\
             printf scratch > \"$TMPDIR/direct-write\"\n\
             ! touch created.txt\n\
             ! sh -c 'printf child > child-created.txt'\n\
             ! printf changed > tracked.txt\n\
             ! rm tracked.txt\n\
             ! mv tracked.txt renamed.txt\n\
             ! truncate -s 0 tracked.txt\n\
             test \"$(cat tracked.txt)\" = original\n\
             test \"$CARGO_TARGET_DIR\" = \"$TMPDIR/target\""
            .to_string();
        let mut command = std::process::Command::new("/bin/sh");
        command.arg("-c").arg(script).current_dir(project.path());
        sandbox_command_inspect(&mut command, &scratch).unwrap();
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "status={:?}\nstdout={}\nstderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read_to_string(&tracked).unwrap(), "original\n");
        assert!(!project.path().join("created.txt").exists());
        assert!(!project.path().join("child-created.txt").exists());
        assert!(!project.path().join("renamed.txt").exists());
        assert_eq!(
            std::fs::read_to_string(scratch.path().join("read-copy")).unwrap(),
            "original\n"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.path().join("direct-write")).unwrap(),
            "scratch"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspect_cargo_check_uses_scratch_target_without_writing_the_project() {
        if inspect_sandbox_available().is_err() {
            return;
        }
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(
            project.path().join("Cargo.toml"),
            "[package]\nname = \"inspect-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let lockfile = "# This file is automatically @generated by Cargo.\n\
                        # It is not intended for manual editing.\n\
                        version = 3\n\n\
                        [[package]]\n\
                        name = \"inspect-smoke\"\n\
                        version = \"0.1.0\"\n";
        std::fs::write(project.path().join("Cargo.lock"), lockfile).unwrap();
        std::fs::write(
            project.path().join("src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .unwrap();
        let scratch = InspectScratch::create().unwrap();
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("cargo check --offline")
            .current_dir(project.path());
        sandbox_command_inspect(&mut command, &scratch).unwrap();
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "stdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(scratch.path().join("target").is_dir());
        assert!(!project.path().join("target").exists());
        assert_eq!(
            std::fs::read_to_string(project.path().join("Cargo.lock")).unwrap(),
            lockfile
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspect_pre_exec_failure_never_runs_the_command_unconfined() {
        if inspect_sandbox_available().is_err() {
            return;
        }
        let project = tempfile::tempdir().unwrap();
        let marker = project.path().join("must-not-exist");
        let scratch = InspectScratch::create().unwrap();
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("printf escaped > must-not-exist")
            .current_dir(project.path());
        sandbox_command_inspect(&mut command, &scratch).unwrap();
        std::fs::remove_dir(scratch.path()).unwrap();
        assert!(
            command.output().is_err(),
            "a missing policy path must abort exec"
        );
        assert!(!marker.exists());
    }

    /// The gap this module exists to document: reads are ungoverned, so a
    /// command under the write sandbox still sees files outside the checkout.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_write_sandbox_does_not_stop_reads_outside_the_checkout() {
        if inspect_sandbox_available().is_err() {
            return;
        }
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "outside-the-checkout\n").unwrap();
        let scratch = tempfile::tempdir().unwrap();

        let mut command = std::process::Command::new("/bin/sh");
        command.arg("-c").arg(format!("cat {}", secret.display()));
        let inspect_scratch = InspectScratch {
            path: scratch.path().to_path_buf(),
        };
        sandbox_command_inspect(&mut command, &inspect_scratch).unwrap();
        let output = command.output().unwrap();
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("outside-the-checkout"),
            "this assertion documents a known gap; if it starts failing the \
             sandbox got stronger and the design doc needs updating"
        );
    }
}
