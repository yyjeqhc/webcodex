use super::*;

#[derive(Default)]
pub(super) struct BatchProbe {
    effects: Vec<String>,
    pub(super) waits: usize,
    replace_after: Option<usize>,
    close_after: Option<usize>,
    uncertain_at: Option<usize>,
    pub(super) fail_freshness: bool,
    pub(super) fail_after_effect: bool,
    pub(super) replace_on_settle: bool,
}

impl FakeBackend {
    pub(super) fn record_batch_effect(&mut self, effect: String) -> BrowserResult<()> {
        let Some(probe) = &self.batch_probe else {
            return Ok(());
        };
        let mut probe = probe.lock().unwrap();
        probe.effects.push(effect);
        if probe.fail_after_effect {
            probe.fail_freshness = true;
        }
        if probe.replace_after == Some(probe.effects.len()) {
            self.pages[0].document_id = "replaced-document".into();
        }
        if probe.close_after == Some(probe.effects.len()) {
            self.pages.clear();
        }
        if probe.uncertain_at == Some(probe.effects.len()) {
            return Err(BrowserError::uncertain(
                "fixture_disconnect",
                "lost reply after dispatch",
                "snapshot",
            ));
        }
        Ok(())
    }
}

pub(super) fn form_nodes() -> Vec<BackendNode> {
    vec![
        fixture_node("textbox", "Name", Some(1), ControlCapability::text_input()),
        fixture_node("textbox", "Email", Some(2), ControlCapability::text_input()),
        fixture_node(
            "combobox",
            "Degree",
            Some(3),
            ControlCapability::select_option(),
        ),
        fixture_node(
            "textbox",
            "Month",
            Some(4),
            ControlCapability::exact_value(),
        ),
        fixture_node("checkbox", "Consent", Some(5), ControlCapability::click()),
        fixture_node("radio", "Location", Some(6), ControlCapability::click()),
        fixture_node(
            "button",
            "Resume",
            Some(7),
            ControlCapability::file_upload(),
        ),
    ]
}

struct BatchFactory(Arc<Mutex<BatchProbe>>);
impl BackendFactory for BatchFactory {
    fn available(&self) -> bool {
        true
    }
    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        let mut backend = FakeBackend::new();
        backend.batch_probe = Some(self.0.clone());
        Ok(Box::new(backend))
    }
}

fn setup() -> (
    BrowserSupervisor,
    String,
    String,
    Vec<String>,
    Arc<Mutex<BatchProbe>>,
) {
    let probe = Arc::new(Mutex::new(BatchProbe::default()));
    let supervisor = BrowserSupervisor::with_factory(Arc::new(BatchFactory(probe.clone())));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 8).unwrap().remove(0).page_id;
    let ids = snapshot_ids(&supervisor, &browser, &page);
    (supervisor, browser, page, ids, probe)
}

fn snapshot_ids(supervisor: &BrowserSupervisor, browser: &str, page: &str) -> Vec<String> {
    supervisor
        .snapshot(browser, page, SnapshotMode::Interactive, 32, 32)
        .unwrap()
        .nodes
        .into_iter()
        .map(|n| n.element_id.unwrap())
        .collect()
}

fn inputs(ids: &[String]) -> Vec<BatchOperation> {
    vec![
        BatchOperation::InputText {
            element_id: ids[0].clone(),
            text: "Alice".into(),
        },
        BatchOperation::InputText {
            element_id: ids[1].clone(),
            text: "a@example.test".into(),
        },
    ]
}

#[test]
fn batch_preserves_order_all_field_kinds_and_settles_once() {
    let (s, b, p, ids, probe) = setup();
    let mut operations = inputs(&ids);
    operations.extend([
        BatchOperation::SelectOption {
            element_id: ids[2].clone(),
            option: "Bachelor".into(),
        },
        BatchOperation::SetValue {
            element_id: ids[3].clone(),
            value: "2027-06".into(),
        },
        BatchOperation::Click {
            element_id: ids[4].clone(),
        },
        BatchOperation::Click {
            element_id: ids[5].clone(),
        },
        BatchOperation::UploadFile {
            element_id: ids[6].clone(),
            path: "resume.pdf".into(),
        },
    ]);
    let receipt = s.batch(&b, &p, &operations).unwrap();
    assert_eq!(receipt.execution_state, ExecutionState::Completed);
    assert_eq!(receipt.requested_count, 7);
    assert_eq!(receipt.completed_count, 7);
    assert_eq!(receipt.remaining_count, 0);
    assert!(receipt.stopped_at_index.is_none());
    assert!(!receipt.needs_snapshot);
    assert!(receipt.error.is_none());
    assert_eq!(probe.lock().unwrap().waits, 1);
    assert_eq!(
        probe.lock().unwrap().effects,
        [
            "input:1:Alice",
            "input:2:a@example.test",
            "select:3:Bachelor",
            "value:4:2027-06",
            "click:5",
            "click:6",
            "upload:7:resume.pdf"
        ]
    );
    // Ordinary effects preserve sibling authority even across batch calls.
    assert_eq!(s.batch(&b, &p, &inputs(&ids)).unwrap().completed_count, 2);
}

