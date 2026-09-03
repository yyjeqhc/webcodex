use super::*;

#[test]
fn requested_by_from_auth_uses_bootstrap_username_or_anonymous() {
    let bootstrap = auth_context(None, true);
    assert_eq!(requested_by_from_auth(Some(&bootstrap)), "bootstrap");

    let alice = auth_context(Some("alice"), false);
    assert_eq!(requested_by_from_auth(Some(&alice)), "alice");

    assert_eq!(requested_by_from_auth(None), "anonymous");
}

#[test]
fn enforce_register_owner_cases() {
    let bootstrap = auth_context(None, true);
    let user_alice = auth_context(Some("alice"), false);
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    let agent_alice = agent_auth_context(
        "alice",
        "alice-laptop",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    let agent_alice_register_only =
        agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);

    // (case, auth, client_id, owner, Ok or Err(required error fragments)).
    let cases = vec![
        // No AuthMiddleware (unit tests): defer to the middleware, which in
        // production rejects anonymous requests before the handler runs.
        (
            "no auth skips with owner",
            None,
            "client-1",
            Some("anyone"),
            Ok(()),
        ),
        (
            "no auth skips without owner",
            None,
            "client-1",
            None,
            Ok(()),
        ),
        // Bootstrap may register any owner.
        (
            "bootstrap allows missing owner",
            Some(&bootstrap),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "bootstrap allows any owner",
            Some(&bootstrap),
            "client-1",
            Some("bob"),
            Ok(()),
        ),
        (
            "shared key ignores missing owner",
            Some(&shared),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "shared key ignores untrusted owner",
            Some(&shared),
            "client-1",
            Some("forged-owner"),
            Ok(()),
        ),
        // Phase 3: user tokens (Phase 2 personal API tokens) are no longer
        // allowed on Runner transport endpoints. Only bootstrap or agent
        // tokens may register.
        (
            "user token is rejected",
            Some(&user_alice),
            "client-1",
            Some("alice"),
            Err(vec!["user tokens are not allowed"]),
        ),
        // Matching client_id + matching owner -> Ok.
        (
            "agent token matching client_id and owner",
            Some(&agent_alice),
            "alice-laptop",
            Some("alice"),
            Ok(()),
        ),
        // Matching client_id + missing owner -> Ok (owner filled in by the
        // caller via effective_register_owner).
        (
            "agent token matching client_id, missing owner",
            Some(&agent_alice),
            "alice-laptop",
            None,
            Ok(()),
        ),
        (
            "agent token wrong client_id rejected",
            Some(&agent_alice_register_only),
            "other-laptop",
            None,
            Err(vec!["not bound to client_id"]),
        ),
        (
            "agent token owner mismatch rejected",
            Some(&agent_alice_register_only),
            "alice-laptop",
            Some("bob"),
            Err(vec!["agent token owner is 'alice'", "bob"]),
        ),
    ];

    for (case, auth, client_id, owner, expected) in cases {
        let result = enforce_register_owner(auth, client_id, owner);
        match expected {
            Ok(()) => assert!(result.is_ok(), "case '{case}': got: {result:?}"),
            Err(fragments) => {
                let err = result.expect_err(&format!("case '{case}': expected an error"));
                for fragment in fragments {
                    assert!(
                        err.contains(fragment),
                        "case '{case}': missing '{fragment}' in error: {err}"
                    );
                }
            }
        }
    }
}

#[test]
fn effective_register_owner_agent_token_fills_username() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
    // Missing owner -> filled with the token's username.
    assert_eq!(
        effective_register_owner(Some(&alice), None),
        Some("alice".to_string())
    );
    // Matching owner preserved.
    assert_eq!(
        effective_register_owner(Some(&alice), Some("alice")),
        Some("alice".to_string())
    );
    // Bootstrap keeps the request owner.
    let bootstrap = auth_context(None, true);
    assert_eq!(
        effective_register_owner(Some(&bootstrap), Some("bob")),
        Some("bob".to_string())
    );
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert_eq!(
        effective_register_owner(Some(&shared), Some("forged-owner")),
        None,
        "shared-key owner must not become an authorization input"
    );
}
