//! Local human authority for connector tasks.
//!
//! Hosted MCP/GPT clients can propose work, but they cannot accept a patch or
//! approve an arbitrary command. Those decisions are intentionally available
//! only through this host-local CLI and the private SQLite state it resolves.

use crate::connector_runtime::workspace::{LocalResultDecision, WorkspaceManager};
use crate::project_entry::{resolve_local_task_state, LocalTaskState};
use crate::Database;
use serde_json::json;
use std::path::{Path, PathBuf};

const DEFAULT_PROFILE: &str = "personal";
const DEFAULT_LIST_LIMIT: usize = 20;
const EVENT_LIMIT: usize = 100;
const LOCAL_PATCH_PREVIEW_BYTES: usize = 512 * 1024;
const LOCAL_ACTOR: &str = "local_cli";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskLocationOptions {
    root: PathBuf,
    state_dir: Option<PathBuf>,
    profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TaskCliCommand {
    List {
        location: TaskLocationOptions,
        limit: usize,
        json: bool,
    },
    Show {
        location: TaskLocationOptions,
        task_id: String,
    },
    Accept {
        location: TaskLocationOptions,
        task_id: String,
    },
    Reject {
        location: TaskLocationOptions,
        task_id: String,
    },
    Resume {
        location: TaskLocationOptions,
        task_id: String,
    },
    Approve {
        location: TaskLocationOptions,
        task_id: String,
        approval_id: String,
    },
    Deny {
        location: TaskLocationOptions,
        task_id: String,
        approval_id: String,
    },
}

pub(crate) fn usage() -> &'static str {
    "Usage: webcodex task <COMMAND> [ARGS] [OPTIONS]\n\
\n\
Commands:\n\
  list                       List recent tasks for this project\n\
  show TASK_ID               Show result, approvals, and timeline\n\
  accept TASK_ID             Apply a reviewed result to this checkout\n\
  reject TASK_ID             Reject the stable result; release matching leftover slot\n\
  resume TASK_ID             Resume a preserved run after runtime restart\n\
  approve TASK_ID APPROVAL   Approve one exact raw command for one use\n\
  deny TASK_ID APPROVAL      Deny one exact raw command\n\
\n\
Options:\n\
  --root PATH                Project path; defaults to current directory\n\
  --profile NAME             Hosted connector profile; default personal\n\
  --state-dir PATH           Override private hosted state directory\n\
  --limit N                  List at most N tasks (1..=100; list only)\n\
  --json                     Emit JSON (list only)\n\
  -h, --help                 Print help and exit\n"
}

pub(crate) fn parse(args: &[String]) -> Result<TaskCliCommand, String> {
    let Some(operation) = args.first() else {
        return Err("missing task command".to_string());
    };
    if matches!(operation.as_str(), "--help" | "-h") {
        return Err("help requested".to_string());
    }
    let mut root = std::env::current_dir().map_err(|error| format!("cannot read cwd: {error}"))?;
    let mut state_dir = None;
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut limit = DEFAULT_LIST_LIMIT;
    let mut limit_set = false;
    let mut json_output = false;
    let mut positional = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--root" => root = PathBuf::from(value(&mut index)?),
            "--state-dir" => state_dir = Some(PathBuf::from(value(&mut index)?)),
            "--profile" => profile = value(&mut index)?,
            "--limit" => {
                limit_set = true;
                limit = value(&mut index)?
                    .parse::<usize>()
                    .map_err(|_| "--limit must be an integer from 1 to 100".to_string())?;
                if !(1..=100).contains(&limit) {
                    return Err("--limit must be an integer from 1 to 100".to_string());
                }
            }
            "--json" => json_output = true,
            "--help" | "-h" => return Err("help requested".to_string()),
            value if value.starts_with('-') => {
                return Err(format!("unknown task option '{value}'"))
            }
            value => positional.push(value.to_string()),
        }
        index += 1;
    }
    let location = TaskLocationOptions {
        root,
        state_dir,
        profile,
    };
    match operation.as_str() {
        "list" if positional.is_empty() => Ok(TaskCliCommand::List {
            location,
            limit,
            json: json_output,
        }),
        "list" => Err("task list accepts no positional arguments".to_string()),
        "show" | "accept" | "reject" | "resume" => {
            if positional.len() != 1 {
                return Err(format!("task {operation} requires exactly one TASK_ID"));
            }
            if limit_set || json_output {
                return Err(format!("--limit/--json are only valid with task list"));
            }
            let task_id = positional.remove(0);
            Ok(match operation.as_str() {
                "show" => TaskCliCommand::Show { location, task_id },
                "accept" => TaskCliCommand::Accept { location, task_id },
                "reject" => TaskCliCommand::Reject { location, task_id },
                "resume" => TaskCliCommand::Resume { location, task_id },
                _ => unreachable!(),
            })
        }
        "approve" | "deny" => {
            if positional.len() != 2 {
                return Err(format!("task {operation} requires TASK_ID and APPROVAL_ID"));
            }
            if limit_set || json_output {
                return Err(format!("--limit/--json are only valid with task list"));
            }
            let task_id = positional.remove(0);
            let approval_id = positional.remove(0);
            Ok(if operation == "approve" {
                TaskCliCommand::Approve {
                    location,
                    task_id,
                    approval_id,
                }
            } else {
                TaskCliCommand::Deny {
                    location,
                    task_id,
                    approval_id,
                }
            })
        }
        _ => Err(format!("unknown task command '{operation}'")),
    }
}

