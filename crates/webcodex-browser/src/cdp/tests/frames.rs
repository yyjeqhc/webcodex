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
