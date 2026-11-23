// Standalone process-tree fixture for validation execution lifecycle tests.
// Tests compile this file directly with rustc; it is not a production binary
// target (same pattern as fake_claude_mcp.rs / fake_server.rs / the
// job-manager use of webcodex-process's process_tree_helper.rs).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
const SIGTERM: i32 = 15;
#[cfg(unix)]
const SIG_IGN: usize = 1;
#[cfg(unix)]
static SIGTERM_HANDLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
    fn write(fd: i32, buf: *const core::ffi::c_void, count: usize) -> isize;
}

/// SIGTERM handler for the `sigterm-marker` mode. Writes the marker to the
/// captured stdout with raw `write(2)` (async-signal-safe) and records that
/// the graceful request was received. SIGKILL cannot be caught, so the marker
/// only appears when a graceful SIGTERM reached the helper.
#[cfg(unix)]
extern "C" fn handle_sigterm(_signum: i32) {
    let msg = b"SIGTERM_HANDLED\n";
    // SAFETY: fd 1 is the capture pipe inherited from the parent; write(2)
    // is async-signal-safe.
    unsafe {
        write(1, msg.as_ptr() as *const core::ffi::c_void, msg.len());
    }
    SIGTERM_HANDLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("sleep");
    match mode {
        "sleep" => {
            println!("VALIDATION_HELPER_STDOUT");
            eprintln!("VALIDATION_HELPER_STDERR");
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            let code: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            std::process::exit(code);
        }
        #[cfg(windows)]
        "console-probe" | "-V" => {
            use std::io::Read;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetConsoleWindow() -> *mut core::ffi::c_void;
            }
            // The test owner has no console. An ordinary console spawn here
            // creates a window; CREATE_NO_WINDOW permits only a hidden console.
            let window = unsafe { !GetConsoleWindow().is_null() };
            if mode == "-V" {
                // The real SSH capability probe discards its output. Its
                // injected executable reports console allocation by exit code.
                std::process::exit(if window { 37 } else { 0 });
            }
            let mut input = Vec::new();
            std::io::stdin()
                .read_to_end(&mut input)
                .expect("read stdin");
            println!("CONSOLE_WINDOW={window}");
            println!("STDIN_BYTES={}", input.len());
            eprintln!("CONSOLE_PROBE_STDERR");
            std::process::exit(37);
        }
        "spawn-descendant" => {
            spawn_descendant(&args, false);
            // Wait until the descendant has written its own marker, so the
            // direct child never exits before the descendant is provably
            // alive (a race-free "parent exits first" fixture).
            let alive_marker = args.get(3).expect("alive marker path");
            wait_until_file(Path::new(alive_marker));
            std::process::exit(0);
        }
        "spawn-descendant-keepalive" => {
            spawn_descendant(&args, true);
        }
        "ignore-term-keepalive" => {
            #[cfg(unix)]
            // SAFETY: setting SIGTERM to SIG_IGN is async-signal-safe; the
            // helper is single-threaded at this point.
            unsafe {
                signal(SIGTERM, SIG_IGN);
            }
            #[cfg(not(unix))]
            {
                eprintln!("validation_tree_helper: ignore-term-keepalive is unix-only");
                std::process::exit(2);
            }
            spawn_descendant(&args, true);
        }
        "sigterm-marker" => {
            #[cfg(unix)]
            {
                let total: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
                // SAFETY: installing the handler is async-signal-safe and the
                // helper is single-threaded at this point.
                unsafe {
                    signal(SIGTERM, handle_sigterm as usize);
                }
                // Keep running until the graceful SIGTERM arrives. Exiting 0
                // after `total` is only a defensive backstop for a fixture
                // misconfiguration; the graceful exit path is the exit 0
                // reached right after the handler runs.
                let deadline = std::time::Instant::now() + Duration::from_secs(total);
                while !SIGTERM_HANDLED.load(std::sync::atomic::Ordering::SeqCst) {
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                std::process::exit(0);
            }
            #[cfg(not(unix))]
            {
                eprintln!("validation_tree_helper: sigterm-marker is unix-only");
                std::process::exit(2);
            }
        }
        "descendant" => {
            let alive_marker = args.get(2).expect("alive marker path");
            let total: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
            std::fs::write(
                alive_marker,
                format!("DESCENDANT_PID={}\n", std::process::id()),
            )
            .expect("write alive marker");
            std::thread::sleep(std::time::Duration::from_secs(total));
            std::process::exit(0);
        }
        other => {
            eprintln!("validation_tree_helper: unknown mode: {other}");
            std::process::exit(2);
        }
    }
}

/// Write the parent/descendant pid markers, spawn ourself as a descendant that
/// inherits stdout/stderr (so it keeps the validation capture pipe open), print
/// the descendant pid on stdout, then either exit (`keepalive = false`) or
/// sleep `total` seconds (`keepalive = true`).
/// Poll until `path` exists (bounded, so a missing descendant cannot hang the
/// fixture's parent forever).
fn wait_until_file(path: &Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn spawn_descendant(args: &[String], keepalive: bool) {
    let parent_marker = args.get(2).expect("parent marker path");
    let alive_marker = args.get(3).expect("alive marker path");
    let total: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
    let self_exe = std::env::current_exe().expect("current_exe");
    let mut cmd = Command::new(self_exe);
    cmd.args(["descendant", alive_marker, &total.to_string()]);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let descendant = cmd.spawn().expect("spawn descendant");
    let pid = descendant.id();
    std::fs::write(
        parent_marker,
        format!("PARENT_PID={}\nDESCENDANT_PID={pid}\n", std::process::id()),
    )
    .expect("write parent marker");
    println!("DESCENDANT_PID={pid}");
    std::io::stdout().flush().expect("flush stdout");
    if keepalive {
        std::thread::sleep(std::time::Duration::from_secs(total));
        std::process::exit(0);
    }
}
