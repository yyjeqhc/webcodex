//! Actual Campus stdio provider + owned Chromium, using only a fictional profile.
//! This adapter resolves one synthetic Project's relative upload path; it does not
//! exercise Runner Project authorization or claim to measure model round trips.
use super::widgets::with_widget_page;
use super::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const FIXTURE_PROJECT: &str = "campus-fixture";

struct CampusProvider {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    calls: usize,
}

impl CampusProvider {
    fn start(resume: &Path) -> Self {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut child = Command::new("node")
            .arg(repo.join("plugins/campus-application/fixtures/campus-provider-driver.mjs"))
            .current_dir(&repo)
            .env("CAMPUS_FIXTURE_RESUME_PATH", resume)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("owned E2E requires Node and the built Campus provider");
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            calls: 0,
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        // The helper enforces a 30-second response timeout and terminates on errors.
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(
            &mut *input,
            &json!({ "name": name, "arguments": arguments }),
        )
        .unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "actual provider ended without a reply"
        );
        self.calls += 1;
        serde_json::from_str(&line).expect("provider helper returns one structured result per line")
    }

    fn finish(&mut self) {
        self.input.take();
        assert!(
            self.child.wait().unwrap().success(),
            "provider helper must clean up successfully"
        );
    }
}

impl Drop for CampusProvider {
    fn drop(&mut self) {
        self.input.take();
        // Closing stdin lets the helper clean up its own provider/profile directory.
        // The fallback can only kill this test-owned helper, never a user process.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
    }
}

fn observe_fields(s: &BrowserSupervisor, b: &str, p: &str) -> SemanticSnapshot {
    let query = crate::BrowserSnapshotQuery {
        fields_only: true,
        ..Default::default()
    };
    let snapshot = s
        .snapshot_query(b, p, SnapshotMode::Auto, 256, 32, 0, Some(&query))
        .unwrap();
    assert!(
        !snapshot.truncated,
        "owned fixture must fit one semantic query"
    );
    snapshot
}

fn scope(snapshot: &SemanticSnapshot, page: &PageSummary) -> Value {
    let nodes: Vec<_> = snapshot
        .nodes
        .iter()
        .filter(|node| {
            !node
                .name
                .as_deref()
                .unwrap_or("")
                .starts_with("Fixture report")
        })
        .collect();
    json!({
        "client_id": "fixture",
        "browser_id": snapshot.browser_id,
        "page_id": snapshot.page_id,
        "snapshot_generation": snapshot.snapshot_generation,
        "url": page.url,
        "nodes": nodes
    })
}

fn plan_arguments(snapshot: &SemanticSnapshot, page: &PageSummary, upload_root: &Path) -> Value {
    let mut arguments = scope(snapshot, page);
    arguments["title"] = json!(page.title);
    arguments["widget_batch_limit"] = json!(5);
    arguments["upload_source"] = json!({ "project": FIXTURE_PROJECT, "project_root": upload_root });
    arguments
}

fn operations(result: &Value, b: &str, p: &str, upload_root: &Path) -> Vec<BatchOperation> {
    let batch = &result["batch"];
    assert_eq!(batch["action"], "batch", "{result}");
    assert_eq!(batch["client_id"], "fixture");
    assert_eq!(batch["browser_id"], b);
    assert_eq!(batch["page_id"], p);
    batch["operations"]
        .as_array()
        .expect("executable batch operations")
        .iter()
        .map(|operation| {
            let mut operation = operation.clone();
            if operation["action"] == "upload_file" {
                assert_eq!(operation["project"], FIXTURE_PROJECT);
                let relative = PathBuf::from(operation["path"].as_str().unwrap());
                assert!(!relative.as_os_str().is_empty());
                assert!(
                    relative
                        .components()
                        .all(|part| matches!(part, Component::Normal(_))),
                    "fixture upload must retain a project-relative path"
                );
                let resolved = upload_root.join(relative);
                assert!(resolved.is_file());
                assert!(resolved
                    .canonicalize()
                    .unwrap()
                    .starts_with(upload_root.canonicalize().unwrap()));
                operation.as_object_mut().unwrap().remove("project");
                operation["path"] = json!(resolved);
            }
            serde_json::from_value(operation)
                .expect("Campus output must use canonical Browser batch operations")
        })
        .collect()
}

fn assert_completed(receipt: &BatchResult, expected: usize) {
    assert_eq!(
        receipt.execution_state,
        ExecutionState::Completed,
        "{receipt:?}"
    );
    assert_eq!(receipt.requested_count, expected);
    assert_eq!(receipt.completed_count, expected, "{receipt:?}");
    assert_eq!(receipt.remaining_count, 0);
    assert!(receipt.error.is_none(), "{receipt:?}");
    assert!(
        !receipt.needs_snapshot,
        "stable sibling fields must preserve batch authority"
    );
    assert!(
        receipt
            .stability
            .as_ref()
            .is_some_and(|stability| stability.stable),
        "{receipt:?}"
    );
}

