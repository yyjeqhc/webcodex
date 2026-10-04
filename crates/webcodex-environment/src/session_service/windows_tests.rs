use super::*;
use crate::process::CommandOutputExt;
use crate::service::current_account;
use std::process::Command;

fn fixture(dir: &Path) -> (ServiceSpec, HelperPlan) {
    let mut spec = super::tests::spec(dir);
    let account = current_account().unwrap();
    spec.account = ServiceAccount::SystemUser {
        name: account.name,
        group: None,
        expected_identity: account.identity,
        home: Some(dir.to_path_buf()),
    };
    spec.config_identity = uuid::Uuid::new_v4().to_string();
    let helper = plan(&spec, dir).unwrap();
    (spec, helper)
}

#[test]
fn windows_helper_accepts_same_owner_aliases_and_missing_default_run_level() {
    let dir = tempfile::tempdir().unwrap();
    let (_, helper) = fixture(dir.path());
    let xml = render_task(&helper).unwrap();
    let sid_element = format!("<UserId>{}</UserId>", xml_escape(&helper.account_identity));
    let aliases = [
        helper.account_name.clone(),
        helper.account_name.rsplit('\\').next().unwrap().to_owned(),
        helper.account_identity.clone(),
    ];
    for principal in &aliases {
        for trigger in &aliases {
            let mixed = xml.replacen(
                &sid_element,
                &format!("<UserId>{}</UserId>", xml_escape(trigger)),
                1,
            );
            let (before, after) = mixed.rsplit_once(&sid_element).unwrap();
            let mixed = format!("{before}<UserId>{}</UserId>{after}", xml_escape(principal));
            assert!(owned_task(&helper, &mixed), "same-owner aliases rejected");
            assert!(
                owned_task(
                    &helper,
                    &mixed.replace("<RunLevel>LeastPrivilege</RunLevel>", "")
                ),
                "default run level rejected"
            );
        }
    }
}

#[test]
fn windows_helper_rejects_foreign_unresolved_and_changed_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let (_, helper) = fixture(dir.path());
    let xml = render_task(&helper).unwrap();
    let sid_element = format!("<UserId>{}</UserId>", xml_escape(&helper.account_identity));
    let foreign = if helper.account_identity == "S-1-5-18" {
        "S-1-5-19"
    } else {
        "S-1-5-18"
    };
    for invalid in [
        "",
        "WebCodexMissingAccount_97e59d8f",
        "S-1-invalid",
        foreign,
    ] {
        let altered = xml.replacen(
            &sid_element,
            &format!("<UserId>{}</UserId>", xml_escape(invalid)),
            1,
        );
        assert!(!owned_task(&helper, &altered));
        let altered = xml.replacen(
            &sid_element,
            &format!("<UserId>{}</UserId>", xml_escape(&helper.account_name)),
            1,
        );
        let altered = altered.replacen(
            &sid_element,
            &format!("<UserId>{}</UserId>", xml_escape(invalid)),
            1,
        );
        assert!(!owned_task(&helper, &altered));
    }
    for (from, to) in [
        ("LeastPrivilege", "HighestAvailable"),
        ("LeastPrivilege", "unknown"),
        ("InteractiveToken", "Password"),
        ("--computer-session-helper", "--other-mode"),
        (
            "<RunLevel>LeastPrivilege</RunLevel>",
            "<RunLevel/><RunLevel>LeastPrivilege</RunLevel>",
        ),
    ] {
        assert!(!owned_task(&helper, &xml.replace(from, to)));
    }
    assert!(!owned_task(
        &helper,
        &xml.replace(&helper.marker, "foreign")
    ));
    assert!(!owned_task(
        &helper,
        &xml.replace("webcodex-runner.exe", "other.exe")
    ));
}

fn xml_escape(value: &str) -> String {
    super::xml(value)
}

#[test]
#[ignore = "registers and deletes one disposable Windows Task Scheduler task; never starts it"]
fn windows_helper_task_scheduler_roundtrip_is_owned() {
    let dir = tempfile::tempdir().unwrap();
    let (spec, helper) = fixture(dir.path());
    let source = render_task(&helper).unwrap();
    let path = dir.path().join("roundtrip.xml");
    let bytes: Vec<u8> = std::iter::once(0xFEFF_u16)
        .chain(source.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(&path, bytes).unwrap();
    // The executable does not exist and /Run is never invoked. No credentials,
    // service, Runner connection or existing task participates in this fixture.
    let registered = Command::new("schtasks.exe")
        .args(["/Create", "/TN", &helper.id, "/XML", path.to_str().unwrap()])
        .bounded_output()
        .unwrap();
    if !registered.status.success() {
        panic!(
            "disposable task registration failed: {}",
            String::from_utf8_lossy(&registered.stderr)
        );
    }
    let inspected = inspect_session_helper(&spec, dir.path());
    // Clean up before asserting the production check, including the unfixed case.
    let deleted = Command::new("schtasks.exe")
        .args(["/Delete", "/TN", &helper.id, "/F"])
        .bounded_output()
        .unwrap();
    assert!(deleted.status.success(), "disposable task cleanup failed");
    assert_eq!(inspected.unwrap().ownership, Ownership::Owned);
}
