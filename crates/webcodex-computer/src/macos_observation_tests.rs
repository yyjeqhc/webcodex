use super::*;

fn fingerprint(role: &str, subrole: Option<&str>, protected: bool) -> ElementFingerprint {
    ElementFingerprint {
        role: role.into(),
        subrole: subrole.map(str::to_owned),
        identifier: Some(format!("fixture-{role}")),
        title: None,
        description: None,
        placeholder: None,
        protected,
        #[cfg(windows)]
        native_runtime_id: vec![1, 2],
    }
}

fn assert_redacted(fingerprint: &ElementFingerprint) {
    assert!(fingerprint.protected);
    assert_eq!(fingerprint.title, None);
    assert_eq!(fingerprint.description, None);
    assert_eq!(fingerprint.placeholder, None);
    assert_eq!(
        ax_observed_value(fingerprint, || panic!("protected AXValue must not be read")).unwrap(),
        None
    );
}

#[test]
fn ax_observation_secure_role_and_subrole_skip_sensitive_text_reads() {
    for (role, subrole, protected) in [
        ("AXSecureTextField", None, false),
        ("AXTextField", Some("AXSecureTextField"), false),
        ("AXGroup", None, true),
    ] {
        let original = fingerprint(role, subrole, protected);
        let observed = finish_ax_fingerprint(original.clone(), |_| {
            panic!("secure or protected identity text must not be read")
        })
        .unwrap();
        assert_redacted(&observed);
        assert_eq!(observed.role, original.role);
        assert_eq!(observed.subrole, original.subrole);
        assert_eq!(observed.identifier, original.identifier);
    }
}

#[test]
fn ax_observation_secure_ancestor_survives_repeated_scoped_queries() {
    for (role, subrole) in [
        ("AXSecureTextField", None),
        ("AXTextField", Some("AXSecureTextField")),
    ] {
        let window = fingerprint("AXWindow", None, false);
        let secure =
            finish_ax_fingerprint(fingerprint(role, subrole, false), |_| Ok(None)).unwrap();
        assert_redacted(&secure);
        let child = finish_ax_fingerprint(fingerprint("AXGroup", None, secure.protected), |_| {
            panic!("descendant text must inherit protection")
        })
        .unwrap();
        assert_redacted(&child);
        let mut root = ElementRecord {
            surface_id: "surface_fixture".into(),
            path: vec![0, 0],
            lineage: vec![window, secure, child],
        };
        for _ in 0..3 {
            validate_element_state_target(&root).unwrap();
            let (path, mut ancestors, inherited) = ax_accessibility_tree_seed(Some(&root)).unwrap();
            assert!(inherited);
            let current = finish_ax_fingerprint(fingerprint("AXGroup", None, inherited), |_| {
                panic!("scoped observation must not reread protected text")
            })
            .unwrap();
            ensure_correlated_fingerprint(root.target_fingerprint().unwrap(), &current, false)
                .unwrap();
            assert_redacted(&current);
            ancestors.push(current);
            let grandchild = finish_ax_fingerprint(fingerprint("AXGroup", None, inherited), |_| {
                panic!("new scoped descendants must remain protected")
            })
            .unwrap();
            assert_redacted(&grandchild);
            ancestors.push(grandchild);
            root.path = path;
            root.path.push(0);
            root.lineage = ancestors;
        }
    }
}

#[test]
fn ax_observation_scoped_seed_checks_secure_ancestry_without_protected_flag() {
    for (role, subrole) in [
        ("AXSecureTextField", None),
        ("AXTextField", Some("AXSecureTextField")),
    ] {
        let root = ElementRecord {
            surface_id: "surface_fixture".into(),
            path: vec![0, 0],
            lineage: vec![
                fingerprint("AXWindow", None, false),
                fingerprint(role, subrole, false),
                fingerprint("AXGroup", None, false),
            ],
        };
        assert!(ax_accessibility_tree_seed(Some(&root)).unwrap().2);
    }
}

