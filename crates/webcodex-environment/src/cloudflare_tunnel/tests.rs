use super::*;
use crate::storage::{atomic_private_write, ensure_private_directory};

fn fixture() -> (tempfile::TempDir, EnvironmentStore) {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    ensure_private_directory(&store.root().join("server")).unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_ADDR=127.0.0.1:62645\n",
    )
    .unwrap();
    (temp, store)
}

fn record(id: &str, provider: TunnelProvider, autostart: bool) -> TunnelRecord {
    TunnelRecord {
        configuration_id: None,
        profile_id: id.into(),
        provider,
        name: id.into(),
        host_mode: TunnelHostMode::Embedded,
        autostart,
        revision: 3,
        runtime_revision: 2,
        installed: false,
        started: false,
    }
}
fn named(origin: &str, id: &str) -> TunnelProvider {
    TunnelProvider::CloudflareNamed {
        public_origin: origin.into(),
        tunnel_id: id.into(),
    }
}
fn bind(store: &EnvironmentStore, record: &TunnelRecord, token: Option<&str>) -> Option<PathBuf> {
    let directory = profile_directory(store, &record.profile_id);
    ensure_private_directory(&directory).unwrap();
    let token_path = token.map(|token| {
        let directory = directory.join("tokens");
        ensure_private_directory(&directory).unwrap();
        let path = directory.join(uuid::Uuid::new_v4().to_string());
        atomic_private_write(&path, token.as_bytes()).unwrap();
        path
    });
    let token_line = token_path
        .as_ref()
        .map(|path| {
            format!(
                "CLOUDFLARE_TUNNEL_TOKEN_REF={}\n",
                path.file_name().unwrap().to_string_lossy()
            )
        })
        .unwrap_or_default();
    let kind = if matches!(record.provider, TunnelProvider::CloudflareNamed { .. }) {
        "cloudflare_named"
    } else {
        "cloudflare_quick"
    };
    atomic_private_write(
        &directory.join("webcodex.env"),
        format!(
            "WEBCODEX_TUNNEL_PROFILE_ID={}\nWEBCODEX_TUNNEL_PROVIDER={kind}\n{token_line}",
            record.profile_id
        )
        .as_bytes(),
    )
    .unwrap();
    token_path
}

#[test]
fn old_records_default_to_openai_without_migration() {
    let record: TunnelRecord = serde_json::from_value(
        serde_json::json!({"profile_id":"old","installed":true,"started":false}),
    )
    .unwrap();
    assert_eq!(record.provider, TunnelProvider::Openai);
    assert_eq!(record.host_mode, TunnelHostMode::Standalone);
}

#[test]
fn canonical_catalog_allows_saved_alternatives_but_one_cloudflare_startup_selection() {
    let primary = record("named", named("https://mcp.example.com", "id-one"), true);
    let quick = record("quick", TunnelProvider::CloudflareQuick, false);
    let openai = record("openai", TunnelProvider::Openai, true);
    validate_catalog(&[primary.clone(), quick.clone(), openai]).unwrap();
    let mut selected = quick;
    selected.autostart = true;
    assert_eq!(
        validate_catalog(&[primary.clone(), selected])
            .unwrap_err()
            .code,
        "cloudflare_selection_conflict"
    );
    let duplicate = record(
        "duplicate",
        named("https://other.example.com", "id-one"),
        false,
    );
    assert_eq!(
        validate_catalog(&[primary, duplicate]).unwrap_err().code,
        "tunnel_identity_duplicate"
    );
}

