use super::*;

struct WidgetFactory(Arc<Mutex<batch::BatchProbe>>);

impl BackendFactory for WidgetFactory {
    fn available(&self) -> bool {
        true
    }

    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        let mut backend = FakeBackend::new();
        backend.query_nodes = Some(vec![
            fixture_node(
                "textbox",
                "Native",
                Some(41),
                ControlCapability::native_text_input(),
            ),
            fixture_node(
                "combobox",
                "Choice",
                Some(42),
                ControlCapability {
                    custom_choice: true,
                    ..ControlCapability::click()
                },
            ),
            fixture_node(
                "combobox",
                "Date",
                Some(43),
                ControlCapability {
                    custom_date: true,
                    ..ControlCapability::click()
                },
            ),
        ]);
        backend.batch_probe = Some(self.0.clone());
        Ok(Box::new(backend))
    }
}

fn mock_widgets() -> (
    BrowserSupervisor,
    String,
    String,
    Arc<Mutex<batch::BatchProbe>>,
) {
    let probe = Arc::new(Mutex::new(batch::BatchProbe::default()));
    let supervisor = BrowserSupervisor::with_factory(Arc::new(WidgetFactory(probe.clone())));
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 1).unwrap().remove(0).page_id;
    (supervisor, browser, page, probe)
}

fn observe_widgets(s: &BrowserSupervisor, b: &str, p: &str) -> SemanticSnapshot {
    s.snapshot(b, p, SnapshotMode::Interactive, 256, 256)
        .unwrap()
}

fn element(snapshot: &SemanticSnapshot, name: &str) -> String {
    let matches = snapshot
        .nodes
        .iter()
        .filter(|node| node.name.as_deref() == Some(name) && node.element_id.is_some())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "expected one admitted {name:?}: {:?}",
        snapshot.nodes
    );
    matches[0].element_id.clone().unwrap()
}

fn field_value<'a>(snapshot: &'a SemanticSnapshot, name: &str) -> &'a str {
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing field {name:?}: {:?}", snapshot.nodes));
    // The existing sparse projection omits an empty value.
    node.value.as_deref().unwrap_or("")
}

fn report(snapshot: &SemanticSnapshot) -> serde_json::Value {
    let mut combined: serde_json::Value =
        serde_json::from_str(field_value(snapshot, "Fixture report")).unwrap();
    for node in &snapshot.nodes {
        if let Some(name) = node
            .name
            .as_deref()
            .and_then(|name| name.strip_prefix("Fixture report: "))
        {
            combined[name] = serde_json::from_str(node.value.as_deref().unwrap()).unwrap();
        }
    }
    combined
}

#[test]
fn widget_batch_keeps_native_and_semantic_operations_in_one_settle() {
    let (s, b, p, probe) = mock_widgets();
    let snapshot = observe_widgets(&s, &b, &p);
    let receipt = s
        .batch(
            &b,
            &p,
            &[
                BatchOperation::SetValue {
                    element_id: element(&snapshot, "Native"),
                    value: "replacement".into(),
                },
                BatchOperation::SelectChoice {
                    element_id: element(&snapshot, "Choice"),
                    choice_path: vec!["Sichuan".into(), "Chengdu".into()],
                },
                BatchOperation::SetDate {
                    element_id: element(&snapshot, "Date"),
                    value: "2028-02-29".into(),
                },
            ],
        )
        .unwrap();
    assert_eq!(receipt.execution_state, ExecutionState::Completed);
    assert_eq!(receipt.completed_count, 3);
    assert_eq!(receipt.remaining_count, 0);
    assert!(receipt.error.is_none(), "{:?}", receipt.error);
    assert_eq!(probe.lock().unwrap().waits, 1);
}

#[test]
fn widget_batch_rejects_invalid_path_and_calendar_date_before_dispatch() {
    let (s, b, p, probe) = mock_widgets();
    let snapshot = observe_widgets(&s, &b, &p);
    let native = element(&snapshot, "Native");
    let choice = element(&snapshot, "Choice");
    let date = element(&snapshot, "Date");
    let mut invalid = vec![
        BatchOperation::SelectChoice {
            element_id: choice.clone(),
            choice_path: vec![],
        },
        BatchOperation::SelectChoice {
            element_id: choice.clone(),
            choice_path: vec!["x".into(); 5],
        },
        BatchOperation::SelectChoice {
            element_id: choice.clone(),
            choice_path: vec![" ".into()],
        },
        BatchOperation::SelectChoice {
            element_id: choice,
            choice_path: vec!["a\0b".into()],
        },
    ];
    for value in [
        "2027-02-29",
        "1900-02-29",
        "2027-04-31",
        "2027-00",
        "0000-06",
        "2027/06/30",
        "2027-06-30T00:00",
    ] {
        invalid.push(BatchOperation::SetDate {
            element_id: date.clone(),
            value: value.into(),
        });
    }
    for operation in invalid {
        let receipt = s.batch(&b, &p, &[operation]).unwrap();
        assert_eq!(receipt.completed_count, 0);
        assert_eq!(receipt.remaining_count, 1);
        assert_eq!(
            receipt.error.unwrap().execution_state,
            ExecutionState::NotStarted
        );
    }
    let mixed = s
        .batch(
            &b,
            &p,
            &[
                BatchOperation::SetValue {
                    element_id: native,
                    value: "must-not-run".into(),
                },
                BatchOperation::SetDate {
                    element_id: date,
                    value: "2027-02-29".into(),
                },
            ],
        )
        .unwrap();
    assert_eq!(mixed.execution_state, ExecutionState::NotStarted);
    assert_eq!(mixed.completed_count, 0);
    assert_eq!(mixed.remaining_count, 2);
    assert_eq!(mixed.stopped_at_index, Some(1));
    assert_eq!(
        mixed.error.unwrap().execution_state,
        ExecutionState::NotStarted
    );
    let probe = probe.lock().unwrap();
    assert!(
        probe.effects.is_empty(),
        "invalid suffix must reject before native prefix effects"
    );
    assert_eq!(probe.waits, 0);
}

