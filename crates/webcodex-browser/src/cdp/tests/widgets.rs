use super::*;

fn ax(id: i64, role: &str) -> Value {
    json!({"nodeId": id.to_string(), "backendDOMNodeId": id,
        "role": {"value": role}, "name": {"value": format!("Field {id}")}})
}

fn actions(nodes: &[BackendNode], id: i64) -> Vec<String> {
    nodes
        .iter()
        .find(|node| node.backend_node_id == Some(id))
        .unwrap()
        .capability
        .action_names()
}

#[test]
fn custom_widget_admission_uses_dom_control_kind_and_preserves_native_actions() {
    let root = json!({"nodeType":9, "children":[
        {"nodeType":1,"localName":"select","backendNodeId":1},
        {"nodeType":1,"localName":"input","backendNodeId":2,"attributes":["role","combobox","type","text"]},
        {"nodeType":1,"localName":"div","backendNodeId":3,"attributes":["role","listbox"]},
        {"nodeType":1,"localName":"input","backendNodeId":4,"attributes":["role","combobox","aria-haspopup","dialog","aria-label","Date","readonly",""]},
        {"nodeType":1,"localName":"input","backendNodeId":5,"attributes":["type","date"]},
        {"nodeType":1,"localName":"input","backendNodeId":6,"attributes":["type","month"]},
        {"nodeType":1,"localName":"input","backendNodeId":7,"attributes":["type","file","aria-haspopup","dialog"]},
        {"nodeType":1,"localName":"input","backendNodeId":8,"attributes":["type","date","aria-haspopup","dialog"]},
        {"nodeType":1,"localName":"input","backendNodeId":9,"attributes":["type","number"]},
        {"nodeType":1,"localName":"input","backendNodeId":10,"attributes":["role","combobox","aria-haspopup","dialog","aria-label","Address"]}
    ]});
    let raw = [
        ax(1, "combobox"),
        ax(2, "combobox"),
        ax(3, "listbox"),
        ax(4, "combobox"),
        ax(5, "Date"),
        ax(6, "DateTime"),
        ax(7, "combobox"),
        ax(8, "combobox"),
        ax(9, "combobox"),
        ax(10, "combobox"),
    ];
    let (nodes, _) = project_ax_nodes(&raw, Some(&root));
    assert_eq!(actions(&nodes, 1), ["select_option"]);
    assert!(actions(&nodes, 2).contains(&"select_choice".to_string()));
    assert_eq!(actions(&nodes, 3), ["select_choice"]);
    assert!(actions(&nodes, 4).contains(&"set_date".to_string()));
    assert!(!actions(&nodes, 4).contains(&"select_choice".to_string()));
    assert_eq!(actions(&nodes, 5), ["set_value"]);
    assert_eq!(actions(&nodes, 6), ["set_value"]);
    assert_eq!(actions(&nodes, 7), ["upload_file"]);
    assert_eq!(actions(&nodes, 8), ["set_value"]);
    assert_eq!(actions(&nodes, 9), ["set_value"]);
    assert!(actions(&nodes, 10).contains(&"select_choice".to_string()));
    assert!(!actions(&nodes, 10).contains(&"set_date".to_string()));
    let (without_dom, _) = project_ax_nodes(&raw, None);
    assert!(without_dom
        .iter()
        .all(|node| !node.capability.custom_choice && !node.capability.custom_date));
}

#[test]
fn custom_widget_hints_inherit_through_generic_inputs_without_admitting_clear_buttons() {
    let root = json!({"nodeType":9,"children":[
        {"nodeType":1,"localName":"div","backendNodeId":1,"attributes":["class","el-date-editor"],"children":[
            {"nodeType":1,"localName":"input","backendNodeId":2,"attributes":["class","el-input__inner","readonly",""]},
            {"nodeType":1,"localName":"button","backendNodeId":3,"attributes":["aria-label","Clear"]},
            {"nodeType":1,"localName":"input","backendNodeId":4,"attributes":["type","date"]}
        ]},
        {"nodeType":1,"localName":"div","backendNodeId":10,"attributes":["class","ant-cascader"],"children":[
            {"nodeType":1,"localName":"input","backendNodeId":11,"attributes":["class","ant-input"]},
            {"nodeType":1,"localName":"button","backendNodeId":12,"attributes":["aria-label","Clear"]},
            {"nodeType":1,"localName":"input","backendNodeId":13,"attributes":["type","file"]}
        ]}
    ]});
    let raw = [
        ax(2, "textbox"),
        ax(3, "button"),
        ax(4, "Date"),
        ax(11, "textbox"),
        ax(12, "button"),
        ax(13, "button"),
    ];
    let (mut nodes, _) = project_ax_nodes(&raw, Some(&root));
    let contexts = index_form_contexts(&root);
    assert_eq!(
        contexts[&2].component_hint.as_deref(),
        Some("element-datepicker")
    );
    apply_custom_widget_contexts(&mut nodes, &contexts);
    assert!(actions(&nodes, 2).contains(&"set_date".to_string()));
    assert!(!actions(&nodes, 3).contains(&"set_date".to_string()));
    assert_eq!(actions(&nodes, 4), ["set_value"]);
    assert!(actions(&nodes, 11).contains(&"select_choice".to_string()));
    assert_eq!(actions(&nodes, 12), ["click"]);
    assert_eq!(actions(&nodes, 13), ["upload_file"]);
}

#[test]
fn disabled_custom_widgets_withhold_choice_and_date_effects() {
    let root = json!({"nodeType":9, "children":[
        {"nodeType":1,"localName":"input","backendNodeId":1,"attributes":["role","combobox","disabled",""]},
        {"nodeType":1,"localName":"input","backendNodeId":2,"attributes":["role","combobox","aria-haspopup","dialog","disabled",""]}
    ]});
    let mut raw = [ax(1, "combobox"), ax(2, "combobox")];
    for node in &mut raw {
        node["properties"] = json!([{"name":"disabled","value":{"value":true}}]);
    }
    let (mut nodes, _) = project_ax_nodes(&raw, Some(&root));
    apply_custom_widget_contexts(&mut nodes, &index_form_contexts(&root));
    assert!(nodes.iter().all(|node| !node.capability.admits_any()));
}
