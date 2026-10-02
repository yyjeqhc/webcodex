//! Exact Project-bound provider dispatch regressions; no Python/Node dependency.
use super::*;
use std::time::{Duration, Instant};
use webcodex_core::plugin::PluginProjectTarget;

fn register(fixture: &Fixture, path: &Path) -> (PathBuf, PluginProjectTarget) {
    let registry = fixture._temp.path().join("registry");
    fs::create_dir_all(&registry).unwrap();
    fs::write(
        registry.join("bound.toml"),
        format!("id = \"bound\"\npath = {:?}\n", path.to_string_lossy()),
    )
    .unwrap();
    let target = PluginProjectTarget {
        project_id: "bound".to_string(),
        root_fingerprint: crate::webcodex_runner::projects::project_root_fingerprint(
            &path.canonicalize().unwrap(),
        ),
    };
    (registry, target)
}

fn request(fixture: &Fixture, target: Option<PluginProjectTarget>) -> PluginGatewayRequest {
    PluginGatewayRequest::ToolsCall {
        provider_id: fixture.provider.provider_id.clone(),
        provider_instance_id: fixture.provider.provider_instance_id.clone(),
        name: "echo".to_string(),
        arguments: json!({"value":"bound"}),
        expected_schema: fixture.schema.clone().unwrap(),
        project_target: target,
    }
}

#[test]
fn project_bound_provider_requires_exact_current_project_before_dispatch() {
    let fixture = Fixture::new("project_bound", 5);
    assert!(fixture.schema.as_ref().unwrap().project_bound);
    let (registry, target) = register(&fixture, fixture._temp.path());
    for invalid in [
        None,
        Some(PluginProjectTarget {
            project_id: "missing".into(),
            ..target.clone()
        }),
        Some(PluginProjectTarget {
            root_fingerprint: "stale-root".into(),
            ..target.clone()
        }),
    ] {
        let response = fixture
            .manager
            .handle_with_project_registry(request(&fixture, invalid), &registry);
        assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
        assert_eq!(response.error.unwrap().code, "plugin_project_mismatch");
    }
    assert_eq!(fixture.marker_count("call"), 0);
    let valid = fixture
        .manager
        .handle_with_project_registry(request(&fixture, Some(target)), &registry);
    assert_eq!(
        valid.dispatch_state,
        PluginDispatchState::Completed,
        "{:?}",
        valid.error
    );
    assert!(valid.error.is_none());
    assert_eq!(fixture.marker_count("call"), 1);
}

#[test]
fn project_bound_provider_rejects_retargeted_registry_and_missing_registry_authority() {
    let fixture = Fixture::new("project_bound", 5);
    let (registry, target) = register(&fixture, fixture._temp.path());
    let absent = fixture
        .manager
        .handle(request(&fixture, Some(target.clone())));
    assert_eq!(absent.error.unwrap().code, "plugin_project_mismatch");
    let other = fixture._temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let (_, moved) = register(&fixture, &other);
    for selected in [target, moved] {
        let result = fixture
            .manager
            .handle_with_project_registry(request(&fixture, Some(selected)), &registry);
        assert_eq!(result.dispatch_state, PluginDispatchState::NotStarted);
        assert_eq!(result.error.unwrap().code, "plugin_project_mismatch");
    }
    assert_eq!(fixture.marker_count("call"), 0);
}

#[cfg(unix)]
#[test]
fn project_bound_provider_rejects_same_path_directory_replacement() {
    let fixture = Fixture::new("project_bound", 5);
    let root = fixture._temp.path().to_path_buf();
    let registry = tempfile::tempdir().unwrap();
    fs::write(
        registry.path().join("bound.toml"),
        format!("id = \"bound\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();
    let target = PluginProjectTarget {
        project_id: "bound".to_string(),
        root_fingerprint: crate::webcodex_runner::projects::project_root_fingerprint(
            &root.canonicalize().unwrap(),
        ),
    };
    let retired = root.with_extension("retired");
    fs::rename(&root, &retired).unwrap();
    fs::create_dir(&root).unwrap();

    let response = fixture
        .manager
        .handle_with_project_registry(request(&fixture, Some(target)), registry.path());
    assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(response.error.unwrap().code, "plugin_project_mismatch");
    assert_eq!(fixture.marker_count("call"), 0);

    drop(fixture);
    fs::remove_dir_all(retired).unwrap();
}

#[test]
fn project_bound_calls_respect_write_revocation_without_a_path_or_provider_change() {
    let fixture = Fixture::new("project_bound", 5);
    let (registry, target) = register(&fixture, fixture._temp.path());
    for revoked in ["allow_patch = false", "disabled = true"] {
        fs::write(
            registry.join("bound.toml"),
            format!(
                "id = \"bound\"\npath = {:?}\n{revoked}\n",
                fixture._temp.path().to_string_lossy()
            ),
        )
        .unwrap();
        let response = fixture
            .manager
            .handle_with_project_registry(request(&fixture, Some(target.clone())), &registry);
        assert_eq!(response.dispatch_state, PluginDispatchState::NotStarted);
        assert_eq!(response.error.unwrap().code, "plugin_project_mismatch");
    }
    assert_eq!(fixture.marker_count("call"), 0);
}

#[test]
fn project_bound_authority_flag_is_part_of_the_observed_schema_fence() {
    let fixture = Fixture::new("project_bound", 5);
    let (registry, target) = register(&fixture, fixture._temp.path());
    let mut observed = request(&fixture, Some(target));
    if let PluginGatewayRequest::ToolsCall {
        expected_schema, ..
    } = &mut observed
    {
        expected_schema.project_bound = false;
    }
    let result = fixture
        .manager
        .handle_with_project_registry(observed, &registry);
    assert_eq!(result.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(result.error.unwrap().code, "plugin_schema_changed");
    assert_eq!(fixture.marker_count("call"), 0);
}

#[test]
fn busy_project_bound_calls_do_not_queue_and_later_recheck_the_exact_target() {
    let fixture = Arc::new(Fixture::new("project_bound_hold", 10));
    let (registry, target) = register(&fixture, fixture._temp.path());
    let first_fixture = fixture.clone();
    let first_registry = registry.clone();
    let first_target = target.clone();
    let first = std::thread::spawn(move || {
        first_fixture.manager.handle_with_project_registry(
            request(&first_fixture, Some(first_target)),
            &first_registry,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.marker_count("call") == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fixture.marker_count("call"), 1);
    let busy = fixture
        .manager
        .handle_with_project_registry(request(&fixture, Some(target.clone())), &registry);
    assert_eq!(busy.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(busy.error.unwrap().code, "plugin_provider_busy");
    fs::remove_file(registry.join("bound.toml")).unwrap();
    fs::write(fixture.marker.with_extension("release"), "release").unwrap();
    assert_eq!(
        first.join().unwrap().dispatch_state,
        PluginDispatchState::Completed
    );
    let rejected = fixture
        .manager
        .handle_with_project_registry(request(&fixture, Some(target)), &registry);
    assert_eq!(rejected.dispatch_state, PluginDispatchState::NotStarted);
    assert_eq!(rejected.error.unwrap().code, "plugin_project_mismatch");
    assert_eq!(fixture.marker_count("call"), 1);
}
