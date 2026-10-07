use super::*;

fn fixture() -> (Vec<BackendNode>, Value, Value) {
    let root = json!({"nodeType":9,"backendNodeId":1,"children":[
        {"nodeType":1,"localName":"div","backendNodeId":2,"childNodeCount":1,"children":[
            {"nodeType":3,"nodeValue":"Graduate software engineer","backendNodeId":3}
        ]}
    ]});
    let (nodes, _) = project_ax_nodes(
        &[json!({"nodeId":"card","backendDOMNodeId":2,"role":{"value":"generic"}})],
        Some(&root),
    );
    let capture = json!({"strings":["pointer","visible","block","auto","1"],"documents":[{
        "nodes":{"backendNodeId":[1,2,3],"isClickable":{"index":[1]}},
        "layout":{"nodeIndex":[1],"styles":[[0,1,2,3,4]],"bounds":[[10,20,300,100]]}
    }]});
    (nodes, root, capture)
}

#[test]
fn card_requires_browser_event_evidence_visible_affordance_and_unambiguous_content() {
    let (mut nodes, root, capture) = fixture();
    admit_clickable_cards(&mut nodes, &root, &capture);
    assert_eq!(nodes[0].capability.action_names(), ["click"]);
    assert_eq!(nodes[0].name.as_deref(), Some("Graduate software engineer"));
    for mutation in 0..8 {
        let (mut nodes, mut root, mut capture) = fixture();
        match mutation {
            0 => capture["documents"][0]["nodes"]["isClickable"]["index"] = json!([]),
            1 => capture["strings"][0] = json!("default"),
            2 => capture["strings"][1] = json!("hidden"),
            3 => capture["strings"][3] = json!("none"),
            4 => capture["documents"][0]["nodes"]["backendNodeId"][0] = json!(99),
            5 => root["children"][0]["attributes"] = json!(["aria-disabled", "true"]),
            6 => root["children"][0]["childNodeCount"] = json!(2),
            _ => {
                root["children"][0]["children"][0] =
                    json!({"nodeType":1,"localName":"button","backendNodeId":3})
            }
        }
        admit_clickable_cards(&mut nodes, &root, &capture);
        assert!(!nodes[0].capability.admits_any(), "case {mutation}");
    }
}

#[test]
fn plain_or_onclick_div_and_nested_event_target_do_not_grant_authority() {
    let (mut nodes, mut root, mut capture) = fixture();
    root["children"][0]["attributes"] = json!(["onclick", "doSomething()"]);
    capture["documents"][0]["nodes"]["isClickable"]["index"] = json!([]);
    admit_clickable_cards(&mut nodes, &root, &capture);
    assert!(!nodes[0].capability.admits_any());
    capture["documents"][0]["nodes"]["isClickable"]["index"] = json!([1, 2]);
    admit_clickable_cards(&mut nodes, &root, &capture);
    assert!(!nodes[0].capability.admits_any());
}

#[test]
fn oversized_or_missing_evidence_and_private_roots_fail_closed() {
    let (mut nodes, root, mut capture) = fixture();
    capture["documents"][0]["nodes"]["backendNodeId"] = json!(vec![1; MAX_CLICK_SCAN_NODES + 1]);
    admit_clickable_cards(&mut nodes, &root, &capture);
    assert!(!nodes[0].capability.admits_any());
    let (mut nodes, mut root, capture) = fixture();
    root["children"][0]["shadowRoots"] = json!([]);
    admit_clickable_cards(&mut nodes, &root, &capture);
    assert!(!nodes[0].capability.admits_any());
    admit_clickable_cards(&mut nodes, &root, &json!({}));
    assert!(!nodes[0].capability.admits_any());
}

#[test]
fn transparent_ancestors_nested_handlers_and_long_or_unknown_content_are_rejected() {
    for case in 0..5 {
        let (mut nodes, mut root, mut capture) = fixture();
        match case {
            0 => {
                let child = root["children"][0].take();
                root["children"] =
                    json!([{"nodeType":1,"localName":"div","backendNodeId":4,"children":[child]}]);
                capture["strings"].as_array_mut().unwrap().push(json!("0"));
                capture["documents"][0]["nodes"]["backendNodeId"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(4));
                capture["documents"][0]["layout"]["nodeIndex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(3));
                capture["documents"][0]["layout"]["styles"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!([0, 1, 2, 3, 5]));
                capture["documents"][0]["layout"]["bounds"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!([0, 0, 300, 100]));
            }
            1 => {
                root["backendNodeId"] = json!(1);
                capture["documents"][0]["nodes"]["isClickable"]["index"] = json!([0, 1]);
            }
            2 => root["children"][0]["children"][0]["nodeValue"] = json!("中文".repeat(100)),
            3 => root["children"][0]["attributes"] = json!(["contenteditable", "true"]),
            _ => root["backendNodeId"] = Value::Null,
        }
        admit_clickable_cards(&mut nodes, &root, &capture);
        assert!(!nodes[0].capability.admits_any(), "case {case}");
    }
}