pub(crate) fn run(command: TaskCliCommand) -> Result<String, String> {
    match command {
        TaskCliCommand::List {
            location,
            limit,
            json,
        } => {
            let (state, db) = open_state(&location)?;
            let tasks = db
                .local_connector_tasks(&state.logical_project_id, limit)
                .map_err(store_error)?;
            let resources = WorkspaceManager::resource_status(&state.runs, &state.cargo_target);
            if json {
                pretty_json(&json!({
                    "project": state.root,
                    "resources": resources,
                    "tasks": tasks
                }))
            } else if tasks.is_empty() {
                Ok(format!(
                    "No connector tasks found for {}.\n{}",
                    state.root.display(),
                    resource_summary(&resources)
                ))
            } else {
                let mut output = format!(
                    "Tasks for {}\n{}\n",
                    state.root.display(),
                    resource_summary(&resources)
                );
                for task in tasks {
                    let goal = one_line(&task.goal, 72);
                    output.push_str(&format!(
                        "{}  {:<16} {:<9} {}\n",
                        task.task_id, task.task_status, task.mode, goal
                    ));
                }
                Ok(output.trim_end().to_string())
            }
        }
        TaskCliCommand::Show { location, task_id } => {
            let (state, db) = open_state(&location)?;
            let task = db
                .local_connector_task(&task_id, &state.logical_project_id)
                .map_err(store_error)?;
            let result = db
                .local_connector_task_result(&task_id, &state.logical_project_id)
                .map_err(store_error)?;
            let patch_preview = result
                .as_ref()
                .map(|result| WorkspaceManager::patch_preview(result, LOCAL_PATCH_PREVIEW_BYTES))
                .transpose()?
                .flatten();
            let approvals = db
                .local_connector_task_approvals(&task_id, &state.logical_project_id)
                .map_err(store_error)?;
            let events = db
                .local_connector_task_events(&task_id, &state.logical_project_id, EVENT_LIMIT)
                .map_err(store_error)?;
            let mut available_actions = Vec::new();
            if task.run_status == "interrupted" {
                available_actions.push(format!("webcodex task resume {task_id}"));
                available_actions.push(format!("webcodex task reject {task_id}"));
            }
            if result
                .as_ref()
                .is_some_and(|result| result.decision_status == "pending")
            {
                available_actions.push(format!("webcodex task accept {task_id}"));
                available_actions.push(format!("webcodex task reject {task_id}"));
            }
            for approval in approvals
                .iter()
                .filter(|approval| approval.state == "pending")
            {
                available_actions.push(format!(
                    "webcodex task approve {task_id} {}",
                    approval.approval_id
                ));
                available_actions.push(format!(
                    "webcodex task deny {task_id} {}",
                    approval.approval_id
                ));
            }
            let mut review =
                crate::connector_runtime::durable_task_review_projection(&task, result.as_ref());
            review["task_id"] = json!(task.task_id);
            review["event_cursor"] = json!(task.event_cursor);
            review["diff_preview"] = json!(patch_preview);
            review["approvals"] = json!(approvals);
            review["timeline"] = json!(events);
            review["available_actions"] = json!(available_actions);
            pretty_json(&json!({ "project": state.root, "review": review }))
        }
        TaskCliCommand::Accept { location, task_id } => decide_result(&location, &task_id, true),
        TaskCliCommand::Reject { location, task_id } => decide_result(&location, &task_id, false),
        TaskCliCommand::Resume { location, task_id } => resume_task(&location, &task_id),
        TaskCliCommand::Approve {
            location,
            task_id,
            approval_id,
        } => decide_approval(&location, &task_id, &approval_id, true),
        TaskCliCommand::Deny {
            location,
            task_id,
            approval_id,
        } => decide_approval(&location, &task_id, &approval_id, false),
    }
}