#[test]
fn named_origins_are_https_origins_and_domain_edits_keep_the_same_identity() {
    for origin in [
        "http://mcp.example.com",
        "https://user:secret@mcp.example.com",
        "https://mcp.example.com/mcp",
        "https://mcp.example.com?secret=x",
        "https://mcp.example.com/#x",
        "https://mcp.example.com/",
        "https://MCP.example.com",
        "HTTPS://mcp.example.com",
        "https://mcp.example.com:443",
    ] {
        assert_eq!(
            validate_provider(&named(origin, "id-one"))
                .unwrap_err()
                .code,
            "cloudflare_origin",
            "{origin}"
        );
    }
    validate_provider(&named("https://mcp.example.com:8443", "id-one")).unwrap();
    let old = named("https://old.example.com", "id-one");
    let updated = named("https://new.example.com", "id-one");
    validate_provider(&updated).unwrap();
    assert!(same_provider_identity(&old, &updated));
    assert!(!same_provider_identity(
        &old,
        &named("https://new.example.com", "id-two")
    ));
    assert!(!same_provider_identity(
        &old,
        &TunnelProvider::CloudflareQuick
    ));
}

#[test]
fn safe_projection_excludes_token_and_private_capability_path_and_quick_has_no_saved_origin() {
    let (_temp, store) = fixture();
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let named = record("named", named("https://mcp.example.com", "id-one"), true);
    let quick = record("quick", TunnelProvider::CloudflareQuick, false);
    let token_path = bind(&store, &named, Some("private-cloudflare-token")).unwrap();
    bind(&store, &quick, None);
    store.write_json("tunnel.json", &[named, quick]).unwrap();
    let snapshots = tunnel_profile_snapshots(&store).unwrap();
    let json = serde_json::to_string(&snapshots).unwrap();
    assert!(json.contains("https://mcp.example.com"));
    assert!(json.contains("45678"));
    assert!(!json.contains("private-cloudflare-token"));
    assert!(!json.contains(token_path.to_str().unwrap()));
    assert!(!serde_json::to_string(&snapshots[1].provider)
        .unwrap()
        .contains("origin"));
    assert!(embedded_tunnel_profiles(store.root()).unwrap().is_empty());
}

#[test]
fn token_references_cannot_escape_profile_directory_and_quick_rejects_a_token() {
    let (_temp, store) = fixture();
    let named = record("named", named("https://mcp.example.com", "id-one"), true);
    bind(&store, &named, Some("token"));
    let path = profile_directory(&store, "named").join("webcodex.env");
    atomic_private_write(&path, b"WEBCODEX_TUNNEL_PROFILE_ID=named\nWEBCODEX_TUNNEL_PROVIDER=cloudflare_named\nCLOUDFLARE_TUNNEL_TOKEN_REF=../../secret\n").unwrap();
    assert!(binding_token_file(&store, &named).is_err());
    let quick = record("quick", TunnelProvider::CloudflareQuick, false);
    bind(&store, &quick, Some("must-not-exist"));
    assert!(binding_token_file(&store, &quick).is_err());
}

#[test]
fn ingress_port_is_shared_and_never_the_management_port() {
    let (_temp, store) = fixture();
    assert!(desired_ingress_port(&store, Some(62645)).is_err());
    assert!(desired_ingress_port(&store, Some(0)).is_err());
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    assert_eq!(desired_ingress_port(&store, None).unwrap(), 45678);
    assert!(desired_ingress_port(&store, Some(45679)).is_err());
}

#[cfg(unix)]
#[test]
fn token_file_must_be_private_regular_owned_file() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, store) = fixture();
    let named = record("named", named("https://mcp.example.com", "id-one"), true);
    let token_path = bind(&store, &named, Some("token")).unwrap();
    std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(binding_token_file(&store, &named).is_err());
}

fn save_environment(store: &EnvironmentStore) {
    let binary = store.root().join("webcodex");
    let request = SetupRequest {
        service_scope: service::ServiceScope::System,
        mode: EnvironmentMode::Create {
            listen: "127.0.0.1:62645".into(),
        },
        server_url: "http://127.0.0.1:62645".into(),
        project: None,
        runner: Some(false),
        runner_display_name: None,
        account: LocalAccount {
            name: "owner".into(),
            identity: "1000".into(),
            home: store.root().to_path_buf(),
        },
        binaries: RuntimeBinaries {
            cli: binary.clone(),
            server: binary.clone(),
            runner: binary,
        },
    };
    store
        .save_environment(&EnvironmentRecord {
            schema_version: ENVIRONMENT_SCHEMA,
            environment_id: "environment".into(),
            request,
            username: Some("authenticated-owner".into()),
            runner_client_id: Some("client-one".into()),
            projects: vec![],
            configured: true,
        })
        .unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_ADDR=0.0.0.0:62645\nWEBCODEX_TOKEN=private-bootstrap\n",
    )
    .unwrap();
}