#[test]
fn batch_stops_after_document_replacement_including_click() {
    for click in [false, true] {
        let (s, b, p, ids, probe) = setup();
        probe.lock().unwrap().replace_after = Some(1);
        let mut ops = inputs(&ids);
        if click {
            ops[0] = BatchOperation::Click {
                element_id: ids[4].clone(),
            };
        }
        let receipt = s.batch(&b, &p, &ops).unwrap();
        assert_eq!(receipt.execution_state, ExecutionState::Completed);
        assert_eq!(receipt.completed_count, 1);
        assert_eq!(receipt.stopped_at_index, Some(0));
        assert_eq!(receipt.remaining_count, 1);
        assert_eq!(
            receipt.error.unwrap().execution_state,
            ExecutionState::Completed
        );
        assert!(receipt.needs_snapshot);
        assert_eq!(probe.lock().unwrap().effects.len(), 1);
        assert_eq!(probe.lock().unwrap().waits, 0);
    }
}

#[test]
fn batch_unknown_is_fail_stop_without_retry_or_settle() {
    let (s, b, p, ids, probe) = setup();
    probe.lock().unwrap().uncertain_at = Some(2);
    let mut ops = inputs(&ids);
    ops.push(BatchOperation::Click {
        element_id: ids[4].clone(),
    });
    let receipt = s.batch(&b, &p, &ops).unwrap();
    assert_eq!(receipt.execution_state, ExecutionState::OutcomeUnknown);
    assert_eq!(receipt.completed_count, 1);
    assert_eq!(receipt.stopped_at_index, Some(1));
    assert_eq!(receipt.remaining_count, 1);
    assert!(receipt.needs_snapshot);
    let error = receipt.error.unwrap();
    assert_eq!(error.execution_state, ExecutionState::OutcomeUnknown);
    assert_eq!(error.recovery_action, Some("snapshot"));
    assert_eq!(probe.lock().unwrap().effects.len(), 2);
    assert_eq!(probe.lock().unwrap().waits, 0);
}

#[test]
fn batch_invalid_value_fails_before_dispatch_without_normalizing_intent() {
    for value in [String::new(), "bad\0value".into(), "界".repeat(4096)] {
        let (s, b, p, ids, probe) = setup();
        let mut ops = inputs(&ids);
        ops[1] = BatchOperation::InputText {
            element_id: ids[1].clone(),
            text: value,
        };
        ops.push(BatchOperation::Click {
            element_id: ids[4].clone(),
        });
        let receipt = s.batch(&b, &p, &ops).unwrap();
        assert_eq!(receipt.execution_state, ExecutionState::Completed);
        assert_eq!(receipt.completed_count, 1);
        assert_eq!(receipt.stopped_at_index, Some(1));
        assert_eq!(receipt.remaining_count, 2);
        assert_eq!(
            receipt.error.unwrap().execution_state,
            ExecutionState::NotStarted
        );
        assert_eq!(probe.lock().unwrap().effects.len(), 1);
    }
}

