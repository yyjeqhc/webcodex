use super::*;

fn fixture() -> (Value, Value, Vec<Value>) {
    let tree = json!({"frameTree": {"frame": {"id":"top", "loaderId":"top-loader", "securityOrigin":"https://example.test"},
        "childFrames":[{"frame":{"id":"child", "parentId":"top", "loaderId":"child-loader", "securityOrigin":"https://example.test"}}]}});
    let controls = [
        (10, "button", "", "button"),
        (11, "input", "text", "textbox"),
        (12, "select", "", "combobox"),
        (13, "input", "number", "spinbutton"),
        (14, "div", "", "slider"),
    ];
    let children = controls.iter().map(|(id, tag, kind, _)| json!({"nodeType":1,"backendNodeId":id,"localName":tag,"attributes":["type",kind]})).collect::<Vec<_>>();
    let root = json!({"nodeType":9,"backendNodeId":1,"children":[{"nodeType":1,"localName":"iframe","backendNodeId":2,"frameId":"child",
        "contentDocument":{"nodeType":9,"backendNodeId":3,"children":children}}]});
    let ax = controls.iter().map(|(id, _, _, role)| json!({"nodeId":id.to_string(),"backendDOMNodeId":id,"role":{"value":role},"name":{"value":"Fixture control"}})).collect();
    (tree, root, ax)
}

#[test]
fn same_origin_document_uses_existing_control_admission() {
    let (tree, root, ax) = fixture();
    let docs = frame_documents(&tree, &root);
    assert_eq!(docs.len(), 1);
    let (nodes, _) = project_ax_nodes(&ax, Some(docs[0].1));
    let actions = nodes
        .iter()
        .map(|n| n.capability.action_names())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            vec!["click"],
            vec!["click", "input_text"],
            vec!["select_option"],
            vec!["set_value"],
            vec![]
        ]
    );
    // The top document must not classify controls belonging to any iframe.
    assert!(project_ax_nodes(&ax, Some(&root))
        .0
        .iter()
        .all(|n| !n.capability.admits_any()));
}

