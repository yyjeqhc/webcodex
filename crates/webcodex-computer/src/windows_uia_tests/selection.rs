use super::*;

fn selection_node(tree: &Value, title: &str) -> String {
    tree["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["title"] == title)
        .unwrap_or_else(|| panic!("missing fixture node {title}"))["element_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn assert_business(runtime: &ComputerRuntime, surface: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let tree = runtime.accessibility_tree(surface, 8, 256).unwrap();
        if tree["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["title"] == expected)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "business state did not become {expected}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "interactive Windows desktop; owned WPF process verifies actual Rust UIA selection and business events"]
fn computer_windows_selection_live() {
    let logs = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/selection-fixture")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&logs).unwrap();
    let child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-STA",
            "-WindowStyle",
            "Hidden",
            "-Command",
            include_str!("../fixtures/windows_selection.ps1"),
        ])
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(logs.join("stdout.txt")).unwrap())
        .stderr(std::fs::File::create(logs.join("stderr.txt")).unwrap())
        .spawn()
        .unwrap();
    let mut fixture = WindowsControlFixture { child };
    let window = fixture.wait_for_window("WebCodex native selection fixture", "selection fixture");
    assert_eq!(window.pid, fixture.child.id());
    let runtime = ComputerRuntime::new(ComputerConfig {
        max_encoded_image_bytes: 1024,
    });
    let windows = runtime.list_windows(MAX_WINDOWS).unwrap();
    let surface = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["title"] == "WebCodex native selection fixture")
        .unwrap()["surface_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        runtime.surfaces.lock().unwrap()[&surface].pid,
        fixture.child.id()
    );
    assert_business(
        &runtime,
        &surface,
        "selection-business:count=2;tab=0;changes=0",
    );
    let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
    let selected = selection_node(&tree, "Selection A");
    let legacy = runtime
        .element_state(&surface, &selected, false, false)
        .unwrap();
    assert!(legacy.get("can_select").is_none());
    assert!(legacy.get("selected").is_none());
    let state = runtime
        .element_state(&surface, &selected, true, false)
        .unwrap();
    assert_eq!(state["can_select"], true);
    assert_eq!(state["selected"], true);
    assert_eq!(state["can_press"], false);
    assert!(runtime
        .control(&surface, &selected, ComputerAction::Press)
        .unwrap_err()
        .contains("InvokePattern"));
    // The target is already selected, but Select must still clear the other selection.
    runtime
        .control(&surface, &selected, ComputerAction::Select)
        .unwrap();
    assert_business(
        &runtime,
        &surface,
        "selection-business:count=1;tab=0;changes=1",
    );
    assert!(runtime
        .control(&surface, &selected, ComputerAction::Select)
        .unwrap_err()
        .starts_with("stale_element:"));
    let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
    let disabled = selection_node(&tree, "Selection disabled");
    assert_eq!(
        runtime
            .element_state(&surface, &disabled, true, false)
            .unwrap()["can_select"],
        false
    );
    assert!(runtime
        .control(&surface, &disabled, ComputerAction::Select)
        .unwrap_err()
        .contains("disabled"));
    let tab = selection_node(&tree, "Selection second tab");
    assert_eq!(
        runtime.element_state(&surface, &tab, true, false).unwrap()["selected"],
        false
    );
    runtime
        .control(&surface, &tab, ComputerAction::Select)
        .unwrap();
    assert_business(
        &runtime,
        &surface,
        "selection-business:count=1;tab=1;changes=2",
    );
    let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
    let tab = selection_node(&tree, "Selection second tab");
    assert_eq!(
        runtime.element_state(&surface, &tab, true, false).unwrap()["selected"],
        true
    );
    runtime
        .control(&surface, &tab, ComputerAction::Select)
        .unwrap();
    assert_business(
        &runtime,
        &surface,
        "selection-business:count=1;tab=1;changes=2",
    );
}
