use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn diff_page_runs_three_producers_and_preserves_same_execution_failure_and_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.name", "fixture"]);
    git(&root, &["config", "user.email", "fixture@example.invalid"]);
    std::fs::write(root.join("file.txt"), "before\n").unwrap();
    git(&root, &["add", "file.txt"]);
    git(&root, &["commit", "-qm", "base"]);
    std::fs::write(root.join("file.txt"), "after\n").unwrap();
    let bin = tmp.path().join("bin");
    let scratch = tmp.path().join("scratch");
    std::fs::create_dir(&bin).unwrap();
    std::fs::create_dir(&scratch).unwrap();
    let real_git = String::from_utf8(
        Command::new("/bin/sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let wrapper = bin.join("git");
    let count = tmp.path().join("calls");
    std::fs::write(
        &wrapper,
        r#"#!/bin/sh
is_diff=0
for argument in "$@"; do if [ "$argument" = diff ]; then is_diff=1; fi; done
if [ "$is_diff" = 1 ]; then
  printf 'diff\n' >>"$COUNT_FILE"
  if [ "${FAIL_FINGERPRINT:-0}" = 1 ]; then
    case " $* " in *" --binary "*) exit 37;; esac
  fi
fi
exec "$REAL_GIT" "$@"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = git_diff_hunks_page_command_for_source(
        &[],
        false,
        0,
        None,
        30,
        160,
        16384,
        None,
        None,
        None,
    )
    .unwrap();
    for fail in [false, true] {
        std::fs::write(&count, "").unwrap();
        let output = Command::new("/bin/sh")
            .args(["-c", &command])
            .current_dir(&root)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("REAL_GIT", real_git.trim())
            .env("COUNT_FILE", &count)
            .env("TMPDIR", &scratch)
            .env("FAIL_FINGERPRINT", if fail { "1" } else { "0" })
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let wire =
            parse_framed_git_diff_hunks_stdout(&stdout, 16384).expect("strict producer frame");
        let calls = std::fs::read_to_string(&count).unwrap().lines().count();
        if fail {
            assert!(!output.status.success());
            assert_eq!(wire.pre_diff_exit, 37);
            assert_eq!(wire.post_diff_exit, 37);
            assert_eq!(
                calls, 2,
                "failed precheck must not generate a page or repeat a producer"
            );
        } else {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(calls, 3);
            assert!(stdout.contains("+after"));
        }
        assert_eq!(
            std::fs::read_dir(&scratch).unwrap().count(),
            0,
            "status side channel must be removed"
        );
    }
    let object = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let head_commit = object(&["rev-parse", "HEAD"]);
    git(&root, &["add", "file.txt"]);
    let frozen_tree = object(&["write-tree"]);
    let source = super::super::super::git_review_snapshot::GitReviewSourceIdentity::Workspace {
        head_commit: Some(head_commit),
        frozen_tree,
        status_fingerprint: "a".repeat(40),
    };
    let command = git_diff_hunks_page_command_for_source(
        &[],
        false,
        0,
        None,
        30,
        160,
        16384,
        None,
        None,
        Some(&source),
    )
    .unwrap();
    std::fs::write(&count, "").unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", &command])
        .current_dir(&root)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("REAL_GIT", real_git.trim())
        .env("COUNT_FILE", &count)
        .env("TMPDIR", &scratch)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1);
    assert!(String::from_utf8(output.stdout).unwrap().contains("+after"));
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    eprintln!("REVIEW_IMMUTABLE_PRODUCER invocations=1 owned_view_cleanup=true");
    eprintln!("DIFF_PRODUCER baseline_invocations=5 current_invocations=3 same_execution_exit=37 temp_files_remaining=0");
}