#[test]
fn runtime_materialization_binds_owner_actual_local_targets_revision_and_protected_credentials() {
    let (_temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let named = record("named", named("https://mcp.example.com", "id-one"), true);
    let mut quick = record("quick", TunnelProvider::CloudflareQuick, false);
    quick.host_mode = TunnelHostMode::Standalone;
    let token_path = bind(&store, &named, Some("private-named-token")).unwrap();
    bind(&store, &quick, None);
    store.write_json("tunnel.json", &[named, quick]).unwrap();
    let profiles = cloudflare_tunnel_profiles(store.root()).unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].owner_username, "authenticated-owner");
    assert_eq!(profiles[0].local_target, "http://127.0.0.1:45678");
    assert_eq!(profiles[0].local_server_url, "http://127.0.0.1:62645");
    assert_eq!(profiles[0].bootstrap_token.expose(), "private-bootstrap");
    assert_eq!(profiles[0].runtime_revision, 2);
    assert_eq!(profiles[0].token_file.as_ref(), Some(&token_path));
    let paths = materialize_cloudflare_tunnel_profiles(&store).unwrap();
    assert_eq!(paths.len(), 2);
    let selected = load_cloudflare_server_materializations(&store.root().join("server")).unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].runner_client_id.as_deref(), Some("client-one"));
    assert_eq!(selected[1].host_mode, TunnelHostMode::Standalone);
    assert_eq!(
        load_cloudflare_server_ingress_port(&store.root().join("server")).unwrap(),
        Some(45678)
    );
    assert_eq!(selected[0].token_file.as_ref(), Some(&token_path));
    let standalone = load_cloudflare_tunnel_materialization(&paths[1]).unwrap();
    assert_eq!(standalone.host_mode, TunnelHostMode::Standalone);
    assert!(!standalone.autostart);
    assert!(standalone.token_file.is_none());
    let snapshot = serde_json::to_string(&tunnel_profile_snapshots(&store).unwrap()).unwrap();
    assert!(!snapshot.contains("private-bootstrap"));
    assert!(!snapshot.contains("private-named-token"));
}

