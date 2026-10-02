use super::*;

#[test]
fn scm_local_spelling_is_bound_to_the_spec_local_principal() {
    assert_eq!(
        scm_local_account(r"HOST\user", r".\USER", "host"),
        Some(r"HOST\user")
    );
    for expected in [r"DOMAIN\user", "user", r"HOST\other", r".\user"] {
        assert_eq!(scm_local_account(expected, r".\user", "HOST"), None);
    }
    assert_eq!(scm_local_account(r"HOST\user", r".\other", "HOST"), None);
}

#[test]
fn native_scm_local_user_ownership_uses_sid() {
    let current = current_account().unwrap();
    let computer = local_computer_name().unwrap();
    let (domain, user) = current.name.split_once('\\').unwrap();
    // Hosted Windows CI uses a local account. Domain users cannot stand in for
    // this regression: do not normalize their name to a different local user.
    assert!(
        domain.eq_ignore_ascii_case(&computer),
        "run this native regression as a local Windows user"
    );
    let mut spec = ServiceSpec {
        scope: ServiceScope::System,
        id: "WebCodexRunner-ownership-test".into(),
        component: Component::Runner,
        program: PathBuf::from(r"C:\WebCodex\webcodex-runner.exe"),
        args: vec![],
        working_directory: current.home.clone(),
        account: ServiceAccount::SystemUser {
            name: current.name.clone(),
            group: None,
            expected_identity: current.identity,
            home: Some(current.home),
        },
        config_identity: "test-environment".into(),
        env_file: None,
        environment: Default::default(),
        linux_socket: None,
    };
    let observed = format!(r".\{user}");
    let binary = command_line(&spec).unwrap();
    let description = ownership_marker(&spec);
    assert!(configuration_owned_by_spec(&spec, &binary, &observed, &description).unwrap());
    assert!(!configuration_owned_by_spec(&spec, "foreign", &observed, &description).unwrap());
    assert!(!configuration_owned_by_spec(&spec, &binary, &observed, "foreign").unwrap());
    if let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &mut spec.account
    {
        *expected_identity = "S-1-5-18".into();
    }
    assert!(
        !configuration_owned_by_spec(&spec, &binary, &observed, &ownership_marker(&spec)).unwrap()
    );
    assert!(configuration_owned_by_spec(
        &spec,
        &binary,
        r"WebCodexNonexistentDomain\missing",
        &ownership_marker(&spec)
    )
    .is_err());
}