fn resume_task(location: &TaskLocationOptions, task_id: &str) -> Result<String, String> {
    let (state, db) = open_state(location)?;
    let task = db
        .local_connector_task(task_id, &state.logical_project_id)
        .map_err(store_error)?;
    ensure_target(&state, &task.target_root)?;
    WorkspaceManager::validate_resume(&task, &state.runs, &state.projects)?;
    let resumed = db
        .resume_connector_task(
            task_id,
            &state.logical_project_id,
            LOCAL_ACTOR,
            chrono::Utc::now().timestamp(),
        )
        .map_err(store_error)?;
    Ok(format!(
        "Resumed {} in its preserved workspace (run {}). Keep the local connector runtime running before retrying the hosted task.",
        resumed.task_id, resumed.run_id
    ))
}

fn decide_result(
    location: &TaskLocationOptions,
    task_id: &str,
    accept: bool,
) -> Result<String, String> {
    let (state, db) = open_state(location)?;
    let now = chrono::Utc::now().timestamp();
    WorkspaceManager::recover_result_decisions(&db, &state.logical_project_id, &state.root, now)
        .map_err(store_error)?;
    let expected_result_id = db
        .local_connector_task_result(task_id, &state.logical_project_id)
        .map_err(store_error)?
        .map(|result| result.result_id);
    let decision = if accept {
        LocalResultDecision::Accept
    } else {
        LocalResultDecision::Reject
    };
    let outcome = WorkspaceManager::decide_connector_result_local(
        &db,
        &state.logical_project_id,
        task_id,
        expected_result_id.as_deref(),
        &state.root,
        decision,
        LOCAL_ACTOR,
        now,
    )
    .map_err(store_error)?;

    let mut output = if accept {
        format!(
            "Accepted {}: applied {} changed path(s) to {}.",
            task_id,
            outcome.changed_paths.len(),
            state.root.display()
        )
    } else if expected_result_id.is_none() {
        format!(
            "Rejected interrupted {}; its uncaptured workspace changes were discarded.",
            task_id
        )
    } else {
        format!(
            "Rejected {}; its stable result was discarded and the reusable workspace is available.",
            task_id
        )
    };
    if let Some(warning) = outcome.cleanup_warning {
        output.push_str(&format!("\nCleanup warning: {warning}"));
    }
    Ok(output)
}

fn decide_approval(
    location: &TaskLocationOptions,
    task_id: &str,
    approval_id: &str,
    approve: bool,
) -> Result<String, String> {
    let (state, db) = open_state(location)?;
    let approval = db
        .decide_connector_approval(
            task_id,
            &state.logical_project_id,
            approval_id,
            approve,
            LOCAL_ACTOR,
            chrono::Utc::now().timestamp(),
        )
        .map_err(store_error)?;
    Ok(format!(
        "{} {} for {} ({}).",
        if approve { "Approved" } else { "Denied" },
        approval.approval_id,
        task_id,
        approval.action_summary
    ))
}

