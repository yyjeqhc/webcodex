use super::*;
use webcodex_core::computer_scroll_contract::{ComputerScrollAmount, ComputerScrollDirection};

fn node(tree: &Value, title: &str) -> String {
    tree["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["title"] == title)
        .unwrap_or_else(|| panic!("missing fixture container {title}"))["element_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn probe(path: &std::path::Path) -> Value {
    // The application's UI thread independently records actual WPF offsets.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(value) = serde_json::from_str(text.trim_start_matches('\u{feff}')) {
                return value;
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture offset publication missing"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "interactive Windows desktop; owned WPF fixture independently checks real container offsets"]
fn computer_windows_bounded_scroll_live() {
    let logs = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scroll-fixture")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&logs).unwrap();
    let offsets = logs.join("offsets.json");
    let child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-STA",
            "-WindowStyle",
            "Hidden",
            "-Command",
            include_str!("../fixtures/windows_scroll.ps1"),
        ])
        .env("WEBCODEX_SCROLL_FIXTURE_PROBE", &offsets)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(logs.join("stdout.txt")).unwrap())
        .stderr(std::fs::File::create(logs.join("stderr.txt")).unwrap())
        .spawn()
        .unwrap();
    let mut fixture = WindowsControlFixture { child };
    let window = fixture.wait_for_window("WebCodex bounded scroll fixture", "scroll fixture");
    assert_eq!(window.pid, fixture.child.id());
    let runtime = ComputerRuntime::new(ComputerConfig {
        max_encoded_image_bytes: 1024,
    });
    let windows = runtime.list_windows(MAX_WINDOWS).unwrap();
    let surface = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["title"] == "WebCodex bounded scroll fixture")
        .unwrap()["surface_id"]
        .as_str()
        .unwrap()
        .to_string();
    probe(&offsets);
    let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
    let outer = node(&tree, "Scroll outer container");
    let legacy = runtime
        .element_state(&surface, &outer, false, false)
        .unwrap();
    assert!(legacy.get("scroll").is_none());
    let state = runtime
        .element_state(&surface, &outer, false, true)
        .unwrap();
    assert_eq!(state["scroll"]["horizontal_percent"], 0.0);
    assert_eq!(state["scroll"]["vertical_percent"], 0.0);
    let inner = node(&tree, "Scroll inner container");
    assert!(runtime
        .scroll(
            &surface,
            &inner,
            ComputerScrollDirection::Right,
            ComputerScrollAmount::Small
        )
        .unwrap_err()
        .contains("axis"));
    let disabled = node(&tree, "Scroll disabled container");
    assert!(runtime
        .scroll(
            &surface,
            &disabled,
            ComputerScrollDirection::Down,
            ComputerScrollAmount::Small
        )
        .unwrap_err()
        .contains("disabled"));
    let unsupported = node(&tree, "Scroll unsupported label");
    assert!(runtime
        .scroll(
            &surface,
            &unsupported,
            ComputerScrollDirection::Down,
            ComputerScrollAmount::Small
        )
        .unwrap_err()
        .contains("ScrollPattern"));
    assert_eq!(probe(&offsets)["outer_y"], 0.0);
    let output = runtime
        .scroll(
            &surface,
            &inner,
            ComputerScrollDirection::Down,
            ComputerScrollAmount::Large,
        )
        .unwrap();
    assert_eq!(output["position_before"], 0.0);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let actual = probe(&offsets);
        assert_eq!(actual["outer_y"], 0.0);
        assert_eq!(actual["outer_x"], 0.0);
        if actual["inner_y"].as_f64().unwrap() > 0.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "inner scroll did not move actual content"
        );
        thread::sleep(Duration::from_millis(10));
    }
    // A refreshed tree invalidates old IDs; every following action uses the latest container.
    runtime.accessibility_tree(&surface, 8, 256).unwrap();
    assert!(runtime
        .scroll(
            &surface,
            &outer,
            ComputerScrollDirection::Down,
            ComputerScrollAmount::Small
        )
        .unwrap_err()
        .starts_with("stale_element:"));
    for direction in [
        ComputerScrollDirection::Right,
        ComputerScrollDirection::Left,
        ComputerScrollDirection::Down,
        ComputerScrollDirection::Up,
    ] {
        let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
        let outer = node(&tree, "Scroll outer container");
        let before = probe(&offsets);
        runtime
            .scroll(&surface, &outer, direction, ComputerScrollAmount::Large)
            .unwrap();
        let field = if direction.horizontal() {
            "outer_x"
        } else {
            "outer_y"
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let after = probe(&offsets);
            if (after[field].as_f64().unwrap() - before[field].as_f64().unwrap())
                * if direction.increasing() { 1.0 } else { -1.0 }
                > 0.0
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{direction:?} did not move actual content"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    // Boundary behavior is a successful observed no-op, with unchanged application offsets.
    let tree = runtime.accessibility_tree(&surface, 8, 256).unwrap();
    let outer = node(&tree, "Scroll outer container");
    let before = probe(&offsets);
    let output = runtime
        .scroll(
            &surface,
            &outer,
            ComputerScrollDirection::Up,
            ComputerScrollAmount::Small,
        )
        .unwrap();
    assert_eq!(output["position_before"], 0.0);
    assert_eq!(output["position_after"], 0.0);
    assert_eq!(probe(&offsets), before);
}
