//! Test helper for `webcodex-process` integration tests.
//!
//! Self-contained (no cmd, PowerShell, bash, or Git Bash) so the same binary
//! runs on Windows and Unix. Modes are selected by `argv[1]`:
//!
//! * `sleep <secs> [exit_code]` — sleep then exit; used for normal completion.
//! * `spawn-grandchild <marker> <delay> <total>` — spawn ourself again as a
//!   grandchild, print `GRANDCHILD_PID=<pid>`, then exit immediately while the
//!   grandchild keeps running.
//! * `spawn-grandchild-keepalive <marker> <delay> <total> <keepalive>` — spawn
//!   ourself again as a grandchild, print `GRANDCHILD_PID=<pid>`, then sleep
//!   `keepalive` so the direct child stays alive while the grandchild runs.
//! * `grandchild <marker> <delay> <total>` — sleep `delay`, write our own PID
//!   to `marker`, sleep until `total`, then exit 0.
//! * `hold-stdout` — write `PING` and keep the stdout write end open forever.

use std::io::Write;
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("sleep");

    match mode {
        "sleep" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            let code: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            std::process::exit(code);
        }
        "spawn-grandchild" => {
            spawn_grandchild(&args);
            std::process::exit(0);
        }
        "spawn-grandchild-keepalive" => {
            spawn_grandchild(&args);
            let keepalive: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(60);
            std::thread::sleep(std::time::Duration::from_secs(keepalive));
            std::process::exit(0);
        }
        "grandchild" => {
            let marker = args.get(2).expect("marker path");
            let delay: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            let total: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
            std::thread::sleep(std::time::Duration::from_secs(delay));
            std::fs::write(marker, std::process::id().to_string()).expect("write marker");
            let rest = total.saturating_sub(delay);
            std::thread::sleep(std::time::Duration::from_secs(rest));
        }
        "hold-stdout" => {
            println!("PING");
            std::io::stdout().flush().expect("flush stdout");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        other => {
            eprintln!("process_tree_helper: unknown mode: {other}");
            std::process::exit(2);
        }
    }
}

/// Spawn ourself again as a `grandchild` that inherits stdin/stdout/stderr so
/// it keeps the piped stdout write end open, and print its pid on the parent's
/// stdout. Args are indexed as in the `spawn-grandchild` family of modes.
fn spawn_grandchild(args: &[String]) {
    let marker = args.get(2).expect("marker path");
    let delay: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
    let total: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
    let self_exe = std::env::current_exe().expect("current_exe");
    let mut cmd = Command::new(self_exe);
    cmd.args(["grandchild", marker, &delay.to_string(), &total.to_string()]);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    // The grandchild must survive this process: `spawn-grandchild` exits
    // immediately so tests can verify process-tree termination (Job Object /
    // process group) reaches the orphaned grandchild. Waiting for it here
    // would change the test contract.
    #[allow(clippy::zombie_processes)]
    let child = cmd.spawn().expect("spawn grandchild");
    println!("GRANDCHILD_PID={}", child.id());
    std::io::stdout().flush().expect("flush stdout");
}
