use super::runtime::runtime_with_project;
use crate::tool_runtime::git::{
    collect_show_changes_untracked_previews_for_root, git_log_command, parse_porcelain_summary,
    parse_show_changes_output, show_changes_command, split_show_changes_stdout,
};
use crate::tool_runtime::helpers::{run_command_sync, shell_escape_simple};
use crate::tool_runtime::{ApplyTextEditInput, ApplyTextEditKind, ToolRuntime};
use crate::tool_runtime::{LocalJobKiller, TerminateOutcome};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(in crate::tool_runtime::tests) fn init_git_repo(root: &Path) {
    for cmd in [
        "git init",
        "git config user.email webcodex-test@example.com",
        "git config user.name 'WebCodex Test'",
    ] {
        let (exit_code, stdout, stderr, _) = run_command_sync(cmd, root, 30);
        assert_eq!(
            exit_code, 0,
            "git setup command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}

pub(in crate::tool_runtime::tests) fn commit_file(
    root: &Path,
    path: &str,
    content: &str,
    subject: &str,
) {
    fs::write(root.join(path), content).unwrap();
    for cmd in [
        format!("git add -- {}", shell_escape_simple(path)),
        format!("git commit -m {}", shell_escape_simple(subject)),
    ] {
        let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, root, 30);
        assert_eq!(
            exit_code, 0,
            "git commit helper command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}

pub(in crate::tool_runtime::tests) fn git_log_stdout(
    root: &Path,
    limit: usize,
    skip: usize,
) -> String {
    let command = git_log_command(limit, skip);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 30);
    assert_eq!(
        exit_code, 0,
        "git log helper command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stdout
}

pub(in crate::tool_runtime::tests) fn show_changes_output_from_command(
    root: &Path,
    include_diff: bool,
) -> Value {
    let command = show_changes_command(include_diff);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 30);
    assert_eq!(
        exit_code, 0,
        "show_changes command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let (status_stdout, head_stdout, diff_stat, diff_stdout, _untracked_preview_stdout) =
        split_show_changes_stdout(&stdout, include_diff);
    let mut output = parse_show_changes_output(
        "demo",
        &status_stdout,
        &head_stdout,
        &diff_stat,
        include_diff.then_some(diff_stdout.as_str()),
        20,
        80,
        Some(exit_code),
        &stderr,
    );
    if include_diff {
        let untracked_paths = parse_porcelain_summary(&status_stdout).untracked_files;
        let (previews, truncated) =
            collect_show_changes_untracked_previews_for_root(root, &untracked_paths);
        output["untracked_previews"] = json!(previews);
        output["untracked_previews_truncated"] = json!(truncated);
    }
    output
}

/// Write a fake on-disk local job simulating a job that survived a restart.
pub(in crate::tool_runtime::tests) fn write_fake_job(
    root: &Path,
    job_id: &str,
    project: &str,
    path: &str,
    status: &str,
    stdout: &str,
    stderr: &str,
    meta_extra: Value,
) -> PathBuf {
    let dir = root.join(format!(".codex/jobs/{}", job_id));
    fs::create_dir_all(&dir).unwrap();
    let mut meta = json!({
        "job_id": job_id,
        "project": project,
        "path": path,
        "command": "echo test",
        "status": "running",
        "created_at": 1000,
        "started_at": 1000,
        "max_runtime_secs": 3600,
        "executor": "local",
        "kind": "shell",
    });
    if let (Value::Object(ref mut m), Value::Object(extra)) = (&mut meta, meta_extra) {
        for (k, v) in extra {
            m.insert(k, v);
        }
    }
    fs::write(
        dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("status"), status).unwrap();
    fs::write(dir.join("stdout.log"), stdout).unwrap();
    fs::write(dir.join("stderr.log"), stderr).unwrap();
    dir
}

/// A deterministic fake process-killer for testing timeout/stop invariants.
/// Records which (pid, pgid) pairs it was asked to terminate and reports
/// AlreadyGone so the runtime persists a terminal status without touching
/// any real process.
#[derive(Default, Clone)]
pub(in crate::tool_runtime::tests) struct FakeJobKiller {
    calls: Arc<std::sync::Mutex<Vec<(i64, i64)>>>,
}

impl FakeJobKiller {
    pub(in crate::tool_runtime::tests) fn calls(&self) -> Vec<(i64, i64)> {
        self.calls.lock().unwrap().clone()
    }
}

impl LocalJobKiller for FakeJobKiller {
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
        self.calls.lock().unwrap().push((pid, pgid));
        TerminateOutcome::AlreadyGone
    }
}

pub(in crate::tool_runtime::tests) fn runtime_with_fake_killer(
    root: &Path,
    project_id: &str,
) -> (ToolRuntime, FakeJobKiller) {
    let mut runtime = runtime_with_project(root, project_id);
    let killer = FakeJobKiller::default();
    let killer_dyn: Arc<dyn LocalJobKiller> = Arc::new(killer.clone());
    runtime.job_killer = killer_dyn;
    (runtime, killer)
}

/// Write a fake on-disk local job plus a `pid` file and `process_group_id`
/// metadata field, simulating a job spawned by the current code.
pub(in crate::tool_runtime::tests) fn write_fake_job_with_pgid(
    root: &Path,
    job_id: &str,
    project: &str,
    path: &str,
    status: &str,
    pid: i64,
    meta_extra: Value,
) -> PathBuf {
    let dir = write_fake_job(root, job_id, project, path, status, "", "", meta_extra);
    fs::write(dir.join("pid"), pid.to_string()).unwrap();
    dir
}

/// A small patch carrying a distinctive marker line so tests can prove the
/// patch body never leaks into the shell `command` string.
pub(in crate::tool_runtime::tests) fn marker_patch(filename: &str, marker: &str) -> String {
    format!(
        "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1 @@\n+{m}\n",
        f = filename,
        m = marker,
    )
}

/// A patch deliberately larger than the agent shell command limit
/// (`MAX_COMMAND_LEN` = 8000 bytes) so tests can prove the patch still
/// validates/applies via `stdin` rather than the command string.
pub(in crate::tool_runtime::tests) fn large_marker_patch(filename: &str, marker: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1,200 @@\n",
        f = filename,
    ));
    s.push_str(&format!("+{m}\n", m = marker));
    for i in 0..199 {
        s.push_str(&format!("+line-{:04}-{}\n", i, "x".repeat(48)));
    }
    s
}

pub(in crate::tool_runtime::tests) fn text_edit(
    kind: ApplyTextEditKind,
    old_text: Option<&str>,
    new_text: Option<&str>,
    anchor_text: Option<&str>,
) -> ApplyTextEditInput {
    ApplyTextEditInput {
        kind,
        old_text: old_text.map(str::to_string),
        new_text: new_text.map(str::to_string),
        anchor_text: anchor_text.map(str::to_string),
    }
}