#[test]
fn batch_fences_wrong_page_browser_generation_document_and_action() {
    for case in [
        "page",
        "browser",
        "snapshot",
        "generation",
        "document",
        "action",
        "missing_page",
    ] {
        let (s, mut b, mut p, ids, probe) = setup();
        let mut ops = inputs(&ids);
        match case {
            "page" => p = s.new_page(&b).unwrap().page_id,
            "browser" => {
                b = s.launch().unwrap().browser_id;
                p = s.pages(&b, 8).unwrap().remove(0).page_id;
            }
            "snapshot" => {
                snapshot_ids(&s, &b, &p);
            }
            "generation" => {
                s.state()
                    .browsers
                    .get_mut(&b)
                    .unwrap()
                    .pages
                    .get_mut(&p)
                    .unwrap()
                    .snapshot_generation += 1;
            }
            "document" => {
                s.state()
                    .browsers
                    .get_mut(&b)
                    .unwrap()
                    .backend
                    .navigate("private-target", "https://example.test")
                    .unwrap();
            }
            "action" => {
                ops[0] = BatchOperation::InputText {
                    element_id: ids[4].clone(),
                    text: "unsafe".into(),
                }
            }
            "missing_page" => s.close_page(&b, &p).unwrap(),
            _ => unreachable!(),
        }
        match s.batch(&b, &p, &ops) {
            Ok(receipt) => {
                assert_eq!(
                    receipt.execution_state,
                    ExecutionState::NotStarted,
                    "{case}"
                );
                assert_eq!(receipt.completed_count, 0, "{case}");
                assert_eq!(receipt.remaining_count, 2, "{case}");
                assert_eq!(
                    receipt.error.unwrap().execution_state,
                    ExecutionState::NotStarted
                );
            }
            Err(error) => assert_eq!(error.execution_state, ExecutionState::NotStarted),
        }
        assert!(probe.lock().unwrap().effects.is_empty(), "{case}");
    }
}

#[test]
fn batch_bound_and_final_settle_document_change() {
    let (s, b, p, ids, probe) = setup();
    for ops in [vec![], vec![inputs(&ids)[0].clone(); 33]] {
        assert_eq!(
            s.batch(&b, &p, &ops).unwrap_err().execution_state,
            ExecutionState::NotStarted
        );
    }
    probe.lock().unwrap().replace_on_settle = true;
    let receipt = s.batch(&b, &p, &inputs(&ids)).unwrap();
    assert_eq!(receipt.completed_count, 2);
    assert_eq!(receipt.remaining_count, 0);
    assert!(receipt.needs_snapshot);
    assert_eq!(receipt.error.unwrap().kind, "stale_element");
    assert_eq!(probe.lock().unwrap().waits, 1);
}

#[test]
fn batch_failed_freshness_never_dispatches_another_effect() {
    for after_effect in [false, true] {
        let (s, b, p, ids, probe) = setup();
        probe.lock().unwrap().fail_freshness = !after_effect;
        probe.lock().unwrap().fail_after_effect = after_effect;
        let receipt = s.batch(&b, &p, &inputs(&ids)).unwrap();
        assert_eq!(receipt.completed_count, usize::from(after_effect));
        assert_eq!(receipt.remaining_count, 2 - usize::from(after_effect));
        assert_eq!(
            receipt.execution_state,
            if after_effect {
                ExecutionState::Completed
            } else {
                ExecutionState::NotStarted
            }
        );
        assert!(receipt.needs_snapshot);
        assert_eq!(
            probe.lock().unwrap().effects.len(),
            usize::from(after_effect)
        );
        assert_eq!(probe.lock().unwrap().waits, 0);
    }
}

#[test]
fn batch_current_then_old_snapshot_identity_stops_at_old_operation() {
    let (s, b, p, old, probe) = setup();
    let current = snapshot_ids(&s, &b, &p);
    let ops = vec![
        inputs(&current)[0].clone(),
        inputs(&old)[1].clone(),
        inputs(&current)[1].clone(),
    ];
    let receipt = s.batch(&b, &p, &ops).unwrap();
    assert_eq!(receipt.completed_count, 1);
    assert_eq!(receipt.remaining_count, 2);
    assert_eq!(receipt.stopped_at_index, Some(1));
    assert_eq!(receipt.error.unwrap().kind, "stale_element");
    assert_eq!(probe.lock().unwrap().effects.len(), 1);
}

#[test]
fn batch_closed_page_preserves_page_inventory_recovery() {
    let (s, b, p, ids, probe) = setup();
    probe.lock().unwrap().close_after = Some(1);
    let receipt = s.batch(&b, &p, &inputs(&ids)).unwrap();
    assert_eq!(receipt.completed_count, 1);
    assert_eq!(receipt.remaining_count, 1);
    let error = receipt.error.unwrap();
    assert_eq!(error.kind, "stale_page");
    assert_eq!(error.recovery_action, Some("pages"));
    assert_eq!(probe.lock().unwrap().effects.len(), 1);
}