#[test]
fn widget_operations_require_current_and_action_specific_authority() {
    let (s, b, p, probe) = mock_widgets();
    let previous = observe_widgets(&s, &b, &p);
    let current = observe_widgets(&s, &b, &p);
    let stale = s
        .select_choice(&b, &p, &element(&previous, "Choice"), &["Master".into()])
        .unwrap_err();
    assert_eq!(stale.kind, "stale_element");
    assert_eq!(stale.execution_state, ExecutionState::NotStarted);
    let wrong_action = s
        .set_date(&b, &p, &element(&current, "Choice"), "2027-06-30")
        .unwrap_err();
    assert_eq!(wrong_action.execution_state, ExecutionState::NotStarted);
    let native = s
        .select_choice(&b, &p, &element(&current, "Native"), &["Master".into()])
        .unwrap_err();
    assert_eq!(native.execution_state, ExecutionState::NotStarted);
    assert_eq!(probe.lock().unwrap().waits, 0);
}

pub(super) fn with_widget_page(setup: &str, run: impl FnOnce(&BrowserSupervisor, &str, &str)) {
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Owned widget fixture</title>\
         <style>body{{font:16px sans-serif}}input,button,select{{margin:4px}}[hidden]{{display:none!important}}</style>\
         <main id=form></main><input aria-label='Fixture report' id=report readonly>\
         <script>{WIDGET_FIXTURE_JS}\n{setup}</script>"
    );
    let form = FormPage::serve_html(html);
    let supervisor = BrowserSupervisor::new();
    let _shutdown = ShutdownOnDrop(&supervisor);
    let browser = supervisor.launch().unwrap().browser_id;
    let page = supervisor.pages(&browser, 1).unwrap().remove(0).page_id;
    supervisor
        .navigate(&browser, &page, &format!("http://127.0.0.1:{}/", form.port))
        .unwrap();
    run(&supervisor, &browser, &page);
}