fn reconcile_arguments(
    snapshot: &SemanticSnapshot,
    page: &PageSummary,
    plan: &Value,
    receipt: &BatchResult,
) -> Value {
    let mut arguments = scope(snapshot, page);
    arguments["plan_id"] = plan["plan_id"].clone();
    arguments["receipt"] = json!({
        "execution_state": receipt.execution_state,
        "requested_count": receipt.requested_count,
        "completed_count": receipt.completed_count,
        "remaining_count": receipt.remaining_count,
        "stability": receipt.stability
    });
    arguments
}

fn assert_no_attention(result: &Value) {
    assert_eq!(result["needs_attention"], json!([]), "{result}");
}

fn assert_requested_values(
    source: &SemanticSnapshot,
    operations: &[BatchOperation],
    readback: &SemanticSnapshot,
) {
    for operation in operations {
        let value = match operation {
            BatchOperation::InputText { text, .. } => text.clone(),
            BatchOperation::SetValue { value, .. } | BatchOperation::SetDate { value, .. } => {
                value.clone()
            }
            BatchOperation::SelectChoice { choice_path, .. } => choice_path.join(" / "),
            // Upload is independently checked by actual reconciliation and the fixture's change monitor.
            BatchOperation::UploadFile { .. } => continue,
            _ => panic!("unexpected 26-field fixture operation: {operation:?}"),
        };
        let id = operation.authority().0;
        let original = source
            .nodes
            .iter()
            .find(|node| node.element_id.as_deref() == Some(id))
            .unwrap();
        let matches: Vec<_> = readback
            .nodes
            .iter()
            .filter(|node| node.role == original.role && node.name == original.name)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "field identity must be unambiguous: {original:?}"
        );
        assert_eq!(
            matches[0].value.as_deref(),
            Some(value.as_str()),
            "{original:?}"
        );
    }
}

#[test]
#[ignore = "requires local Chromium, Node, and built plugins/campus-application/dist/plugin.js"]
fn chromium_campus_e2e_actual_provider_confirms_twenty_six_fields() {
    let files = tempfile::tempdir().unwrap();
    let resume = files.path().join("fictional-resume.pdf");
    std::fs::write(
        &resume,
        b"%PDF-1.0\n% synthetic owned fixture, no personal data\n%%EOF\n",
    )
    .unwrap();
    let setup = r#"
        for (const name of ["姓名","姓","名","性别","出生日期","身份证号","民族","政治面貌",
          "健康状况","籍贯","户籍","生源地","婚姻状况","邮箱","手机","城市","地址","学校","学历","专业"]) addNative(name);
        addChoice("学历", [["硕士","本科"]]);
        addChoice("政治面貌", [["群众","党员"]]);
        addChoice("性别", [["男","女"]]);
        addChoice("籍贯", [["Sichuan","Guangdong"],["Chengdu","Mianyang"]]);
        addDate("毕业日期"); addUpload("简历");
    "#;
    with_widget_page(setup, |s, b, p| {
        // Launch/navigation and the page metadata lookup are fixture bootstrap.
        let page = s
            .pages(b, 1)
            .unwrap()
            .into_iter()
            .find(|page| page.page_id == p)
            .unwrap();
        let started = Instant::now();
        let mut provider = CampusProvider::start(&resume);
        let initial = observe_fields(s, b, p);
        let plan_args = plan_arguments(&initial, &page, files.path());
        assert_eq!(plan_args["nodes"].as_array().unwrap().len(), 26);
        let plan = provider.call("plan_fill", plan_args);
        assert_no_attention(&plan);
        assert_eq!(plan["deferred_count"], 5, "{plan}");
        let native = operations(&plan, b, p, files.path());
        assert_eq!(native.len(), 21, "{plan}");
        assert_eq!(
            native
                .iter()
                .filter(|operation| matches!(
                    operation,
                    BatchOperation::InputText { .. } | BatchOperation::SetValue { .. }
                ))
                .count(),
            20
        );
        assert_eq!(
            native
                .iter()
                .filter(|operation| matches!(operation, BatchOperation::UploadFile { .. }))
                .count(),
            1
        );
        let native_receipt = s.batch(b, p, &native).unwrap();
        assert_completed(&native_receipt, 21);
        let middle = observe_fields(s, b, p);
        let continued = provider.call(
            "reconcile_fill",
            reconcile_arguments(&middle, &page, &plan, &native_receipt),
        );
        assert_eq!(continued["confirmed"], 21, "{continued}");
        assert_no_attention(&continued);
        let widgets = operations(&continued, b, p, files.path());
        assert_eq!(widgets.len(), 5, "{continued}");
        assert_eq!(
            widgets
                .iter()
                .filter(|operation| matches!(operation, BatchOperation::SelectChoice { .. }))
                .count(),
            4
        );
        assert_eq!(
            widgets
                .iter()
                .filter(|operation| matches!(operation, BatchOperation::SetDate { .. }))
                .count(),
            1
        );
        let widget_receipt = s.batch(b, p, &widgets).unwrap();
        assert_completed(&widget_receipt, 5);
        let final_snapshot = observe_fields(s, b, p);
        let done = provider.call(
            "reconcile_fill",
            reconcile_arguments(&final_snapshot, &page, &continued, &widget_receipt),
        );
        assert_eq!(done["confirmed"], 5, "{done}");
        assert_no_attention(&done);
        assert!(
            done.get("batch").is_none() && done.get("plan_id").is_none(),
            "{done}"
        );
        let elapsed_ms = started.elapsed().as_millis();
        assert_requested_values(&initial, &native, &final_snapshot);
        assert_requested_values(&middle, &widgets, &final_snapshot);
        let monitor = final_snapshot
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some("Fixture report: 简历"))
            .unwrap();
        let upload: Value = serde_json::from_str(monitor.value.as_deref().unwrap()).unwrap();
        assert_eq!(upload["value"], "fictional-resume.pdf");
        assert_eq!(upload["changes"], 1);
        assert_eq!(provider.calls, 3);
        println!(
            "CAMPUS_BROWSER_E2E {}",
            json!({
                "fields":26, "native_fields":20, "custom_widgets":5, "uploads":1,
                "browser_calls":5, "semantic_queries":3, "batch_calls":2,
                "actual_provider_calls":provider.calls, "primitive_operations":26,
            "native_input_text":native.iter().filter(|operation| matches!(operation, BatchOperation::InputText { .. })).count(),
            "native_set_value":native.iter().filter(|operation| matches!(operation, BatchOperation::SetValue { .. })).count(),
                "confirmed_fields":26, "needs_attention":0, "screenshots":0, "pointer_operations":0,
                "elapsed_ms":elapsed_ms,
                "derived_combined_orchestration_rounds":6,
                "model_round_trips":"not measured; six rounds are derived from query/provider pairing and batch execution",
                "timing_scope":"provider spawn plus initial fields query through final actual-provider reconciliation; excludes Chromium bootstrap",
                "upload_boundary":"synthetic Project path resolution only; Runner authorization is outside this fixture"
            })
        );
        provider.finish();
    });
}

