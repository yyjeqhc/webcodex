use super::*;
use crate::unified_update::{PackageFormat, RuntimePlatform};
use crate::upgrade::{tests::fixture, upgrade_observation, upgrade_status, Phase};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FrozenLegacyPhase {
    Prepared,
    Stopping,
    Stopped,
    SnapshotReady,
    Verifying,
    Committed,
    Restoring,
    RolledBack,
    RecoveryRequired,
}
// Historical readers ignore additive fields and unguarded finish does not
// check schema_version. Its closed Phase decoder is the required rejection.
#[derive(Deserialize)]
struct FrozenLegacyJournal {
    schema_version: u16,
    phase: FrozenLegacyPhase,
}

fn runtime(phase: Phase) -> UpgradeJournal {
    let mut journal = fixture(std::path::Path::new("/fixture"), phase);
    journal.schema_version = 2;
    journal.candidate.package_flavor = PackageFlavor::Runtime;
    journal.candidate.platform = "linux-x64".into();
    journal.installer_target = Some(InstallerTarget::runtime(
        RuntimePlatform::LinuxX64,
        PackageFormat::Deb,
    ));
    journal
}

#[test]
fn full_schema_one_keeps_exact_legacy_wire_and_roundtrips() {
    let journal = fixture(std::path::Path::new("/fixture"), Phase::SnapshotReady);
    let encoded = serde_json::to_string(&journal).unwrap();
    assert_eq!(encoded, serde_json::to_string(&Fields(&journal)).unwrap());
    let legacy: FrozenLegacyJournal = serde_json::from_str(&encoded).unwrap();
    assert_eq!(legacy.schema_version, 1);
    assert_eq!(legacy.phase, FrozenLegacyPhase::SnapshotReady);
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert!(value["candidate"].get("package_flavor").is_none());
    assert!(value.get("installer_target").is_none());
    assert_eq!(value["phase"], "snapshot_ready");
    let decoded: UpgradeJournal = serde_json::from_str(&encoded).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
}

#[test]
fn every_runtime_phase_rejects_frozen_legacy_reader_and_current_roundtrips() {
    for phase in [
        Phase::Prepared,
        Phase::Stopping,
        Phase::Stopped,
        Phase::SnapshotReady,
        Phase::Verifying,
        Phase::Committed,
        Phase::Restoring,
        Phase::RolledBack,
        Phase::RecoveryRequired,
    ] {
        let mut journal = runtime(phase);
        journal.rollback_from = Some(Phase::Stopping);
        let encoded = serde_json::to_string(&journal).unwrap();
        assert!(serde_json::from_str::<FrozenLegacyJournal>(&encoded).is_err());
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["candidate"]["package_flavor"], "runtime");
        assert_eq!(value["installer_target"]["platform"], "linux-x64");
        assert_eq!(value["installer_target"]["format"], "deb");
        let ordinary = serde_json::to_value(phase).unwrap();
        assert_eq!(
            value["phase"],
            format!("runtime_{}", ordinary.as_str().unwrap())
        );
        assert_eq!(value["rollback_from"], "stopping");
        let decoded: UpgradeJournal = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.phase, phase);
        assert_eq!(decoded.rollback_from, Some(Phase::Stopping));
        assert_eq!(decoded.schema_version, 2);
        assert_eq!(decoded.installer_target, journal.installer_target);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    }
}

#[test]
fn schema_flavor_prefix_and_mandatory_field_mismatches_fail_closed() {
    let valid = serde_json::to_value(runtime(Phase::Prepared)).unwrap();
    let decode = |value: Value| serde_json::from_value::<UpgradeJournal>(value);
    for (key, value) in [
        ("schema_version", serde_json::json!(1)),
        ("schema_version", serde_json::json!(3)),
        ("phase", serde_json::json!("prepared")),
        ("phase", serde_json::json!("runtime_unknown")),
        ("phase", serde_json::json!("runtime_runtime_prepared")),
    ] {
        let mut malformed = valid.clone();
        malformed[key] = value;
        assert!(decode(malformed).is_err());
    }
    for flavor in [
        serde_json::json!("full"),
        serde_json::json!("unknown"),
        Value::Null,
    ] {
        let mut malformed = valid.clone();
        malformed["candidate"]["package_flavor"] = flavor;
        assert!(decode(malformed).is_err());
    }
    let mut absent_flavor = valid.clone();
    absent_flavor["candidate"]
        .as_object_mut()
        .unwrap()
        .remove("package_flavor");
    assert!(decode(absent_flavor).is_err());
    for field in [
        "schema_version",
        "installer_target",
        "operation_id",
        "candidate",
        "phase",
        "record",
        "services",
        "programs",
    ] {
        let mut incomplete = valid.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(decode(incomplete).is_err(), "missing {field}");
    }
    let mut full =
        serde_json::to_value(fixture(std::path::Path::new("/fixture"), Phase::Prepared)).unwrap();
    full["phase"] = serde_json::json!("runtime_prepared");
    assert!(decode(full).is_err());
    let mut wrong_internal_schema = runtime(Phase::Prepared);
    wrong_internal_schema.schema_version = 1;
    assert!(serde_json::to_vec(&wrong_internal_schema).is_err());
}