#[test]
fn same_origin_iframe_withholds_disabled_and_read_only_effects() {
    let tree = json!({"frameTree": {"frame": {"id":"top", "loaderId":"top-loader", "securityOrigin":"https://example.test"},
        "childFrames":[{"frame":{"id":"child", "parentId":"top", "loaderId":"child-loader", "securityOrigin":"https://example.test"}}]}});
    let root = json!({"nodeType":9,"backendNodeId":1,"children":[{"nodeType":1,"localName":"iframe","backendNodeId":2,"frameId":"child",
    "contentDocument":{"nodeType":9,"backendNodeId":3,"children":[
        {"nodeType":1,"localName":"input","backendNodeId":21,"attributes":["type","number","disabled",""]},
        {"nodeType":1,"localName":"select","backendNodeId":22,"attributes":["disabled",""],"children":[
            {"nodeType":1,"localName":"option","backendNodeId":23,"attributes":["value","alpha"]}
        ]},
        {"nodeType":1,"localName":"input","backendNodeId":24,"attributes":["type","text","readonly","readonly"]},
        {"nodeType":1,"localName":"input","backendNodeId":25,"attributes":["type","text"]},
        {"nodeType":1,"localName":"input","backendNodeId":26,"attributes":["type","number"]},
        {"nodeType":1,"localName":"select","backendNodeId":27,"children":[
            {"nodeType":1,"localName":"option","backendNodeId":28,"attributes":["value","beta"]}
        ]}
    ]}}]});
    let ax = vec![
        frame_state_ax("Qty", "spinbutton", 21, Some(true), None),
        frame_state_ax("Pick", "combobox", 22, Some(true), None),
        frame_state_ax("Alpha", "option", 23, None, None),
        frame_state_ax("Notes", "textbox", 24, None, Some(true)),
        frame_state_ax("Name", "textbox", 25, None, None),
        frame_state_ax("Amount", "spinbutton", 26, Some(false), None),
        frame_state_ax("Kind", "combobox", 27, None, None),
        frame_state_ax("Beta", "option", 28, None, None),
    ];
    let docs = frame_documents(&tree, &root);
    assert_eq!(docs.len(), 1);
    let (nodes, _) = project_ax_nodes(&ax, Some(docs[0].1));
    let node = |name: &str| {
        nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    let actions = |name: &str| node(name).capability.action_names();
    assert_eq!(node("Qty").disabled, Some(true));
    assert!(actions("Qty").is_empty());
    assert!(!node("Qty").capability.admits_any());
    assert!(actions("Pick").is_empty());
    assert!(!node("Pick").capability.admits_any());
    assert!(actions("Alpha").is_empty());
    assert!(!node("Alpha").select_choice);
    assert_eq!(node("Notes").read_only, Some(true));
    assert_eq!(actions("Notes"), ["click"]);
    assert_eq!(node("Name").disabled, None);
    assert_eq!(node("Name").read_only, None);
    assert_eq!(actions("Name"), ["click", "input_text"]);
    assert_eq!(node("Amount").disabled, Some(false));
    assert_eq!(actions("Amount"), ["set_value"]);
    assert_eq!(actions("Kind"), ["select_option"]);
    assert!(node("Beta").select_choice);
    assert!(actions("Beta").is_empty());
    assert!(project_ax_nodes(&ax, Some(&root))
        .0
        .iter()
        .all(|node| !node.capability.admits_any()));
}

fn frame_state_ax(
    name: &str,
    role: &str,
    backend_node_id: i64,
    disabled: Option<bool>,
    read_only: Option<bool>,
) -> Value {
    let mut node = json!({
        "nodeId": format!("ax-{name}"),
        "role": {"value": role},
        "name": {"value": name},
        "backendDOMNodeId": backend_node_id
    });
    let mut properties = Vec::new();
    if let Some(disabled) = disabled {
        properties.push(json!({"name": "disabled", "value": {"value": disabled}}));
    }
    if let Some(read_only) = read_only {
        properties.push(json!({"name": "readonly", "value": {"value": read_only}}));
    }
    if !properties.is_empty() {
        node["properties"] = json!(properties);
    }
    node
}

#[test]
fn cross_origin_opaque_missing_and_sandboxed_frames_fail_closed() {
    let (tree, root, _) = fixture();
    for origin in [
        json!("https://other.test"),
        json!("http://example.test"),
        json!("https://example.test:444"),
        json!("://"),
        Value::Null,
    ] {
        let mut tree = tree.clone();
        tree["frameTree"]["childFrames"][0]["frame"]["securityOrigin"] = origin;
        assert!(frame_documents(&tree, &root).is_empty());
    }
    let mut sandbox = root.clone();
    sandbox["children"][0]["attributes"] = json!(["sandbox", "allow-scripts"]);
    assert!(frame_documents(&tree, &sandbox).is_empty());
    let mut missing = root.clone();
    missing["children"][0]["contentDocument"] = Value::Null;
    assert!(frame_documents(&tree, &missing).is_empty());
}

#[test]
fn fence_tracks_frame_loader_document_replacement_and_origin_but_not_form_values() {
    let (tree, root, _) = fixture();
    let original = frame_fence(&tree, &root);
    for field in ["id", "loaderId", "securityOrigin", "parentId"] {
        let mut changed = tree.clone();
        changed["frameTree"]["childFrames"][0]["frame"][field] = json!("replacement");
        assert_ne!(original, frame_fence(&changed, &root));
    }
    let mut detached = tree.clone();
    detached["frameTree"]["childFrames"] = json!([]);
    assert_ne!(original, frame_fence(&detached, &root));
    let mut replaced = root.clone();
    replaced["children"][0]["contentDocument"]["backendNodeId"] = json!(99);
    assert_ne!(original, frame_fence(&tree, &replaced));
    let mut edited = root.clone();
    edited["children"][0]["contentDocument"]["children"][1]["attributes"] =
        json!(["type", "text", "value", "new value"]);
    assert_eq!(original, frame_fence(&tree, &edited));
}
