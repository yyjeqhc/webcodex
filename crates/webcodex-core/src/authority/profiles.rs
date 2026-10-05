//! Closed issuance/consent profiles shared by Server, CLI and local setup.
//! A supported-scope registry is not a default grant. Existing credentials keep
//! their materialized scopes; importing these lists never updates a grant.
use super::*;

pub const LOCAL_USER: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_RUNNER_MANAGE,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
];
pub const SHARED_KEY_MODEL: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_RUNNER_MANAGE,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_MEMORY_READ,
    SCOPE_MEMORY_MANAGE,
    SCOPE_COMMUNICATION_READ,
    SCOPE_COMMUNICATION_MANAGE,
    SCOPE_JOB_RUN,
    SCOPE_COMPUTER_READ,
    SCOPE_COMPUTER_CONTROL,
];
/// Explicit Browser delegation, independent of Computer consent.
pub const OPTIONAL_BROWSER: &[&str] = &[
    SCOPE_BROWSER_READ,
    SCOPE_BROWSER_CONTROL,
    SCOPE_BROWSER_LAUNCH,
];
pub const OPTIONAL_COMPUTER: &[&str] = &[
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
];
pub const SHARED_KEY_COMPUTER: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_RUNNER_MANAGE,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_MEMORY_READ,
    SCOPE_MEMORY_MANAGE,
    SCOPE_COMMUNICATION_READ,
    SCOPE_COMMUNICATION_MANAGE,
    SCOPE_JOB_RUN,
    SCOPE_COMPUTER_READ,
    SCOPE_COMPUTER_CONTROL,
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
];

/// Scope membership only. Subject binding, client ownership and delegation
/// checks remain in their existing auth layers. Empty/duplicates fail closed.
pub fn is_baseline(scopes: &[String]) -> bool {
    !scopes.is_empty()
        && unique(scopes)
        && scopes
            .iter()
            .all(|scope| SHARED_KEY_MODEL.contains(&scope.as_str()))
}
pub fn is_computer(scopes: &[String]) -> bool {
    unique(scopes)
        && scopes
            .iter()
            .any(|scope| SHARED_KEY_MODEL.contains(&scope.as_str()))
        && scopes
            .iter()
            .all(|scope| SHARED_KEY_COMPUTER.contains(&scope.as_str()))
        && OPTIONAL_COMPUTER
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}
pub fn with_computer(scopes: &[String]) -> Option<Vec<String>> {
    if is_computer(scopes) {
        return Some(scopes.to_vec());
    }
    if !is_baseline(scopes) {
        return None;
    }
    // Explicit opt-in adds only the selected class, never fills in a narrow
    // existing grant's other baseline capabilities.
    Some(
        scopes
            .iter()
            .cloned()
            .chain(OPTIONAL_COMPUTER.iter().map(|s| s.to_string()))
            .collect(),
    )
}
fn unique(scopes: &[String]) -> bool {
    scopes
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == scopes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_closed_and_transports_never_enter_model_defaults() {
        for profile in [LOCAL_USER, SHARED_KEY_MODEL, SHARED_KEY_COMPUTER] {
            assert_eq!(
                profile
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                profile.len()
            );
            assert!(profile.iter().all(|scope| KNOWN_SCOPES.contains(scope)));
            for forbidden in [
                SCOPE_ADMIN,
                SCOPE_ACCOUNT_MANAGE,
                SCOPE_JOB_DETACH,
                SCOPE_PLUGIN_MANAGE,
            ] {
                assert!(!profile.contains(&forbidden));
            }
            assert!(!profile.iter().any(|scope| AGENT_SCOPES.contains(scope)));
        }
        assert_eq!(
            SHARED_KEY_COMPUTER,
            [SHARED_KEY_MODEL, OPTIONAL_COMPUTER].concat()
        );
    }
    #[test]
    fn narrow_historical_grants_stay_narrow_without_implicit_expansion() {
        let prior = vec![SCOPE_PROJECT_READ.to_string()];
        let granted = with_computer(&prior).unwrap();
        assert_eq!(granted.len(), 1 + OPTIONAL_COMPUTER.len());
        assert!(!granted.iter().any(|s| s == SCOPE_RUNNER_MANAGE));
        assert_eq!(with_computer(&granted), Some(granted.clone()));
        assert!(with_computer(&[]).is_none());
        assert!(with_computer(&vec![SCOPE_ADMIN.into()]).is_none());
        assert!(
            with_computer(&vec![SCOPE_PROJECT_READ.into(), SCOPE_PROJECT_READ.into()]).is_none()
        );
        assert!(!is_computer(
            &OPTIONAL_COMPUTER
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        ));
    }
}
