//! Environment-sensitive assertions run in an exact child test, with environment
//! supplied before process startup. No set_var/remove_var occurs in libtest.
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use webcodex_process::ManagedChild;

const CASE: &str = "WEBCODEX_ISOLATED_TEST_CASE";
const COMPLETED: &str = "webcodex-isolated-env-assertions-completed";

#[derive(Default)]
pub(crate) struct IsolatedEnv {
    values: Vec<(&'static str, Option<OsString>)>,
}

impl IsolatedEnv {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set(mut self, name: &'static str, value: impl AsRef<OsStr>) -> Self {
        self.values.push((name, Some(value.as_ref().to_owned())));
        self
    }

    pub(crate) fn remove(mut self, name: &'static str) -> Self {
        self.values.push((name, None));
        self
    }

    /// Multiple cases in one test are independent child invocations. Only the
    /// selected case executes assertions; no branch can silently pass zero tests.
    pub(crate) fn run(self, case: &str, assertions: impl FnOnce()) {
        let thread = std::thread::current();
        let test_name = thread.name().expect("named libtest thread");
        let identity = format!("{test_name}/{case}");
        if let Some(selected) = std::env::var_os(CASE) {
            if selected == OsStr::new(&identity) {
                assertions();
                println!("{COMPLETED}");
            }
            return;
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
            .env(CASE, &identity)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in self.values {
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
        let mut child = ManagedChild::spawn(&mut command).unwrap();
        let out = child.child_mut().stdout.take().unwrap();
        let err = child.child_mut().stderr.take().unwrap();
        let out = std::thread::spawn(move || bounded_capture(out));
        let err = std::thread::spawn(move || bounded_capture(err));
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.terminate_tree().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        // Descendants must not hold diagnostic pipes open after the test exits.
        child.terminate_tree().unwrap();
        let stdout = out.join().unwrap();
        let stderr = err.join().unwrap();
        assert!(
            status.success() && stdout.contains(COMPLETED) && stdout.contains("1 passed"),
            "isolated case {identity} did not complete: {status}\n{stdout}\n{stderr}"
        );
    }
}

fn bounded_capture(mut reader: impl Read) -> String {
    let mut retained = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let n = reader.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        let remaining = (64 * 1024_usize).saturating_sub(retained.len());
        retained.extend_from_slice(&chunk[..n.min(remaining)]);
    }
    String::from_utf8_lossy(&retained).into_owned()
}

#[test]
fn child_environment_is_isolated_and_parent_is_unchanged() {
    let before = std::env::var_os("WEBCODEX_ENV_ISOLATION_SENTINEL");
    IsolatedEnv::new()
        .set("WEBCODEX_ENV_ISOLATION_SENTINEL", "only-in-child")
        .run("set", || {
            assert_eq!(
                std::env::var("WEBCODEX_ENV_ISOLATION_SENTINEL").unwrap(),
                "only-in-child"
            )
        });
    IsolatedEnv::new()
        .remove("WEBCODEX_ENV_ISOLATION_SENTINEL")
        .run("missing", || {
            assert!(std::env::var_os("WEBCODEX_ENV_ISOLATION_SENTINEL").is_none())
        });
    assert_eq!(std::env::var_os("WEBCODEX_ENV_ISOLATION_SENTINEL"), before);
}