fn open_state(location: &TaskLocationOptions) -> Result<(LocalTaskState, Database), String> {
    let state = resolve_local_task_state(
        &location.root,
        &location.profile,
        location.state_dir.as_deref(),
    )?;
    let db_path = state.data.join("webcodex.db");
    if !db_path.is_file() {
        return Err(format!(
            "project state was not found at {}; run 'webcodex setup' first",
            state.state.display()
        ));
    }
    let db = Database::open(&db_path)
        .map_err(|error| format!("cannot open connector task state: {error}"))?;
    Ok((state, db))
}

fn ensure_target(state: &LocalTaskState, recorded: &str) -> Result<(), String> {
    if Path::new(recorded) != state.root {
        return Err(
            "task target does not match the resolved project checkout; no result was applied"
                .to_string(),
        );
    }
    Ok(())
}

fn store_error(error: impl std::fmt::Display) -> String {
    format!("connector task operation failed: {error}")
}

fn pretty_json(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| format!("cannot format task output: {error}"))
}

fn one_line(value: &str, max_chars: usize) -> String {
    let flattened = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= max_chars {
        return flattened;
    }
    let mut truncated = flattened
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn resource_summary(
    resources: &crate::connector_runtime::workspace::WorkspaceResourceStatus,
) -> String {
    format!(
        "Resources: writable slot {}; reusable checkout {}; shared Cargo cache {}{}.",
        resources.slot_state,
        format_bytes(resources.checkout.bytes),
        format_bytes(resources.cargo_cache.bytes),
        if resources.checkout.truncated || resources.cargo_cache.truncated {
            " (bounded scan)"
        } else {
            ""
        }
    )
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(root: &Path) {
        std::fs::create_dir(root).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("README.md"), "before\n").unwrap();
        git(&["add", "README.md"]);
        git(&[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ]);
    }

    #[test]
    fn parses_local_human_commands() {
        let root = tempfile::tempdir().unwrap();
        let command = parse(&[
            "approve".to_string(),
            "wc_task_0123456789abcdef0123456789abcdef".to_string(),
            "wc_apr_0123456789abcdef0123456789abcdef".to_string(),
            "--root".to_string(),
            root.path().to_string_lossy().to_string(),
            "--profile".to_string(),
            "work".to_string(),
        ])
        .unwrap();
        assert!(matches!(command, TaskCliCommand::Approve { .. }));
    }

    #[test]
    fn rejects_list_only_flags_on_decisions() {
        let error = parse(&[
            "accept".to_string(),
            "wc_task_0123456789abcdef0123456789abcdef".to_string(),
            "--json".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("only valid with task list"));
    }

    #[test]
    fn one_line_bounds_human_list_output() {
        assert_eq!(one_line("a\n  b", 10), "a b");
        assert_eq!(one_line("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn local_accept_applies_stable_result_to_resolved_checkout() {
        use crate::connector_runtime::ConnectorContext;
        use crate::db::{ConnectorBinding, NewConnectorResult, NewConnectorTask};

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let state_dir = temp.path().join("state");
        init_repo(&root);
        let state = resolve_local_task_state(&root, "personal", Some(&state_dir)).unwrap();
        std::fs::create_dir_all(&state.data).unwrap();
        let db = Database::open(&state.data.join("webcodex.db")).unwrap();
        let context = ConnectorContext {
            project_id: state.logical_project_id.clone(),
            project_name: "project".to_string(),
            workspace_id: "wc_ws_1234567890".to_string(),
            executor_project: "agent:hosted:project".to_string(),
            executor_root: root.to_string_lossy().to_string(),
            runs_root: state.runs.to_string_lossy().to_string(),
            results_root: state_dir.join("results").to_string_lossy().to_string(),
            projects_dir: state.projects.to_string_lossy().to_string(),
            profile: "personal".to_string(),
            project_grant_id: "wc_pgrant_1111111111111111".to_string(),
        };
        let manager = WorkspaceManager::new(&context).unwrap();
        db.ensure_connector_binding(ConnectorBinding {
            project_id: &context.project_id,
            project_name: &context.project_name,
            workspace_id: &context.workspace_id,
            executor_ref: &context.executor_project,
            subject_id: "user:owner",
            profile: "personal",
            now: 1,
        })
        .unwrap();
        let task_id = "wc_task_4123456789abcdef0123456789abcdef";
        let run_id = "wc_run_4123456789abcdef0123456789abcdef";
        let prepared = manager.prepare(&context, task_id, run_id, false).unwrap();
        let task = db
            .start_connector_task(NewConnectorTask {
                task_id,
                run_id,
                project_id: &context.project_id,
                workspace_id: &context.workspace_id,
                subject_id: "user:owner",
                goal: "update readme",
                mode: "normal",
                target_executor_ref: &context.executor_project,
                execution_executor_ref: &prepared.execution_executor_ref,
                target_root: &context.executor_root,
                execution_root: &prepared.execution_root,
                baseline_commit: prepared.baseline_commit.as_deref(),
                baseline_tree: prepared.baseline_tree.as_deref(),
                isolated: true,
                now: 2,
            })
            .unwrap();
        std::fs::write(Path::new(&task.execution_root).join("README.md"), "after\n").unwrap();
        let captured = manager.capture_result(&task).unwrap();
        db.finish_connector_task(
            task_id,
            &context.project_id,
            "user:owner",
            NewConnectorResult {
                result_id: "wc_result_4123456789abcdef",
                summary: "updated readme",
                patch_artifact: captured.patch_artifact.as_deref(),
                patch_sha256: captured.patch_sha256.as_deref(),
                patch_bytes: captured.patch_bytes,
                changed_paths: &captured.changed_paths,
                validation: &json!({"status": "not_run"}),
                warnings: &captured.warnings,
            },
            3,
        )
        .unwrap();
        assert_eq!(manager.release_task_workspace(&task), None);
        drop(db);

        let output = run(TaskCliCommand::Accept {
            location: TaskLocationOptions {
                root: root.clone(),
                state_dir: Some(state_dir.clone()),
                profile: "personal".to_string(),
            },
            task_id: task_id.to_string(),
        })
        .unwrap();
        assert!(output.contains("Accepted"));
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "after\n"
        );
        let db = Database::open(&state.data.join("webcodex.db")).unwrap();
        let decided = db
            .local_connector_task_result(task_id, &context.project_id)
            .unwrap()
            .unwrap();
        assert_eq!(decided.decision_status, "accepted");

        let abandoned_task_id = "wc_task_5123456789abcdef0123456789abcdef";
        let abandoned_run_id = "wc_run_5123456789abcdef0123456789abcdef";
        let prepared = manager
            .prepare(&context, abandoned_task_id, abandoned_run_id, false)
            .unwrap();
        let interrupted = db
            .start_connector_task(NewConnectorTask {
                task_id: abandoned_task_id,
                run_id: abandoned_run_id,
                project_id: &context.project_id,
                workspace_id: &context.workspace_id,
                subject_id: "user:owner",
                goal: "discard interrupted work",
                mode: "normal",
                target_executor_ref: &context.executor_project,
                execution_executor_ref: &prepared.execution_executor_ref,
                target_root: &context.executor_root,
                execution_root: &prepared.execution_root,
                baseline_commit: prepared.baseline_commit.as_deref(),
                baseline_tree: prepared.baseline_tree.as_deref(),
                isolated: true,
                now: 4,
            })
            .unwrap();
        std::fs::write(
            Path::new(&interrupted.execution_root).join("discard-me.txt"),
            "temporary\n",
        )
        .unwrap();
        db.reconcile_connector_executions(&context.project_id, 5)
            .unwrap();
        drop(db);

        let output = run(TaskCliCommand::Reject {
            location: TaskLocationOptions {
                root: root.clone(),
                state_dir: Some(state_dir.clone()),
                profile: "personal".to_string(),
            },
            task_id: abandoned_task_id.to_string(),
        })
        .unwrap();
        assert!(output.contains("uncaptured workspace changes were discarded"));
        assert!(!Path::new(&interrupted.execution_root)
            .join("discard-me.txt")
            .exists());
        let resources = WorkspaceManager::resource_status(&state.runs, &state.cargo_target);
        assert_eq!(resources.slot_state, "idle");
    }
}