#[test]
fn runtime_binding_rejects_arbitrary_management_targets_and_identity_mismatch() {
    let (_temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let quick = record("quick", TunnelProvider::CloudflareQuick, false);
    bind(&store, &quick, None);
    store.write_json("tunnel.json", &[quick]).unwrap();
    let path = materialize_cloudflare_tunnel_profiles(&store)
        .unwrap()
        .remove(0);
    let content = read_secret(&path).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(content.expose()).unwrap();
    value["local_server_url"] = "http://other.example.com:62645".into();
    atomic_private_write(&path, &serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_cloudflare_tunnel_materialization(&path).is_err());
    value["local_server_url"] = "http://127.0.0.1:62645".into();
    value["profile_id"] = "other".into();
    atomic_private_write(&path, &serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_cloudflare_tunnel_materialization(&path).is_err());
}

#[test]
fn name_edits_preserve_applied_runtime_revision_and_origin_or_token_edits_advance_it() {
    let saved = record("named", named("https://old.example.com", "id-one"), true);
    let mut request = CloudflareTunnelProfileRequest {
        profile_id: "named",
        name: None,
        host_mode: TunnelHostMode::Embedded,
        autostart: true,
        expected_revision: Some(3),
        provider: saved.provider.clone(),
        token: None,
        ingress_port: None,
    };
    let (renamed, runtime_changed) =
        updated_record(Some(&saved), &request, "New name".into(), false).unwrap();
    assert_eq!(renamed.revision, 4);
    assert_eq!(renamed.runtime_revision, 2);
    assert!(!runtime_changed);
    request.provider = named("https://new.example.com", "id-one");
    let (changed, runtime_changed) =
        updated_record(Some(&saved), &request, saved.name.clone(), false).unwrap();
    assert_eq!(changed.revision, 4);
    assert_eq!(changed.runtime_revision, 3);
    assert!(runtime_changed);
    request.provider = saved.provider.clone();
    let (rotated, runtime_changed) =
        updated_record(Some(&saved), &request, saved.name.clone(), true).unwrap();
    assert_eq!(rotated.runtime_revision, 3);
    assert!(runtime_changed);
}

#[tokio::test]
async fn stale_cloudflare_writer_cannot_rotate_credentials_or_change_saved_profile() {
    let (_temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let saved = record("named", named("https://old.example.com", "id-one"), true);
    let path = bind(&store, &saved, Some("saved-token")).unwrap();
    store.write_json("tunnel.json", &[saved.clone()]).unwrap();
    let token = Secret::new("replacement-token".into());
    let request = CloudflareTunnelProfileRequest {
        profile_id: "named",
        name: Some("New"),
        host_mode: TunnelHostMode::Embedded,
        autostart: false,
        expected_revision: Some(2),
        provider: named("https://new.example.com", "id-one"),
        token: Some(&token),
        ingress_port: None,
    };
    let error = NativeEnvironment::new()
        .unwrap()
        .configure_cloudflare_tunnel_profile(&store, &request)
        .await
        .unwrap_err();
    assert_eq!(error.code, "tunnel_revision_stale");
    assert_eq!(read_secret(&path).unwrap().expose(), "saved-token");
    assert_eq!(tunnel_profiles(&store).unwrap()[0].provider, saved.provider);
}

#[test]
fn unselected_standalone_profiles_must_not_claim_native_installed_startup() {
    let mut saved = record("separate", TunnelProvider::CloudflareQuick, false);
    saved.host_mode = TunnelHostMode::Standalone;
    validate_catalog(&[saved.clone()]).unwrap();
    saved.installed = true;
    assert_eq!(
        validate_catalog(&[saved]).unwrap_err().code,
        "cloudflare_standalone_selection"
    );
}

#[cfg(unix)]
#[test]
fn unchanged_materialization_preserves_installer_file_identity() {
    use std::os::unix::fs::MetadataExt;
    let (_temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let quick = record("quick", TunnelProvider::CloudflareQuick, false);
    bind(&store, &quick, None);
    store.write_json("tunnel.json", &[quick]).unwrap();
    let path = materialize_cloudflare_tunnel_profiles(&store)
        .unwrap()
        .remove(0);
    let index = store.root().join("server/cloudflare-tunnels.json");
    let original = (
        std::fs::metadata(&path).unwrap().ino(),
        std::fs::metadata(&index).unwrap().ino(),
    );
    materialize_cloudflare_tunnel_profiles(&store).unwrap();
    assert_eq!(
        original,
        (
            std::fs::metadata(&path).unwrap().ino(),
            std::fs::metadata(&index).unwrap().ino()
        )
    );
}

#[test]
fn recreated_cloudflare_profile_gets_a_fresh_incarnation_and_edits_retain_it() {
    let request = CloudflareTunnelProfileRequest {
        profile_id: "quick",
        name: None,
        host_mode: TunnelHostMode::Embedded,
        autostart: false,
        expected_revision: None,
        provider: TunnelProvider::CloudflareQuick,
        token: None,
        ingress_port: None,
    };
    let (original, _) = updated_record(None, &request, "Quick".into(), false).unwrap();
    let (recreated, _) = updated_record(None, &request, "Quick".into(), false).unwrap();
    assert!(original.configuration_id.is_some());
    assert_ne!(original.configuration_id, recreated.configuration_id);
    let (edited, _) = updated_record(Some(&original), &request, "Renamed".into(), false).unwrap();
    assert_eq!(original.configuration_id, edited.configuration_id);
}

#[tokio::test]
async fn stale_delete_is_fenced_under_the_setup_lock_before_private_file_removal() {
    let (_temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let saved = record("named", named("https://mcp.example.com", "id-one"), true);
    let token = bind(&store, &saved, Some("retained-token")).unwrap();
    store.write_json("tunnel.json", &[saved]).unwrap();
    let backend = NativeEnvironment::new().unwrap();
    let held = store.lock().unwrap();
    assert_eq!(
        backend
            .remove_tunnel_at_revision(&store, "named", Some(2))
            .await
            .unwrap_err()
            .code,
        "setup_busy"
    );
    drop(held);
    assert_eq!(
        backend
            .remove_tunnel_at_revision(&store, "named", Some(2))
            .await
            .unwrap_err()
            .code,
        "tunnel_revision_stale"
    );
    assert_eq!(tunnel_profiles(&store).unwrap().len(), 1);
    assert_eq!(read_secret(&token).unwrap().expose(), "retained-token");
    assert_eq!(
        backend
            .remove_tunnel_at_revision(&store, "deleted", Some(2))
            .await
            .unwrap_err()
            .code,
        "tunnel_revision_stale"
    );
}

#[test]
fn retired_profile_revision_prevents_an_old_editor_from_targeting_recreation() {
    let (_temp, store) = fixture();
    // The absent-service boundary has already admitted catalog withdrawal; no
    // native service or network fixture is needed for this durability fence.
    let request = CloudflareTunnelProfileRequest {
        profile_id: "quick",
        name: None,
        host_mode: TunnelHostMode::Embedded,
        autostart: false,
        expected_revision: None,
        provider: TunnelProvider::CloudflareQuick,
        token: None,
        ingress_port: None,
    };
    let (mut original, _) = updated_record(None, &request, "Quick".into(), false).unwrap();
    original.revision = 7;
    retain_cloudflare_profile_revision(&store, &original).unwrap();
    store
        .write_json("tunnel.json", &Vec::<TunnelRecord>::new())
        .unwrap();
    let (mut recreated, _) = updated_record(None, &request, "Quick".into(), false).unwrap();
    recreated.revision = next_cloudflare_profile_revision(&store, "quick").unwrap();
    recreated.runtime_revision = recreated.revision;
    assert_eq!(recreated.revision, 8);
    assert_ne!(original.configuration_id, recreated.configuration_id);
    assert_eq!(
        crate::tunnel::validate_expected_revision(Some(&recreated), Some(7))
            .unwrap_err()
            .code,
        "tunnel_revision_stale"
    );
    retain_cloudflare_profile_revision(&store, &recreated).unwrap();
    assert_eq!(
        next_cloudflare_profile_revision(&store, "quick").unwrap(),
        9
    );
}

#[test]
fn revision_tombstone_overflow_and_corruption_fail_closed() {
    let (_temp, store) = fixture();
    let directory = profile_directory(&store, "quick");
    ensure_private_directory(&directory).unwrap();
    let path = directory.join("revision.json");
    atomic_private_write(
        &path,
        &serde_json::to_vec(&RevisionTombstone {
            last_revision: u64::MAX,
            pending_cleanup: None,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(next_cloudflare_profile_revision(&store, "quick").is_err());
    atomic_private_write(&path, b"invalid").unwrap();
    assert!(next_cloudflare_profile_revision(&store, "quick").is_err());
}

#[tokio::test]
async fn deletion_fence_rejects_replaced_environment_and_cross_provider_incarnation() {
    let (_temp, store) = fixture();
    save_environment(&store);
    let saved = record("named", named("https://mcp.example.com", "id-one"), true);
    let token = bind(&store, &saved, Some("retained-token")).unwrap();
    store.write_json("tunnel.json", &[saved.clone()]).unwrap();
    let backend = NativeEnvironment::new().unwrap();
    assert_eq!(
        backend
            .remove_tunnel_fenced(
                &store,
                "named",
                Some(3),
                Some("previous-environment"),
                Some("legacy:named")
            )
            .await
            .unwrap_err()
            .code,
        "environment_changed"
    );
    let mut replacement = saved;
    replacement.provider = TunnelProvider::Openai;
    store.write_json("tunnel.json", &[replacement]).unwrap();
    assert_eq!(
        backend
            .remove_tunnel_fenced(
                &store,
                "named",
                Some(3),
                Some("environment"),
                Some("legacy:named")
            )
            .await
            .unwrap_err()
            .code,
        "tunnel_revision_stale"
    );
    assert_eq!(read_secret(&token).unwrap().expose(), "retained-token");
    assert_eq!(tunnel_profiles(&store).unwrap().len(), 1);
}

fn saved_cloudflare_control_fixture() -> (tempfile::TempDir, EnvironmentStore, TunnelRecord, PathBuf)
{
    let (temp, store) = fixture();
    save_environment(&store);
    store
        .write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port: 45678 },
        )
        .unwrap();
    let mut saved = record("named", named("https://mcp.example.com", "id-one"), true);
    saved.configuration_id = Some(uuid::Uuid::new_v4().to_string());
    saved.host_mode = TunnelHostMode::Standalone;
    let token = bind(&store, &saved, Some("retained-token")).unwrap();
    store.write_json("tunnel.json", &[saved.clone()]).unwrap();
    (temp, store, saved, token)
}

#[tokio::test]
async fn standalone_control_rechecks_observed_identity_under_lock_before_service_effects() {
    let (_temp, store, saved, token) = saved_cloudflare_control_fixture();
    let observed = cloudflare_tunnel_profile(&store, &saved.profile_id).unwrap();
    let environment = store.load_environment().unwrap().unwrap();
    let backend = NativeEnvironment::new().unwrap();
    let held = store.lock().unwrap();
    assert_eq!(
        backend
            .control_cloudflare_tunnel(
                &store,
                &environment.environment_id,
                &observed,
                ServiceOperation::Start,
            )
            .await
            .unwrap_err()
            .code,
        "setup_busy"
    );
    drop(held);
    for change in [
        "revision",
        "incarnation",
        "environment",
        "provider",
        "host_mode",
    ] {
        let mut current = saved.clone();
        let mut current_environment = environment.clone();
        match change {
            "revision" => current.revision += 1,
            "incarnation" => current.configuration_id = Some(uuid::Uuid::new_v4().to_string()),
            "environment" => current_environment.environment_id = "replacement-environment".into(),
            "provider" => current.provider = TunnelProvider::CloudflareQuick,
            "host_mode" => current.host_mode = TunnelHostMode::Embedded,
            _ => unreachable!(),
        }
        store.write_json("tunnel.json", &[current]).unwrap();
        store.save_environment(&current_environment).unwrap();
        for operation in [ServiceOperation::Start, ServiceOperation::Stop] {
            let error = backend
                .control_cloudflare_tunnel(
                    &store,
                    &environment.environment_id,
                    &observed,
                    operation,
                )
                .await
                .unwrap_err();
            assert_eq!(
                error.code,
                if change == "environment" {
                    "environment_changed"
                } else {
                    "tunnel_revision_stale"
                },
                "{change}"
            );
        }
        assert_eq!(read_secret(&token).unwrap().expose(), "retained-token");
        assert!(!profile_directory(&store, &saved.profile_id)
            .join("runtime.json")
            .exists());
        assert!(!store.root().join("server/cloudflare-tunnels.json").exists());
    }
}

#[tokio::test]
async fn retirement_intent_fences_live_catalog_before_withdrawal_and_rejects_replacement() {
    let (_temp, store, saved, token) = saved_cloudflare_control_fixture();
    let observed = cloudflare_tunnel_profile(&store, &saved.profile_id).unwrap();
    let runtime = materialize_cloudflare_tunnel_profiles(&store)
        .unwrap()
        .remove(0);
    let backend = NativeEnvironment::new().unwrap();
    let held = store.lock().unwrap();
    // Model a crash after the owner gate and durable intent, before catalog commit.
    begin_cloudflare_profile_cleanup(&store, &held, "environment", &saved).unwrap();
    drop(held);
    assert_eq!(tunnel_profiles(&store).unwrap(), vec![saved.clone()]);
    assert_eq!(
        backend
            .set_tunnel_host(&store, &saved.profile_id, saved.host_mode)
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        cloudflare_tunnel_profile(&store, &saved.profile_id)
            .err()
            .unwrap()
            .code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        load_cloudflare_tunnel_materialization(&runtime)
            .err()
            .unwrap()
            .code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        embedded_tunnel_profiles(store.root()).unwrap_err().code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        materialize_cloudflare_tunnel_profiles(&store)
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    let request = CloudflareTunnelProfileRequest {
        profile_id: &saved.profile_id,
        name: Some("Changed while deletion is pending"),
        host_mode: saved.host_mode,
        autostart: saved.autostart,
        expected_revision: Some(saved.revision),
        provider: saved.provider.clone(),
        token: None,
        ingress_port: None,
    };
    assert_eq!(
        backend
            .configure_cloudflare_tunnel_profile(&store, &request)
            .await
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        backend
            .control_cloudflare_tunnel(&store, "environment", &observed, ServiceOperation::Start,)
            .await
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    let mut replacement = saved.clone();
    replacement.configuration_id = Some(uuid::Uuid::new_v4().to_string());
    store
        .write_json("tunnel.json", &[replacement.clone()])
        .unwrap();
    assert_eq!(
        backend
            .remove_tunnel_fenced(
                &store,
                &replacement.profile_id,
                Some(replacement.revision),
                Some("environment"),
                replacement.configuration_id.as_deref(),
            )
            .await
            .unwrap_err()
            .code,
        "tunnel_revision_stale"
    );
    assert_eq!(tunnel_profiles(&store).unwrap(), vec![replacement]);
    assert_eq!(read_secret(&token).unwrap().expose(), "retained-token");
}

#[tokio::test]
async fn withdrawn_profile_cleanup_retains_exact_recovery_and_resumes_after_local_failure() {
    let (_temp, store, saved, token) = saved_cloudflare_control_fixture();
    let directory = profile_directory(&store, &saved.profile_id);
    let runtime = materialize_cloudflare_tunnel_profiles(&store)
        .unwrap()
        .remove(0);
    let unknown = directory.join("tokens/unknown-private-entry");
    let interrupted_write = directory.join(format!(".setup-{}", uuid::Uuid::new_v4().simple()));
    atomic_private_write(&interrupted_write, b"private-bootstrap-canary").unwrap();
    let lifecycle_log = directory.join(service::SERVICE_LOG_NAME);
    atomic_private_write(&lifecycle_log, b"retained-lifecycle-events").unwrap();
    let held = store.lock().unwrap();
    // The native stopped-owner gate precedes this filesystem transaction.
    begin_cloudflare_profile_cleanup(&store, &held, "environment", &saved).unwrap();
    store
        .write_json("tunnel.json", &Vec::<TunnelRecord>::new())
        .unwrap();
    let error =
        complete_cloudflare_profile_cleanup(&store, &held, "environment", &saved).unwrap_err();
    assert_eq!(error.code, "cloudflare_token");
    assert!(tunnel_profiles(&store).unwrap().is_empty());
    assert_eq!(read_secret(&token).unwrap().expose(), "retained-token");
    assert_eq!(
        read_secret(&interrupted_write).unwrap().expose(),
        "private-bootstrap-canary"
    );
    // Resolve only the injected root artifact, then independently inject an
    // unknown token entry and prove that failure also preserves the intent.
    std::fs::remove_file(&interrupted_write).unwrap();
    atomic_private_write(&unknown, b"do-not-remove-unrecognized-files").unwrap();
    assert_eq!(
        complete_cloudflare_profile_cleanup(&store, &held, "environment", &saved)
            .unwrap_err()
            .code,
        "cloudflare_token"
    );
    assert_eq!(
        read_secret(&unknown).unwrap().expose(),
        "do-not-remove-unrecognized-files"
    );
    assert_eq!(
        pending_cloudflare_profile_cleanup(&store, &saved.profile_id)
            .unwrap()
            .unwrap()
            .profile,
        saved
    );
    drop(held);
    let backend = NativeEnvironment::new().unwrap();
    for (revision, environment, configuration, expected_error) in [
        (
            saved.revision + 1,
            "environment",
            saved.configuration_id.as_deref().unwrap(),
            "tunnel_revision_stale",
        ),
        (
            saved.revision,
            "replaced-environment",
            saved.configuration_id.as_deref().unwrap(),
            "environment_changed",
        ),
        (
            saved.revision,
            "environment",
            "another-incarnation",
            "tunnel_revision_stale",
        ),
    ] {
        assert_eq!(
            backend
                .remove_tunnel_fenced(
                    &store,
                    &saved.profile_id,
                    Some(revision),
                    Some(environment),
                    Some(configuration),
                )
                .await
                .unwrap_err()
                .code,
            expected_error
        );
        assert!(token.exists());
        assert!(unknown.exists());
    }
    let replacement_token = Secret::new("replacement-token".into());
    let request = CloudflareTunnelProfileRequest {
        profile_id: &saved.profile_id,
        name: None,
        host_mode: saved.host_mode,
        autostart: saved.autostart,
        expected_revision: None,
        provider: saved.provider.clone(),
        token: Some(&replacement_token),
        ingress_port: None,
    };
    assert_eq!(
        backend
            .configure_cloudflare_tunnel_profile(&store, &request)
            .await
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    assert_eq!(
        next_cloudflare_profile_revision(&store, &saved.profile_id)
            .unwrap_err()
            .code,
        "tunnel_profile_recovery_required"
    );
    // Resolve only the test-injected obstruction. Simulate a prior cleanup step
    // having completed before another crash; the retry tolerates missing files.
    std::fs::remove_file(&unknown).unwrap();
    std::fs::remove_file(directory.join("webcodex.env")).unwrap();
    let held = store.lock().unwrap();
    complete_cloudflare_profile_cleanup(&store, &held, "environment", &saved).unwrap();
    assert!(!token.exists());
    assert!(!runtime.exists());
    assert!(!directory.join("tokens").exists());
    assert!(directory.join("revision.json").exists());
    assert_eq!(
        read_secret(&lifecycle_log).unwrap().expose(),
        "retained-lifecycle-events"
    );
    assert!(
        pending_cloudflare_profile_cleanup(&store, &saved.profile_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        next_cloudflare_profile_revision(&store, &saved.profile_id).unwrap(),
        saved.revision + 1
    );
    assert!(
        load_cloudflare_server_materializations(&store.root().join("server"))
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn pending_retirement_record_remains_private_and_bounded() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, store, saved, _) = saved_cloudflare_control_fixture();
    let held = store.lock().unwrap();
    begin_cloudflare_profile_cleanup(&store, &held, "environment", &saved).unwrap();
    let path = profile_directory(&store, &saved.profile_id).join("revision.json");
    let original = crate::storage::read_private(&path).unwrap();
    assert!(original.len() < MAX_REVISION_TOMBSTONE_BYTES);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(pending_cloudflare_profile_cleanup(&store, &saved.profile_id).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    atomic_private_write(&path, &vec![b' '; MAX_REVISION_TOMBSTONE_BYTES + 1]).unwrap();
    assert_eq!(
        pending_cloudflare_profile_cleanup(&store, &saved.profile_id)
            .err()
            .unwrap()
            .code,
        "tunnel_revision"
    );
    atomic_private_write(&path, &original).unwrap();
    assert!(
        pending_cloudflare_profile_cleanup(&store, &saved.profile_id)
            .unwrap()
            .is_some()
    );
}