#[test]
#[ignore = "requires local Chromium and actual Campus provider; select value/label disagreement must never replay"]
fn chromium_campus_e2e_native_select_label_difference_blocks_replay() {
    let files = tempfile::tempdir().unwrap();
    let resume = files.path().join("unused-fictional-resume.pdf");
    let setup = r#"
        const label=document.createElement("label");label.textContent="性别";
        const select=document.createElement("select");select.setAttribute("aria-label","性别");
        for(const [value,text] of [["","Choose"],["男","Fictional selected male"],["女","Fictional selected female"]]){
          const option=document.createElement("option");option.value=value;option.textContent=text;select.append(option);
        }
        label.append(select);form.append(label);
        const counts=states["Native select events"]={input:0,change:0};
        for(const event of ["input","change"])select.addEventListener(event,()=>{counts[event]++;publish();});
        publish();
    "#;
    with_widget_page(setup, |s, b, p| {
        let page = s
            .pages(b, 1)
            .unwrap()
            .into_iter()
            .find(|page| page.page_id == p)
            .unwrap();
        let mut provider = CampusProvider::start(&resume);
        let initial = observe_fields(s, b, p);
        let plan = provider.call("plan_fill", plan_arguments(&initial, &page, files.path()));
        assert_no_attention(&plan);
        let native = operations(&plan, b, p, files.path());
        assert_eq!(native.len(), 1, "{plan}");
        assert!(
            matches!(&native[0], BatchOperation::SelectOption { option, .. } if option == "男")
        );
        let receipt = s.batch(b, p, &native).unwrap();
        assert_completed(&receipt, 1);
        let readback = observe_fields(s, b, p);
        let selected = readback
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some("性别"))
            .unwrap();
        assert_eq!(selected.value.as_deref(), Some("Fictional selected male"));
        let done = provider.call(
            "reconcile_fill",
            reconcile_arguments(&readback, &page, &plan, &receipt),
        );
        assert_eq!(done["confirmed"], 0, "{done}");
        assert_eq!(
            done["needs_attention"].as_array().unwrap().len(),
            1,
            "{done}"
        );
        assert_eq!(done["needs_attention"][0]["status"], "unresolved", "{done}");
        assert!(
            done.get("batch").is_none(),
            "completed native selection must never be replayed: {done}"
        );
        let event_counts = |snapshot: &SemanticSnapshot| -> Value {
            let node = snapshot
                .nodes
                .iter()
                .find(|node| node.name.as_deref() == Some("Fixture report: Native select events"))
                .unwrap();
            serde_json::from_str(node.value.as_deref().unwrap()).unwrap()
        };
        let counts = event_counts(&readback);
        assert_eq!(counts, json!({"input":1,"change":1}));
        s.select_option(b, p, selected.element_id.as_deref().unwrap(), "男")
            .unwrap();
        let after_noop = observe_fields(s, b, p);
        assert_eq!(
            event_counts(&after_noop),
            counts,
            "already-selected native value must not emit more events"
        );
        provider.finish();
    });
}
