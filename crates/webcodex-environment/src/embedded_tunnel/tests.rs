use super::*;
use crate::storage::ensure_private_directory;

fn fixture() -> (tempfile::TempDir, EnvironmentStore) {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    (temp, store)
}
fn profile(store: &EnvironmentStore, name: &str, id: &str, key: &str, proxy: Option<&str>) {
    let directory = store.root().join("server/tunnels").join(name);
    ensure_private_directory(&directory).unwrap();
    let mut value = format!("WEBCODEX_TUNNEL_PROFILE_ID={name}\nCONTROL_PLANE_TUNNEL_ID={id}\nCONTROL_PLANE_API_KEY={key}\nWEBCODEX_TOKEN=local-{name}\n");
    if let Some(proxy) = proxy {
        value.push_str(&format!("WEBCODEX_TUNNEL_PROXY={proxy}\n"));
    }
    atomic_private_write(&directory.join("webcodex.env"), value.as_bytes()).unwrap();
    atomic_private_write(&directory.join("readiness.json"), b"{}").unwrap();
}
fn records(store: &EnvironmentStore, names: &[&str]) {
    let values: Vec<_> = names
        .iter()
        .map(|name| TunnelRecord {
            profile_id: (*name).into(),
            host_mode: TunnelHostMode::Embedded,
            installed: false,
            started: false,
        })
        .collect();
    store.write_json("tunnel.json", &values).unwrap();
}
#[test]
fn independent_profile_files_bind_identity_key_local_token_and_proxy_as_a_unit() {
    let (_temp, store) = fixture();
    records(&store, &["primary", "secondary"]);
    profile(
        &store,
        "primary",
        "tunnel_one",
        "one-key",
        Some("http://user:password@127.0.0.1:7890"),
    );
    profile(&store, "secondary", "tunnel_two", "two-key", None);
    let profiles = embedded_tunnel_profiles(store.root()).unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].credentials.tunnel_id.expose(), "tunnel_one");
    assert_eq!(profiles[0].credentials.api_key.expose(), "one-key");
    assert_eq!(profiles[0].local_token.expose(), "local-primary");
    assert_eq!(profiles[1].credentials.api_key.expose(), "two-key");
    assert!(profiles[1].proxy.is_none());
    let debug = format!("{profiles:?}");
    for secret in ["one-key", "two-key", "local-primary", "password"] {
        assert!(!debug.contains(secret));
    }
}
#[test]
fn inherited_credentials_are_not_used_to_fill_missing_profile_fields() {
    // Isolated child environment, never a process-global mutation in parallel tests.
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "embedded_tunnel::tests::missing_explicit_key_is_rejected",
        ])
        .env("CONTROL_PLANE_TUNNEL_ID", "inherited-fixture")
        .env("CONTROL_PLANE_API_KEY", "inherited-fixture-key")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(result.status.success());
}
#[test]
fn missing_explicit_key_is_rejected() {
    let (_temp, store) = fixture();
    records(&store, &["primary"]);
    profile(&store, "primary", "tunnel_one", "", None);
    assert!(embedded_tunnel_profiles(store.root()).is_err());
}
#[test]
fn duplicated_names_identity_or_credential_keys_fail_closed() {
    let (_temp, store) = fixture();
    records(&store, &["first", "first"]);
    profile(&store, "first", "tunnel_one", "key", None);
    assert!(embedded_tunnel_profiles(store.root()).is_err());
    records(&store, &["first", "second"]);
    profile(&store, "second", "tunnel_one", "other-key", None);
    assert!(embedded_tunnel_profiles(store.root()).is_err());
    records(&store, &["first"]);
    let path = store.root().join("server/tunnels/first/webcodex.env");
    let mut content = read_secret(&path).unwrap().expose().to_string();
    content.push_str("\nCONTROL_PLANE_API_KEY=second-key\n");
    atomic_private_write(&path, content.as_bytes()).unwrap();
    assert!(embedded_tunnel_profiles(store.root()).is_err());
}
#[test]
fn existing_standalone_records_default_to_standalone_and_are_not_loaded() {
    let (_temp, store) = fixture();
    let value = serde_json::json!([{"profile_id":"existing","installed":true,"started":true}]);
    store.write_json("tunnel.json", &value).unwrap();
    assert_eq!(
        tunnel_profiles(&store).unwrap()[0].host_mode,
        TunnelHostMode::Standalone
    );
    // No credential file needed: the embedded loader does not read this profile.
    assert!(embedded_tunnel_profiles(store.root()).unwrap().is_empty());
}