#[test]
fn installer_target_is_required_and_bound_to_runtime_candidate() {
    let journal = runtime(Phase::Prepared);
    let valid = serde_json::to_value(&journal).unwrap();
    for target in [
        Value::Null,
        serde_json::json!({}),
        serde_json::to_value(InstallerTarget::new(
            RuntimePlatform::LinuxX64,
            PackageFormat::Deb,
        ))
        .unwrap(),
        serde_json::to_value(InstallerTarget::runtime(
            RuntimePlatform::LinuxArm64,
            PackageFormat::Deb,
        ))
        .unwrap(),
        serde_json::to_value(InstallerTarget::runtime(
            RuntimePlatform::LinuxX64,
            PackageFormat::Pkg,
        ))
        .unwrap(),
        serde_json::to_value(InstallerTarget::runtime(
            RuntimePlatform::DarwinX64,
            PackageFormat::Pkg,
        ))
        .unwrap(),
    ] {
        let mut malformed = valid.clone();
        malformed["installer_target"] = target;
        assert!(serde_json::from_value::<UpgradeJournal>(malformed).is_err());
    }
    let mut mismatched_platform = valid;
    mismatched_platform["candidate"]["platform"] = serde_json::json!("linux-arm64");
    assert!(serde_json::from_value::<UpgradeJournal>(mismatched_platform).is_err());

    let mut missing = journal.clone();
    missing.installer_target = None;
    assert!(serde_json::to_value(&missing).is_err());
    let mut mismatched = journal.clone();
    mismatched.candidate.platform = "linux-arm64".into();
    assert!(serde_json::to_value(&mismatched).is_err());
    let mut full = fixture(std::path::Path::new("/fixture"), Phase::Prepared);
    full.installer_target = journal.installer_target;
    assert!(serde_json::to_value(&full).is_err());
    let mut full_wire = serde_json::to_value(Fields(&full)).unwrap();
    assert!(serde_json::from_value::<UpgradeJournal>(full_wire.clone()).is_err());
    full_wire["installer_target"] = Value::Null;
    assert!(serde_json::from_value::<UpgradeJournal>(full_wire).is_ok());
}

#[test]
fn duplicate_discriminators_and_nested_fields_are_rejected() {
    let encoded = serde_json::to_string(&runtime(Phase::Prepared)).unwrap();
    for duplicate in [
        encoded.replacen(
            "\"schema_version\":2",
            "\"schema_version\":2,\"schema_version\":2",
            1,
        ),
        encoded.replacen(
            "\"phase\":\"runtime_prepared\"",
            "\"phase\":\"runtime_prepared\",\"phase\":\"runtime_prepared\"",
            1,
        ),
        encoded.replacen(
            "\"package_flavor\":\"runtime\"",
            "\"package_flavor\":\"runtime\",\"package_flavor\":\"runtime\"",
            1,
        ),
        encoded.replacen(
            "\"format\":\"deb\"",
            "\"format\":\"deb\",\"format\":\"deb\"",
            1,
        ),
    ] {
        assert_ne!(duplicate, encoded);
        assert!(serde_json::from_str::<UpgradeJournal>(&duplicate).is_err());
    }
}

#[test]
fn current_persisted_runtime_journal_is_readable_by_status_and_recovery_observation() {
    let temp = crate::test_tempdir().unwrap();
    let store = crate::EnvironmentStore::open(temp.path().join("environment")).unwrap();
    drop(store.lock().unwrap());
    let mut journal = runtime(Phase::Restoring);
    journal.record.request.account = crate::current_account().unwrap();
    journal.record.request.account.home = temp.path().into();
    store.save_environment(&journal.record).unwrap();
    crate::upgrade::save(&store, &journal).unwrap();
    let bytes = std::fs::read(store.root().join("upgrade.json")).unwrap();
    assert!(serde_json::from_slice::<FrozenLegacyJournal>(&bytes).is_err());
    let recovered: UpgradeJournal = store.read_json("upgrade.json").unwrap().unwrap();
    assert_eq!(recovered.phase, Phase::Restoring);
    assert_eq!(recovered.schema_version, 2);
    assert_eq!(recovered.installer_target, journal.installer_target);
    let status = upgrade_status(&store).unwrap().unwrap();
    assert_eq!(status.package_flavor, PackageFlavor::Runtime);
    assert_eq!(status.phase, crate::upgrade::UpgradePhase::Restoring);
    let observation = upgrade_observation(&store).unwrap().unwrap();
    assert_eq!(observation.package_flavor, PackageFlavor::Runtime);
    assert_eq!(
        observation.outcome,
        crate::upgrade::UpgradeOutcome::RecoveryRequired
    );
    assert_eq!(
        std::fs::read(store.root().join("upgrade.json")).unwrap(),
        bytes
    );
}