#[test]
fn ax_unverified_subrole_redacts_text_blocks_input_and_cannot_attest_identity() {
    let unverified = ElementFingerprint {
        role: "AXTextField".into(),
        subrole: None,
        identifier: None,
        title: None,
        description: None,
        placeholder: None,
        protected: true,
        #[cfg(windows)]
        native_runtime_id: vec![1, 2],
    };
    let observed = finish_ax_fingerprint(unverified, |_| {
        panic!("unknown secure subrole must prevent every text attribute read")
    })
    .unwrap();
    assert_redacted(&observed);
    assert!(!observed.has_positive_evidence());

    let element = ElementRecord {
        surface_id: "surface_unverified".into(),
        path: vec![0],
        lineage: vec![fingerprint("AXWindow", None, false), observed],
    };
    assert!(element.contains_protected_content());
    assert!(validate_element_state_target(&element).is_err());
    assert!(validate_text_input_target(&element).is_err());

    let descendant = finish_ax_fingerprint(
        fingerprint("AXGroup", None, element.contains_protected_content()),
        |_| panic!("descendant of uncertain subrole must not read text"),
    )
    .unwrap();
    assert_redacted(&descendant);

    // A separate sibling with verified metadata remains readable.
    let sibling = finish_ax_fingerprint(fingerprint("AXButton", None, false), |attribute| {
        Ok((attribute == "AXTitle").then_some("7".into()))
    })
    .unwrap();
    assert_eq!(sibling.title.as_deref(), Some("7"));
    assert!(!sibling.protected);
}

#[test]
fn ax_observation_nonsecure_text_and_provider_errors_are_preserved() {
    let mut read = Vec::new();
    let observed = finish_ax_fingerprint(fingerprint("AXGroup", None, false), |attribute| {
        read.push(attribute);
        Ok(Some(attribute.into()))
    })
    .unwrap();
    assert!(!observed.protected);
    assert_eq!(read, ["AXTitle", "AXDescription", "AXPlaceholderValue"]);
    assert_eq!(observed.title.as_deref(), Some("AXTitle"));
    assert_eq!(observed.description.as_deref(), Some("AXDescription"));
    assert_eq!(observed.placeholder.as_deref(), Some("AXPlaceholderValue"));
    assert_eq!(
        ax_observed_value(&observed, || Ok(Some("visible value".into()))).unwrap(),
        Some("visible value".into())
    );
    assert_eq!(
        ax_observed_value(
            &observed,
            || Err("accessibility_failed: value error".into())
        )
        .unwrap_err(),
        "accessibility_failed: value error"
    );
    let error = finish_ax_fingerprint(fingerprint("AXGroup", None, false), |_| {
        Err("accessibility_failed: provider error".into())
    })
    .unwrap_err();
    assert_eq!(error, "accessibility_failed: provider error");
}

#[test]
fn ax_observation_scoped_seed_retains_correlation_bounds() {
    assert_eq!(
        ax_accessibility_tree_seed(None).unwrap(),
        (vec![], vec![], false)
    );
    let mut root = ElementRecord {
        surface_id: "surface_fixture".into(),
        path: vec![0; MAX_ACCESSIBILITY_PATH_DEPTH],
        lineage: vec![fingerprint("AXGroup", None, true); MAX_ACCESSIBILITY_PATH_DEPTH + 1],
    };
    assert!(ax_accessibility_tree_seed(Some(&root)).unwrap().2);
    root.path.push(0);
    root.lineage.push(fingerprint("AXGroup", None, true));
    assert!(ax_accessibility_tree_seed(Some(&root)).is_err());
    root.path = vec![MAX_ACCESSIBILITY_NODES];
    root.lineage.truncate(2);
    assert!(ax_accessibility_tree_seed(Some(&root)).is_err());
    root.path = vec![0];
    root.lineage.pop();
    assert!(ax_accessibility_tree_seed(Some(&root)).is_err());
}
