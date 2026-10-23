use crate::project_build::{ProjectBuildAdapter, ProjectBuildRequest, ProjectBuildScope};
use crate::project_operation::ProjectOperationScope;
use crate::project_validation::{
    ProjectValidationAction, ProjectValidationAdapter, ProjectValidationRequest,
    ProjectValidationScope,
};

fn build_request(scope: ProjectBuildScope) -> ProjectBuildRequest {
    ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Auto,
        scope: Some(scope),
        dependency_policy: None,
    }
}

#[test]
fn shared_scope_all_packages_is_closed_and_mutually_exclusive() {
    let all = ProjectOperationScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    };
    assert!(build_request(all.clone()).validate().is_ok());
    assert!(validation_request(all.clone()).validate().is_ok());
    assert_eq!(
        serde_json::to_value(all).unwrap(),
        serde_json::json!({"all_packages":true})
    );

    let mixed = ProjectOperationScope {
        packages: vec!["package-a".into()],
        all_packages: true,
        exclude_packages: Vec::new(),
    };
    assert!(build_request(mixed.clone()).validate().is_err());
    assert!(validation_request(mixed).validate().is_err());
}

fn validation_request(scope: ProjectValidationScope) -> ProjectValidationRequest {
    ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: None,
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Auto,
        scope: Some(scope),
        dependency_policy: None,
        dependency_resolution_policy: None,
        test: None,
    }
}

#[test]
fn project_build_and_validation_scope_names_share_one_wire_type() {
    let shared = ProjectOperationScope {
        packages: vec!["package-a".into(), "package-b".into()],
        all_packages: false,
        exclude_packages: Vec::new(),
    };
    let build: ProjectBuildScope = shared.clone();
    let validation: ProjectValidationScope = shared.clone();

    assert_eq!(build, validation);
    assert_eq!(
        serde_json::to_value(build).unwrap(),
        serde_json::json!({"packages":["package-a","package-b"]})
    );
    assert_eq!(
        serde_json::to_value(validation).unwrap(),
        serde_json::json!({"packages":["package-a","package-b"]})
    );
}

#[test]
fn shared_scope_validation_preserves_operation_specific_errors() {
    for (packages, build_error, validation_error) in [
        (
            Vec::<String>::new(),
            "project build scope must select packages or all_packages=true",
            "project validation scope must select packages or all_packages=true",
        ),
        (
            vec!["bad\npackage".into()],
            "invalid project build package scope",
            "invalid project validation package scope",
        ),
    ] {
        let scope = ProjectOperationScope {
            packages,
            all_packages: false,
            exclude_packages: Vec::new(),
        };
        assert_eq!(
            build_request(scope.clone()).validate().unwrap_err(),
            build_error
        );
        assert_eq!(
            validation_request(scope).validate().unwrap_err(),
            validation_error
        );
    }
}

#[test]
fn workspace_exclusions_require_bounded_exact_members_and_leave_one_selected() {
    let scope = ProjectOperationScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: vec!["zeta".into(), "alpha".into(), "alpha".into()],
    };
    assert!(scope.validate().is_ok());
    assert_eq!(
        scope.normalized_exclude_packages().unwrap(),
        ["alpha", "zeta"]
    );
    assert!(scope
        .validate_excluded_members(&["alpha".into(), "beta".into(), "zeta".into()])
        .is_ok());
    assert_eq!(
        serde_json::to_value(scope).unwrap(),
        serde_json::json!({"all_packages":true,"exclude_packages":["zeta","alpha","alpha"]})
    );

    for exclude_packages in [vec!["missing".into()], vec!["alpha".into(), "beta".into()]] {
        let scope = ProjectOperationScope {
            packages: Vec::new(),
            all_packages: true,
            exclude_packages,
        };
        if scope.exclude_packages == ["missing"] {
            assert_eq!(
                scope.validate_excluded_members(&["alpha".into(), "beta".into()]),
                Err("excluded package is not a workspace member")
            );
        } else {
            assert_eq!(
                scope.validate_excluded_members(&["alpha".into(), "beta".into()]),
                Err("scope cannot exclude every workspace member")
            );
        }
    }

    for bad in ["alpha*", "../alpha", "alpha beta", "-p"] {
        let scope = ProjectOperationScope {
            packages: Vec::new(),
            all_packages: true,
            exclude_packages: vec![bad.into()],
        };
        assert_eq!(
            scope.validate(),
            Err(crate::project_operation::ProjectOperationScopeError::InvalidExclusion)
        );
    }
    let explicit = ProjectOperationScope {
        packages: vec!["alpha".into()],
        all_packages: false,
        exclude_packages: vec!["beta".into()],
    };
    assert_eq!(
        explicit.validate(),
        Err(crate::project_operation::ProjectOperationScopeError::ExclusionRequiresAllPackages)
    );
}

#[test]
fn workspace_exclusion_schema_allows_empty_but_bounds_nonempty_lists() {
    let scope = ProjectOperationScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    };
    assert!(build_request(scope.clone()).validate().is_ok());
    assert_eq!(
        serde_json::to_value(scope).unwrap(),
        serde_json::json!({"all_packages":true})
    );

    let schema = serde_json::to_value(schemars::schema_for!(ProjectOperationScope)).unwrap();
    let mut pending = vec![&schema];
    let mut found = false;
    let mut empty_only_found = false;
    while let Some(value) = pending.pop() {
        if let Some(properties) = value
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            if let Some(exclusions) = properties.get("exclude_packages") {
                let max = exclusions
                    .get("maxItems")
                    .and_then(serde_json::Value::as_u64);
                if max == Some(0) {
                    empty_only_found = true;
                    continue;
                }
                found = true;
                assert_eq!(max, Some(8));
                assert!(exclusions.get("minItems").is_none());
                let item = exclusions.get("items").unwrap();
                assert_eq!(
                    item.get("minLength").and_then(serde_json::Value::as_u64),
                    Some(1)
                );
                assert_eq!(
                    item.get("maxLength").and_then(serde_json::Value::as_u64),
                    Some(256)
                );
            }
        }
        match value {
            serde_json::Value::Object(object) => pending.extend(object.values()),
            serde_json::Value::Array(array) => pending.extend(array.iter()),
            _ => {}
        }
    }
    assert!(found, "manual project scope schema must expose exclusions");
    assert!(
        empty_only_found,
        "explicit packages must permit only empty exclusions"
    );
}
