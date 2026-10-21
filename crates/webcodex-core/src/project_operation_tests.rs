use crate::project_build::{ProjectBuildAdapter, ProjectBuildRequest, ProjectBuildScope};
use crate::project_operation::ProjectOperationScope;
use crate::project_validation::{
    ProjectValidationAction, ProjectValidationAdapter, ProjectValidationRequest,
    ProjectValidationScope,
};

fn build_request(scope: ProjectBuildScope) -> ProjectBuildRequest {
    ProjectBuildRequest {
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