#[test]
#[ignore = "requires local Chromium; deterministic synthetic controls in an isolated owned profile"]
fn chromium_widgets_custom_choice_path_and_existing_selection() {
    with_widget_page(
        r#"
        addChoice("Degree", [["  Master   degree  ", "Bachelor"]], {unrelated:true});
        addChoice("Location", [["Sichuan", "Guangdong"], ["Chengdu", "Mianyang"]]);
        addChoice("Existing", [["Master", "Bachelor"]], {initial:"Master"});
    "#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            s.select_choice(
                b,
                p,
                &element(&snapshot, "Degree"),
                &["Master degree".into()],
            )
            .unwrap_or_else(|error| {
                let readback = observe_widgets(s, b, p);
                panic!(
                    "owned popup selection failed: {error:?}; report={}",
                    report(&readback)
                );
            });
            s.select_choice(
                b,
                p,
                &element(&snapshot, "Location"),
                &["Sichuan".into(), "Chengdu".into()],
            )
            .unwrap();
            s.select_choice(b, p, &element(&snapshot, "Existing"), &["Master".into()])
                .unwrap();
            let readback = observe_widgets(s, b, p);
            assert_eq!(field_value(&readback, "Degree"), "Master degree");
            assert_eq!(field_value(&readback, "Location"), "Sichuan / Chengdu");
            let state = report(&readback);
            assert_eq!(state["Degree"]["opens"], 1);
            assert_eq!(state["Degree"]["choices"], 1);
            assert_eq!(state["Location"]["choices"], 2);
            assert_eq!(state["Existing"]["opens"], 0);
            assert_eq!(state["Existing"]["choices"], 0);
            assert_eq!(state["unrelated_choices"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; deterministic synthetic controls in an isolated owned profile"]
fn chromium_widgets_choice_failures_preserve_field_or_report_partial_effect() {
    for (name, setup, path, expected_kind, expected_state, choices) in [
        (
            "Missing",
            r#"addChoice("Missing", [["Bachelor"]]);"#,
            vec!["Master"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            0,
        ),
        (
            "Duplicate",
            r#"addChoice("Duplicate", [["Master", "Master"]]);"#,
            vec!["Master"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            0,
        ),
        (
            "Disabled",
            r#"addChoice("Disabled", [["Master"]], {disabled:true});"#,
            vec!["Master"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            0,
        ),
        (
            "Partial",
            r#"addChoice("Partial", [["Sichuan"], ["Mianyang"]]);"#,
            vec!["Sichuan", "Chengdu"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            1,
        ),
        (
            "Rerender",
            r#"addChoice("Rerender", [["Master"]], {rerender:true});"#,
            vec!["Master"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            1,
        ),
        (
            "Rejected",
            r#"addChoice("Rejected", [["Master"]], {reject:true});"#,
            vec!["Master"],
            "form_control_outcome_unknown",
            ExecutionState::OutcomeUnknown,
            1,
        ),
    ] {
        with_widget_page(setup, |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let path = path.into_iter().map(String::from).collect::<Vec<_>>();
            let error = s
                .select_choice(b, p, &element(&snapshot, name), &path)
                .unwrap_err();
            assert_eq!(error.kind, expected_kind, "{name}: {error:?}");
            assert_eq!(error.execution_state, expected_state, "{name}: {error:?}");
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state[name]["choices"], choices, "{name}: {state}");
            if choices == 0 {
                assert_eq!(field_value(&readback, name), "");
            }
            assert_eq!(error.recovery_action, Some("snapshot"));
            if name == "Rerender" {
                assert_eq!(field_value(&readback, name), "Master");
            }
        });
    }
}

#[test]
#[ignore = "requires local Chromium; deterministic synthetic controls in an isolated owned profile"]
fn chromium_widgets_date_native_setter_month_and_dialog_fallback() {
    with_widget_page(
        r#"
        addDate("Fast date", {fast:true});
        addDate("Fast month", {fast:true});
        addDate("Dialog date");
        addDate("Existing date", {initial:"2027-06-30"});
    "#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            s.set_date(b, p, &element(&snapshot, "Fast date"), "2028-02-29")
                .unwrap();
            s.set_date(b, p, &element(&snapshot, "Fast month"), "2027-06")
                .unwrap();
            s.set_date(b, p, &element(&snapshot, "Dialog date"), "2027-06-30")
                .unwrap();
            s.set_date(b, p, &element(&snapshot, "Existing date"), "2027-06-30")
                .unwrap();
            let readback = observe_widgets(s, b, p);
            assert_eq!(field_value(&readback, "Fast date"), "2028-02-29");
            assert_eq!(field_value(&readback, "Fast month"), "2027-06");
            assert_eq!(field_value(&readback, "Dialog date"), "2027-06-30");
            let state = report(&readback);
            assert_eq!(state["Fast date"]["opens"], 0);
            assert_eq!(state["Fast date"]["changes"], 1);
            assert_eq!(state["Fast month"]["opens"], 0);
            assert_eq!(state["Dialog date"]["opens"], 1);
            assert_eq!(state["Dialog date"]["choices"], 1);
            assert_eq!(state["Existing date"]["opens"], 0);
            assert_eq!(state["Existing date"]["choices"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; deterministic synthetic controls in an isolated owned profile"]
fn chromium_widgets_date_missing_disabled_duplicate_and_rerender() {
    for (options, choices) in [
        (r#"{missing:true}"#, 0),
        (r#"{disabled:true}"#, 0),
        (r#"{duplicate:true}"#, 0),
        (r#"{rerender:true}"#, 1),
        (r#"{reject:true}"#, 1),
    ] {
        let setup = format!(r#"addDate("Date", {options});"#);
        with_widget_page(&setup, |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let error = s
                .set_date(b, p, &element(&snapshot, "Date"), "2027-06-30")
                .unwrap_err();
            assert_eq!(
                error.execution_state,
                ExecutionState::OutcomeUnknown,
                "{options}: {error:?}"
            );
            assert_eq!(error.kind, "form_control_outcome_unknown");
            assert_eq!(error.recovery_action, Some("snapshot"));
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state["Date"]["choices"], choices);
            if choices == 0 {
                assert_eq!(field_value(&readback, "Date"), "");
            }
        });
    }
}
#[test]
#[ignore = "requires local Chromium; deterministic synthetic controls in an isolated owned profile"]
fn chromium_widgets_stale_choice_and_date_never_open_popup() {
    with_widget_page(
        r#"addChoice("Choice", [["Master"]]); addDate("Date");"#,
        |s, b, p| {
            let previous = observe_widgets(s, b, p);
            let _current = observe_widgets(s, b, p);
            for error in [
                s.select_choice(b, p, &element(&previous, "Choice"), &["Master".into()])
                    .unwrap_err(),
                s.set_date(b, p, &element(&previous, "Date"), "2027-06-30")
                    .unwrap_err(),
            ] {
                assert_eq!(error.kind, "stale_element");
                assert_eq!(error.execution_state, ExecutionState::NotStarted);
            }
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state["Choice"]["opens"], 0);
            assert_eq!(state["Date"]["opens"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; hierarchy discovery must advance beyond the previous level"]
fn chromium_widgets_missing_child_cannot_choose_an_old_sibling() {
    with_widget_page(
        r#"addChoice("Location", [["Sichuan","Chengdu"],["Unused"]], {noChild:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let error = s
                .select_choice(
                    b,
                    p,
                    &element(&snapshot, "Location"),
                    &["Sichuan".into(), "Chengdu".into()],
                )
                .unwrap_err();
            assert_eq!(error.execution_state, ExecutionState::OutcomeUnknown);
            let readback = observe_widgets(s, b, p);
            assert_eq!(field_value(&readback, "Location"), "Sichuan");
            assert_eq!(report(&readback)["Location"]["choices"], 1);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; search text is not a committed custom selection"]
fn chromium_widgets_editable_search_text_does_not_count_as_selected() {
    with_widget_page(
        r#"addChoice("Search", [["Master","Bachelor"]], {editable:true,display:"Master"});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            assert_eq!(field_value(&snapshot, "Search"), "Master");
            let search = snapshot
                .nodes
                .iter()
                .find(|node| node.name.as_deref() == Some("Search"))
                .unwrap();
            assert_eq!(search.form_context.as_ref().unwrap().dom_tag, "input");
            assert_ne!(search.read_only, Some(true));
            s.select_choice(b, p, &element(&snapshot, "Search"), &["Master".into()])
                .unwrap();
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state["Search"]["opens"], 1);
            assert_eq!(state["Search"]["choices"], 1);
            assert_eq!(state["Search"]["value"], "Master");
        },
    );
}

#[test]
#[ignore = "requires local Chromium; day cells must not be reinterpreted as month choices"]
fn chromium_widgets_unlabelled_day_grid_is_not_a_month_panel() {
    with_widget_page(r#"addUnlabelledCalendar("Date");"#, |s, b, p| {
        let snapshot = observe_widgets(s, b, p);
        let error = s
            .set_date(b, p, &element(&snapshot, "Date"), "2027-03-03")
            .unwrap_err();
        assert_eq!(
            error.execution_state,
            ExecutionState::OutcomeUnknown,
            "{error:?}"
        );
        let readback = observe_widgets(s, b, p);
        assert_eq!(field_value(&readback, "Date"), "");
        assert_eq!(report(&readback)["Date"]["choices"], 0);
    });
}

#[test]
#[ignore = "requires local Chromium; mousedown rerender must stop the remaining event sequence"]
fn chromium_widgets_mousedown_rerender_never_clicks_the_detached_option() {
    with_widget_page(
        r#"addChoice("Degree", [["Master"]], {detachOnDown:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let error = s
                .select_choice(b, p, &element(&snapshot, "Degree"), &["Master".into()])
                .unwrap_err();
            assert_eq!(
                error.execution_state,
                ExecutionState::OutcomeUnknown,
                "{error:?}"
            );
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state["Degree"]["mousedowns"], 1);
            assert_eq!(state["Degree"]["detachedMouseups"], 0);
            assert_eq!(state["Degree"]["detachedClicks"], 0);
            assert_eq!(state["Degree"]["choices"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; virtual ARIA nodes are not the visible option surface"]
fn chromium_widgets_virtual_aria_popup_uses_visible_generic_options() {
    with_widget_page(
        r#"addChoice("Degree", [["Master degree","Bachelor"]], {virtualA11y:true,unrelated:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            s.select_choice(
                b,
                p,
                &element(&snapshot, "Degree"),
                &["Master degree".into()],
            )
            .unwrap();
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(field_value(&readback, "Degree"), "Master degree");
            assert_eq!(state["Degree"]["choices"], 1);
            assert_eq!(state["unrelated_choices"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; verified native backing select should avoid opening a popup"]
fn chromium_widgets_native_backing_select_finishes_without_opening_popup() {
    with_widget_page(
        r#"addBackedChoice("Degree"); addBackedChoice("Existing", {initial:"Master", noMirror:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            s.select_choice(b, p, &element(&snapshot, "Degree"), &["Master".into()])
                .unwrap();
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(state["Degree"]["value"], "Master");
            assert_eq!(state["Degree"]["changes"], 1);
            assert_eq!(state["Degree"]["opens"], 0);

            // The trigger intentionally does not mirror the backing select, so
            // the widget fast path must inspect the already-selected native option
            // and avoid emitting redundant input/change events.
            s.select_choice(b, p, &element(&readback, "Existing"), &["Master".into()])
                .unwrap();
            let final_readback = observe_widgets(s, b, p);
            let state = report(&final_readback);
            assert_eq!(state["Existing"]["value"], "Master");
            assert_eq!(state["Existing"]["changes"], 0);
            assert_eq!(state["Existing"]["opens"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; hierarchical values retain their segment boundaries"]
fn chromium_widgets_regression_choice_path_segments_cannot_collide() {
    with_widget_page(
        r#"addChoice("Location", [["A"],["BC"]], {initial:"AB / C"});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            s.select_choice(
                b,
                p,
                &element(&snapshot, "Location"),
                &["A".into(), "BC".into()],
            )
            .unwrap();
            let readback = observe_widgets(s, b, p);
            assert_eq!(field_value(&readback, "Location"), "A / BC");
            assert_eq!(report(&readback)["Location"]["choices"], 2);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; real button calendar panels and deferred decade navigation"]
fn chromium_widgets_regression_button_calendar_navigates_year_month_day() {
    with_widget_page(r#"addButtonCalendar("Date");"#, |s, b, p| {
        let snapshot = observe_widgets(s, b, p);
        s.set_date(b, p, &element(&snapshot, "Date"), "2000-06-30")
            .unwrap_or_else(|error| {
                let readback = observe_widgets(s, b, p);
                panic!(
                    "button calendar failed: {error:?}; report={}",
                    report(&readback)
                );
            });
        let readback = observe_widgets(s, b, p);
        assert_eq!(field_value(&readback, "Date"), "2000-06-30");
        let state = report(&readback);
        assert_eq!(state["Date"]["opens"], 1);
        assert_eq!(state["Date"]["year_selections"], 1);
        assert_eq!(state["Date"]["month_selections"], 1);
        assert_eq!(state["Date"]["decade_moves"], 2);
        assert_eq!(state["Date"]["choices"], 1);
    });
}

#[test]
#[ignore = "requires local Chromium; an expanded trigger must retain explicit popup scope"]
fn chromium_widgets_regression_expanded_hidden_popup_rejects_unrelated_choices() {
    with_widget_page(
        r#"addChoice("Degree", [["Master degree"]], {expandedBefore:true,noPopup:true,unrelated:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let error = s
                .select_choice(
                    b,
                    p,
                    &element(&snapshot, "Degree"),
                    &["Master degree".into()],
                )
                .unwrap_err();
            assert_eq!(
                error.execution_state,
                ExecutionState::NotStarted,
                "{error:?}"
            );
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            assert_eq!(field_value(&readback, "Degree"), "");
            assert_eq!(state["Degree"]["opens"], 0);
            assert_eq!(state["Degree"]["choices"], 0);
            assert_eq!(state["unrelated_choices"], 0);
        },
    );
}

#[test]
#[ignore = "requires local Chromium; semantic option clicks must suppress native submit/reset defaults"]
fn chromium_widgets_regression_option_buttons_never_submit_or_reset_forms() {
    with_widget_page(
        r#"
        addChoice("Submit option", [["Master"]], {defaultAction:"implicit_submit",captureStop:true});
        addChoice("Reset option", [["Master"]], {defaultAction:"reset",captureStop:true});
        "#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            for name in ["Submit option", "Reset option"] {
                s.select_choice(b, p, &element(&snapshot, name), &["Master".into()])
                    .unwrap();
            }
            let readback = observe_widgets(s, b, p);
            let state = report(&readback);
            for name in ["Submit option", "Reset option"] {
                assert_eq!(field_value(&readback, name), "Master");
                assert_eq!(state[name]["choices"], 1);
                assert_eq!(state[name]["form_submits"], 0, "{name}");
                assert_eq!(state[name]["form_resets"], 0, "{name}");
            }
        },
    );
}

#[test]
#[ignore = "requires local Chromium; already-expanded picker effects retain uncertain execution"]
fn chromium_widgets_review_already_expanded_date_effects_are_unknown() {
    with_widget_page(
        r#"
        addDate("Committed navigation", {expandedBefore:true,commitNavigation:true,missing:true});
        addDate("Detached navigation", {expandedBefore:true,detachOnNavigation:true,missing:true});
        addDate("Disabled year", {expandedBefore:true,disabledPart:"year",initialMonth:"06",missing:true});
        addDate("Disabled month", {expandedBefore:true,disabledPart:"month",initialYear:"2027",missing:true});
        addDate("Readonly year", {expandedBefore:true,yearInput:true,readOnlyYear:true,initialMonth:"06",missing:true});
        addDate("Aria readonly year", {expandedBefore:true,yearInput:true,ariaReadOnlyYear:true,initialMonth:"06",missing:true});
        "#,
        |s, b, p| {
            for name in ["Committed navigation", "Detached navigation"] {
                let snapshot = observe_widgets(s, b, p);
                let error = s
                    .set_date(b, p, &element(&snapshot, name), "2027-06-30")
                    .unwrap_err();
                assert_eq!(
                    error.execution_state,
                    ExecutionState::OutcomeUnknown,
                    "{name}: {error:?}"
                );
                assert_eq!(error.kind, "form_control_outcome_unknown");
                assert_eq!(error.recovery_action, Some("snapshot"));
                let readback = observe_widgets(s, b, p);
                let state = report(&readback);
                assert_eq!(state[name]["opens"], 0);
                assert_eq!(state[name]["choices"], 0);
                if name == "Committed navigation" {
                    assert_eq!(field_value(&readback, name), "2027-06-01");
                    assert_eq!(state[name]["changes"], 2);
                }
            }
            for name in [
                "Disabled year",
                "Disabled month",
                "Readonly year",
                "Aria readonly year",
            ] {
                let snapshot = observe_widgets(s, b, p);
                let error = s
                    .set_date(b, p, &element(&snapshot, name), "2027-06-30")
                    .unwrap_err();
                assert_eq!(
                    error.execution_state,
                    ExecutionState::NotStarted,
                    "{name}: {error:?}"
                );
                let readback = observe_widgets(s, b, p);
                let state = report(&readback);
                assert_eq!(field_value(&readback, name), "");
                assert_eq!(state[name]["opens"], 0);
                assert_eq!(state[name]["choices"], 0);
                assert_eq!(state[name]["navigation_events"], 0, "{name}");
            }
        },
    );
}

#[test]
#[ignore = "requires local Chromium; hidden child association must not widen to its visible parent popup"]
fn chromium_widgets_review_hidden_linked_child_rejects_parent_sibling() {
    with_widget_page(
        r#"addChoice("Location", [["Sichuan","Chengdu"],["Unused"]], {noChild:true,linkedHiddenChild:true});"#,
        |s, b, p| {
            let snapshot = observe_widgets(s, b, p);
            let error = s
                .select_choice(
                    b,
                    p,
                    &element(&snapshot, "Location"),
                    &["Sichuan".into(), "Chengdu".into()],
                )
                .unwrap_err();
            assert_eq!(
                error.execution_state,
                ExecutionState::OutcomeUnknown,
                "{error:?}"
            );
            assert_eq!(error.recovery_action, Some("snapshot"));
            let readback = observe_widgets(s, b, p);
            assert_eq!(field_value(&readback, "Location"), "Sichuan");
            assert_eq!(report(&readback)["Location"]["choices"], 1);
        },
    );
}

#[derive(Default, serde::Serialize)]
struct CallBudget {
    browser_calls: usize,
    snapshot_calls: usize,
    primitive_operations: usize,
    screenshots: usize,
    pointer_operations: usize,
    elapsed_ms: u128,
}

fn budget_snapshot(
    s: &BrowserSupervisor,
    b: &str,
    p: &str,
    budget: &mut CallBudget,
) -> SemanticSnapshot {
    budget.browser_calls += 1;
    budget.snapshot_calls += 1;
    observe_widgets(s, b, p)
}

fn native_operations(snapshot: &SemanticSnapshot, resume: &std::path::Path) -> Vec<BatchOperation> {
    let mut operations = (0..20)
        .map(|i| BatchOperation::SetValue {
            element_id: element(snapshot, &format!("Native {i}")),
            value: format!("Fictional value {i}"),
        })
        .collect::<Vec<_>>();
    operations.push(BatchOperation::UploadFile {
        element_id: element(snapshot, "Resume"),
        path: resume.to_path_buf(),
    });
    operations
}

fn assert_typical_form(snapshot: &SemanticSnapshot) {
    for i in 0..20 {
        assert_eq!(
            field_value(snapshot, &format!("Native {i}")),
            format!("Fictional value {i}")
        );
    }
    for name in ["Degree", "Employment", "Language"] {
        assert_eq!(field_value(snapshot, name), "Master");
    }
    assert_eq!(field_value(snapshot, "Location"), "Sichuan / Chengdu");
    assert_eq!(field_value(snapshot, "Graduation"), "2027-06-30");
    assert_eq!(report(snapshot)["Resume"]["value"], "fictional-resume.pdf");
}

fn legacy_fill(s: &BrowserSupervisor, b: &str, p: &str, resume: &std::path::Path) -> CallBudget {
    let started = Instant::now();
    let mut budget = CallBudget::default();
    let mut snapshot = budget_snapshot(s, b, p, &mut budget);
    let native = native_operations(&snapshot, resume);
    let receipt = s.batch(b, p, &native).unwrap();
    assert_eq!(receipt.completed_count, native.len());
    budget.browser_calls += 1;
    budget.primitive_operations += native.len();
    for name in ["Degree", "Employment", "Language"] {
        s.click(b, p, &element(&snapshot, name)).unwrap();
        budget.browser_calls += 1;
        budget.primitive_operations += 1;
        snapshot = budget_snapshot(s, b, p, &mut budget);
        s.click(b, p, &element(&snapshot, "Master")).unwrap();
        budget.browser_calls += 1;
        budget.primitive_operations += 1;
        snapshot = budget_snapshot(s, b, p, &mut budget);
    }
    s.click(b, p, &element(&snapshot, "Location")).unwrap();
    budget.browser_calls += 1;
    budget.primitive_operations += 1;
    snapshot = budget_snapshot(s, b, p, &mut budget);
    for choice in ["Sichuan", "Chengdu"] {
        s.click(b, p, &element(&snapshot, choice)).unwrap();
        budget.browser_calls += 1;
        budget.primitive_operations += 1;
        snapshot = budget_snapshot(s, b, p, &mut budget);
    }
    s.click(b, p, &element(&snapshot, "Graduation")).unwrap();
    budget.browser_calls += 1;
    budget.primitive_operations += 1;
    snapshot = budget_snapshot(s, b, p, &mut budget);
    s.select_option(b, p, &element(&snapshot, "Year"), "2027")
        .unwrap();
    budget.browser_calls += 1;
    budget.primitive_operations += 1;
    s.select_option(b, p, &element(&snapshot, "Month"), "06")
        .unwrap();
    budget.browser_calls += 1;
    budget.primitive_operations += 1;
    snapshot = budget_snapshot(s, b, p, &mut budget);
    s.click(b, p, &element(&snapshot, "2027-06-30")).unwrap();
    budget.browser_calls += 1;
    budget.primitive_operations += 1;
    snapshot = budget_snapshot(s, b, p, &mut budget);
    budget.elapsed_ms = started.elapsed().as_millis();
    assert_typical_form(&snapshot);
    budget
}

#[test]
#[ignore = "requires local Chromium; measures synthetic 26-field legacy and high-level workflows"]
fn chromium_widgets_typical_form_call_budget_without_screenshot_or_pointer() {
    let files = tempfile::tempdir().unwrap();
    let resume = files.path().join("fictional-resume.pdf");
    std::fs::write(
        &resume,
        b"%PDF-1.0\n% synthetic owned fixture, no personal data\n%%EOF\n",
    )
    .unwrap();
    let setup = r#"
        for (let i=0;i<20;i++) addNative("Native "+i);
        for (const name of ["Degree","Employment","Language"]) addChoice(name, [["Master","Bachelor"]]);
        addChoice("Location", [["Sichuan","Guangdong"],["Chengdu","Mianyang"]]);
        addDate("Graduation"); addUpload("Resume");
    "#;
    let mut legacy = None;
    with_widget_page(setup, |s, b, p| {
        legacy = Some(legacy_fill(s, b, p, &resume));
    });
    let mut semantic = None;
    with_widget_page(setup, |s, b, p| {
        let started = Instant::now();
        let mut budget = CallBudget::default();
        let snapshot = budget_snapshot(s, b, p, &mut budget);
        let mut operations = native_operations(&snapshot, &resume);
        for name in ["Degree", "Employment", "Language"] {
            operations.push(BatchOperation::SelectChoice {
                element_id: element(&snapshot, name),
                choice_path: vec!["Master".into()],
            });
        }
        operations.push(BatchOperation::SelectChoice {
            element_id: element(&snapshot, "Location"),
            choice_path: vec!["Sichuan".into(), "Chengdu".into()],
        });
        operations.push(BatchOperation::SetDate {
            element_id: element(&snapshot, "Graduation"),
            value: "2027-06-30".into(),
        });
        assert_eq!(operations.len(), 26);
        let receipt = s.batch(b, p, &operations).unwrap();
        budget.browser_calls += 1;
        budget.primitive_operations += operations.len();
        assert_eq!(receipt.completed_count, 26, "{receipt:?}");
        assert_eq!(receipt.remaining_count, 0);
        assert!(receipt.error.is_none(), "{receipt:?}");
        assert!(
            !receipt.needs_snapshot,
            "sibling authority should survive popup-only rerenders"
        );
        let readback = budget_snapshot(s, b, p, &mut budget);
        budget.elapsed_ms = started.elapsed().as_millis();
        assert_typical_form(&readback);
        assert_eq!(budget.browser_calls, 3);
        assert_eq!(budget.snapshot_calls, 2);
        semantic = Some(budget);
    });
    let legacy = legacy.unwrap();
    let semantic = semantic.unwrap();
    assert!(legacy.browser_calls > semantic.browser_calls);
    println!(
        "WIDGET_CALL_BUDGET {}",
        serde_json::json!({
            "fields":26,"native_fields":20,"custom_widgets":5,"uploads":1,
            "legacy_browser_workflow":legacy,"semantic_browser_workflow":semantic,
            "model_interactions":"not measured by Rust fixture; report orchestration estimate separately",
            "timing_scope":"initial semantic observation through final readback; excludes Chromium launch/navigation",
            "comparison":"same synthetic form; legacy uses existing click/select_option/batch APIs"
        })
    );
}

const WIDGET_FIXTURE_JS: &str = r#"
const form=document.getElementById('form'), states={unrelated_choices:0};
const nativeValue=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');
const monitors=new Map();
function publish(){
  document.getElementById('report').value=JSON.stringify({unrelated_choices:states.unrelated_choices});
  // Each monitor stays below the existing 512-character semantic value bound.
  for(const [name,state] of Object.entries(states)){
    if(typeof state!=='object')continue;
    let monitor=monitors.get(name);
    if(!monitor){
      monitor=document.createElement('input');monitor.readOnly=true;
      monitor.setAttribute('aria-label','Fixture report: '+name);document.body.append(monitor);monitors.set(name,monitor);
    }
    monitor.value=JSON.stringify(state);
  }
}
function labelInput(name, readOnly){
  const field=document.createElement('div'), label=document.createElement('label');
  label.textContent=name;
  const input=document.createElement('input');
  input.setAttribute('aria-label',name);input.readOnly=readOnly;
  label.append(input);field.append(label);form.append(field);return input;
}
function normalized(v){return String(v).trim().replace(/\s+/g,' ');}
function addNative(name){
  const input=labelInput(name,false);input.value='old value';
  let tracked=input.value;
  Object.defineProperty(input,'value',{get(){return nativeValue.get.call(this);},set(v){tracked=v;nativeValue.set.call(this,v);}});
  input.addEventListener('input',()=>{
    if(tracked===input.value)nativeValue.set.call(input,'tracker missed update');
    tracked=input.value;
  });
}
function addUpload(name){
  const input=labelInput(name,false);input.type='file';
  states[name]={value:'',changes:0};
  input.addEventListener('change',()=>{states[name].value=input.files[0]?.name||'';states[name].changes++;publish();});
  publish();
}
function addChoice(name,layers,options={}){
  let input=labelInput(name,!options.editable);
  const state=states[name]={value:options.initial||'',opens:0,choices:0,changes:0,expanded:!!options.expandedBefore,detachedMouseups:0,detachedClicks:0,mousedowns:0};
  let defaultForm;
  if(options.defaultAction){
    state.form_submits=0;state.form_resets=0;
    defaultForm=document.createElement('form');defaultForm.id='default-form-'+Object.keys(states).length;document.body.append(defaultForm);
    for(const eventName of ['submit','reset'])defaultForm.addEventListener(eventName,event=>{
      event.preventDefault();state[eventName==='submit'?'form_submits':'form_resets']++;publish();
    });
  }
  input.value=options.display||state.value;input.setAttribute('role','combobox');input.setAttribute('aria-haspopup','listbox');
  input.setAttribute('aria-expanded',String(state.expanded));
  const popup=document.createElement('div');popup.id='choices-'+Object.keys(states).length;
  if(options.virtualA11y)popup.className='generic-select-dropdown';else popup.setAttribute('role','listbox');
  popup.hidden=true;document.body.append(popup);
  input.setAttribute('aria-controls',popup.id);
  if(options.virtualA11y){
    const virtual=document.createElement('div');virtual.id=popup.id+'-virtual';virtual.setAttribute('role','listbox');
    virtual.style.cssText='position:absolute;width:0;height:0;overflow:hidden';document.body.append(virtual);
    input.setAttribute('aria-controls',virtual.id);
  }
  const close=()=>{popup.hidden=true;input.setAttribute('aria-expanded','false');state.expanded=false;publish();};
  // A read-only framework combobox rejects arbitrary writes to its display input.
  input.addEventListener('input',()=>{nativeValue.set.call(input,state.value);});
  const render=(depth,path)=>{
    popup.replaceChildren();
    for(const text of layers[depth]){
      const option=document.createElement(options.virtualA11y?'div':'button');option.type='button';
      if(defaultForm){
        option.setAttribute('form',defaultForm.id);
        if(options.defaultAction==='implicit_submit')option.removeAttribute('type');else option.type='reset';
      }
      if(options.virtualA11y){
        option.className='generic-option';option.tabIndex=0;
        const content=document.createElement('span');content.className='generic-option-content';content.textContent=text;option.append(content);
      }else{option.setAttribute('role','option');option.textContent=text;}
      option.setAttribute('data-value',normalized(text));
      if(options.linkedHiddenChild&&depth===0)option.setAttribute('aria-controls',popup.id+'-child');
      if(options.disabled){option.disabled=true;option.setAttribute('aria-disabled','true');}
      if(options.detachOnDown)option.addEventListener('mousedown',()=>{
        state.mousedowns++;const replacement=input.cloneNode(true);input.replaceWith(replacement);input=replacement;
        option.replaceWith(option.cloneNode(true));publish();
      });
      option.addEventListener('mouseup',()=>{if(!option.isConnected){state.detachedMouseups++;publish();}});
      option.addEventListener('click',event=>{
        if(options.captureStop)event.stopImmediatePropagation();
        if(!option.isConnected){state.detachedClicks++;publish();return;}
        state.choices++;
        const selected=path.concat(normalized(text));
        if(depth+1<layers.length){
          state.value=selected.join(' / ');nativeValue.set.call(input,state.value);publish();
          if(options.noChild)return;
          setTimeout(()=>render(depth+1,selected),15);return;
        }
        if(!options.reject){
          state.value=selected.join(' / ');nativeValue.set.call(input,state.value);
          input.dispatchEvent(new Event('input',{bubbles:true}));
          input.dispatchEvent(new Event('change',{bubbles:true}));state.changes++;
        }
        if(options.rerender){const replacement=input.cloneNode(true);replacement.value=state.value;input.replaceWith(replacement);input=replacement;}
        close();
      },!!options.captureStop);
      popup.append(option);
    }
    if(options.linkedHiddenChild&&depth===0){
      const child=document.createElement('div');child.id=popup.id+'-child';child.setAttribute('role','listbox');child.hidden=true;popup.append(child);
    }
  };
  input.addEventListener('click',()=>{
    state.opens++;state.expanded=true;input.setAttribute('aria-expanded','true');publish();
    if(!options.noPopup)setTimeout(()=>{render(0,[]);popup.hidden=false;},15);
  });
  document.addEventListener('keydown',event=>{if(event.key==='Escape')close();});
  if(options.unrelated){
    const unrelated=document.createElement('div');unrelated.setAttribute('role','listbox');
    const duplicate=document.createElement('button');duplicate.setAttribute('role','option');
    duplicate.textContent='Master degree';duplicate.addEventListener('click',()=>{states.unrelated_choices++;publish();});
    unrelated.append(duplicate);document.body.append(unrelated);
  }
  publish();
}
function addBackedChoice(name,options={}){
  const wrapper=document.createElement('label');wrapper.textContent=name;form.append(wrapper);
  const trigger=document.createElement('button');trigger.type='button';trigger.textContent='Choose';
  trigger.setAttribute('role','combobox');trigger.setAttribute('aria-label',name);trigger.setAttribute('aria-haspopup','listbox');
  trigger.setAttribute('aria-expanded','false');
  const backing=document.createElement('select');backing.id='backing-'+Object.keys(states).length;backing.hidden=true;
  for(const value of ['','Master','Bachelor']){const option=document.createElement('option');option.value=value;option.textContent=value;backing.append(option);}
  wrapper.append(trigger,backing);trigger.setAttribute('aria-controls',backing.id);
  const state=states[name]={value:options.initial||'',opens:0,changes:0};backing.value=state.value;
  backing.addEventListener('change',()=>{state.value=backing.value;state.changes++;if(!options.noMirror){trigger.value=state.value;trigger.textContent=state.value;trigger.setAttribute('aria-valuetext',state.value);}publish();});
  trigger.addEventListener('click',()=>{state.opens++;publish();});
  publish();
}
function addButtonCalendar(name){
  const input=labelInput(name,true),state=states[name]={value:'',opens:0,choices:0,expanded:false,year_selections:0,month_selections:0,decade_moves:0};
  input.setAttribute('role','combobox');input.setAttribute('aria-haspopup','dialog');input.setAttribute('aria-expanded','false');
  const dialog=document.createElement('div');dialog.id='button-calendar';dialog.setAttribute('role','dialog');dialog.hidden=true;document.body.append(dialog);
  input.setAttribute('aria-controls',dialog.id);
  let year=2026,month=10,decade=2020;
  const names=['January','February','March','April','May','June','July','August','September','October','November','December'];
  const later=draw=>setTimeout(draw,15);
  const button=(text,label,action)=>{
    const node=document.createElement('button');node.type='button';node.textContent=text;
    if(label)node.setAttribute('aria-label',label);node.addEventListener('click',action);return node;
  };
  const close=()=>{dialog.hidden=true;state.expanded=false;input.setAttribute('aria-expanded','false');publish();};
  const days=()=>{
    dialog.replaceChildren();
    dialog.append(button(String(year),'Choose year',()=>later(years)),button(names[month-1],'Choose month',()=>later(months)));
    const grid=document.createElement('div');grid.setAttribute('role','grid');grid.setAttribute('aria-label','Day');dialog.append(grid);
    const count=new Date(year,month,0).getDate();
    for(let day=1;day<=count;day++){
      const iso=String(year)+'-'+String(month).padStart(2,'0')+'-'+String(day).padStart(2,'0');
      const node=button(String(day),iso,()=>{state.value=iso;state.choices++;nativeValue.set.call(input,iso);close();});
      node.title=iso;grid.append(node);
    }
  };
  const years=()=>{
    dialog.replaceChildren();
    dialog.append(button('Previous decade','Previous decade',()=>{decade-=10;state.decade_moves++;publish();later(years);}));
    const heading=document.createElement('div');heading.setAttribute('role','heading');heading.textContent=String(decade)+' - '+String(decade+9);dialog.append(heading);
    const grid=document.createElement('div');grid.setAttribute('role','grid');grid.setAttribute('aria-label','Year');dialog.append(grid);
    for(let value=decade;value<decade+10;value++){
      grid.append(button(String(value),String(value),()=>{year=value;state.year_selections++;publish();later(days);}));
    }
  };
  const months=()=>{
    dialog.replaceChildren();
    const heading=document.createElement('div');heading.setAttribute('role','heading');heading.textContent=String(year);dialog.append(heading);
    const grid=document.createElement('div');grid.setAttribute('role','grid');grid.setAttribute('aria-label','Month');dialog.append(grid);
    names.forEach((name,index)=>grid.append(button(name,name,()=>{month=index+1;state.month_selections++;publish();later(days);})));
  };
  input.addEventListener('click',()=>{state.opens++;state.expanded=true;input.setAttribute('aria-expanded','true');publish();later(()=>{days();dialog.hidden=false;});});
  input.addEventListener('input',()=>nativeValue.set.call(input,state.value));
  document.addEventListener('keydown',event=>{if(event.key==='Escape')close();});
  publish();
}
function addUnlabelledCalendar(name){
  const input=labelInput(name,true),state=states[name]={value:'',opens:0,choices:0,expanded:false};
  input.setAttribute('role','combobox');input.setAttribute('aria-haspopup','dialog');input.setAttribute('aria-expanded','false');
  const dialog=document.createElement('div');dialog.id='unlabelled-date';dialog.hidden=true;dialog.setAttribute('role','dialog');
  input.setAttribute('aria-controls',dialog.id);document.body.append(dialog);
  const heading=document.createElement('div');heading.setAttribute('role','heading');heading.textContent='2027';dialog.append(heading);
  const grid=document.createElement('div');grid.setAttribute('role','grid');dialog.append(grid);
  for(let day=1;day<=31;day++){
    const button=document.createElement('button');button.textContent=String(day);
    button.addEventListener('click',()=>{state.choices++;publish();});grid.append(button);
  }
  input.addEventListener('click',()=>{state.opens++;state.expanded=true;input.setAttribute('aria-expanded','true');dialog.hidden=false;publish();});
  document.addEventListener('keydown',event=>{if(event.key==='Escape'){dialog.hidden=true;state.expanded=false;input.setAttribute('aria-expanded','false');publish();}});
  publish();
}
function addDate(name,options={}){
  let input=labelInput(name,!options.fast);
  const state=states[name]={value:options.initial||'',opens:0,choices:0,changes:0,navigation_events:0,expanded:!!options.expandedBefore};
  input.value=state.value;input.setAttribute('role','combobox');input.setAttribute('aria-haspopup','dialog');
  input.setAttribute('aria-expanded',String(state.expanded));
  const dialog=document.createElement('div');dialog.id='date-'+Object.keys(states).length;
  dialog.setAttribute('role','dialog');dialog.setAttribute('aria-label','Date picker');dialog.hidden=true;
  document.body.append(dialog);input.setAttribute('aria-controls',dialog.id);
  const close=()=>{dialog.hidden=true;input.setAttribute('aria-expanded','false');state.expanded=false;publish();};
  if(options.fast){
    let tracked=input.value;
    Object.defineProperty(input,'value',{get(){return nativeValue.get.call(this);},set(v){tracked=v;nativeValue.set.call(this,v);}});
    input.addEventListener('input',()=>{
      if(tracked===input.value){nativeValue.set.call(input,state.value);return;}
      tracked=input.value;state.value=input.value;state.changes++;publish();
    });
  }else{
    input.addEventListener('input',()=>nativeValue.set.call(input,state.value));
  }
  const year=document.createElement(options.yearInput?'input':'select');year.setAttribute('aria-label','Year');
  if(!options.yearInput)for(const value of ['2026','2027','2028']){const option=document.createElement('option');option.value=value;option.textContent=value;year.append(option);}
  const month=document.createElement('select');month.setAttribute('aria-label','Month');
  for(let i=1;i<=12;i++){const option=document.createElement('option');option.value=String(i).padStart(2,'0');option.textContent=option.value;month.append(option);}
  year.value=options.initialYear||'2026';month.value=options.initialMonth||'10';
  if(options.disabledPart==='year')year.disabled=true;
  if(options.disabledPart==='month')month.disabled=true;
  if(options.readOnlyYear)year.readOnly=true;
  if(options.ariaReadOnlyYear)year.setAttribute('aria-readonly','true');
  for(const control of [year,month])for(const event of ['input','change'])control.addEventListener(event,()=>{state.navigation_events++;publish();});
  const grid=document.createElement('div');grid.setAttribute('role','grid');
  const draw=()=>{
    grid.replaceChildren();
    const days=new Date(Number(year.value),Number(month.value),0).getDate();
    for(let day=1;day<=days;day++){
      const iso=year.value+'-'+month.value+'-'+String(day).padStart(2,'0');
      if(options.missing&&iso==='2027-06-30')continue;
      const copies=options.duplicate&&iso==='2027-06-30'?2:1;
      for(let copy=0;copy<copies;copy++){
        const button=document.createElement('button');button.type='button';button.textContent=String(day);
        button.setAttribute('aria-label',iso);button.title=iso;button.setAttribute('data-value',iso);
        if(options.disabled&&iso==='2027-06-30'){button.disabled=true;button.setAttribute('aria-disabled','true');}
        button.addEventListener('click',()=>{
          state.choices++;
          if(!options.reject){state.value=iso;nativeValue.set.call(input,iso);state.changes++;}
          if(options.rerender){const replacement=input.cloneNode(true);replacement.value=state.value;input.replaceWith(replacement);input=replacement;}
          close();
        });
        grid.append(button);
      }
    }
  };
  const navigate=()=>{
    draw();
    if(options.commitNavigation){state.value=year.value+'-'+month.value+'-01';nativeValue.set.call(input,state.value);state.changes++;}
    if(options.detachOnNavigation){const replacement=input.cloneNode(true);input.replaceWith(replacement);input=replacement;}
    publish();
  };
  year.addEventListener('change',navigate);month.addEventListener('change',navigate);
  dialog.append(year,month,grid);
  if(options.expandedBefore){draw();dialog.hidden=false;}
  input.addEventListener('click',()=>{state.opens++;state.expanded=true;input.setAttribute('aria-expanded','true');publish();setTimeout(()=>{draw();dialog.hidden=false;},15);});
  document.addEventListener('keydown',event=>{if(event.key==='Escape')close();});
  publish();
}
"#;
