//! Explicit bounded Server diagnostics, never a background sampler or heap profiler.
//! No subprocess, descriptor targets, command lines, environment, arbitrary PID,
//! or payload/database scan. Missing/unsupported observations remain unavailable.
use serde_json::{json, Value};

pub(super) fn observe() -> Value {
    #[cfg(target_os = "linux")]
    let mut value = linux::collect_at(
        std::path::Path::new("/proc/self"),
        std::path::Path::new("/sys/fs/cgroup"),
        std::process::id(),
    );
    #[cfg(not(target_os = "linux"))]
    let mut value = json!({"status":"unsupported", "process":null, "cgroup_v2":null});
    value["scope"] = json!("server_process");
    value["pid"] = json!(std::process::id());
    value["observed_at_ms"] = json!(chrono::Utc::now().timestamp_millis());
    value
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    const MAX_FILE_BYTES: u64 = 32 * 1024;
    const MAX_FDS: usize = 4096;

    pub(super) fn collect_at(proc_self: &Path, cgroup_root: &Path, pid: u32) -> Value {
        let status = read_bounded(&proc_self.join("status"));
        let field = |name, unit| status.as_deref().and_then(|s| status_number(s, name, unit));
        let rss = field("VmRSS", true);
        let threads = field("Threads", false);
        let fds = count_fds(&proc_self.join("fd"));
        let cgroup = read_bounded(&proc_self.join("cgroup"))
            .and_then(|membership| cgroup_directory(&membership, cgroup_root, pid));
        let group = match cgroup {
            None => json!({"status":"unavailable"}),
            Some(directory) => {
                let current = scalar_file(&directory.join("memory.current"));
                let pressure = read_bounded(&directory.join("memory.pressure"));
                let events = read_bounded(&directory.join("memory.events"));
                json!({
                    "status": if current.is_some() {"available"} else {"unavailable"},
                    "source":"process_cgroup_v2",
                    "memory_current_bytes": current,
                    "memory_peak_bytes": scalar_file(&directory.join("memory.peak")),
                    "swap_current_bytes": scalar_file(&directory.join("memory.swap.current")),
                    "pressure_some_avg10": pressure.as_deref().and_then(|p| pressure_avg10(p, "some")),
                    "pressure_full_avg10": pressure.as_deref().and_then(|p| pressure_avg10(p, "full")),
                    "oom_events": events.as_deref().and_then(|e| flat_number(e, "oom")),
                    "oom_kill_events": events.as_deref().and_then(|e| flat_number(e, "oom_kill")),
                })
            }
        };
        json!({
            "status": if rss.is_some() || group["status"] == "available" {"available"} else {"unavailable"},
            "process": {
                "source":"proc_self_status", "rss_accounting":"kernel_approximate",
                "rss_bytes":rss, "rss_anon_bytes":field("RssAnon", true),
                "rss_file_bytes":field("RssFile", true), "rss_shmem_bytes":field("RssShmem", true),
                "rss_peak_bytes":field("VmHWM", true), "private_anon_swap_bytes":field("VmSwap", true),
                "threads":threads,
                "fd_count":fds.map(|(count, _)| count),
                "fd_count_truncated":fds.map(|(_, truncated)| truncated),
            },
            "cgroup_v2":group,
        })
    }

    fn read_bounded(path: &Path) -> Option<String> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_FILE_BYTES as usize {
            return None;
        }
        String::from_utf8(bytes).ok()
    }

    fn unsigned(value: &str) -> Option<u64> {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse().ok())
            .flatten()
    }

    // Reject duplicate/malformed counters instead of silently reporting a guessed zero.
    pub(super) fn status_number(text: &str, name: &str, kib: bool) -> Option<u64> {
        let mut lines = text
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(key, _)| *key == name);
        let (_, value) = lines.next()?;
        if lines.next().is_some() {
            return None;
        }
        let mut fields = value.split_whitespace();
        let number = unsigned(fields.next()?)?;
        if kib && fields.next()? != "kB" {
            return None;
        }
        if fields.next().is_some() {
            return None;
        }
        if kib {
            number.checked_mul(1024)
        } else {
            Some(number)
        }
    }

    fn scalar_file(path: &Path) -> Option<u64> {
        unsigned(read_bounded(path)?.trim())
    }

    fn flat_number(text: &str, name: &str) -> Option<u64> {
        let mut rows = text.lines().filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == name).then_some(words)
        });
        let mut words = rows.next()?;
        if rows.next().is_some() {
            return None;
        }
        let number = unsigned(words.next()?)?;
        words.next().is_none().then_some(number)
    }

    fn pressure_avg10(text: &str, kind: &str) -> Option<f64> {
        let mut rows = text
            .lines()
            .filter(|line| line.split_whitespace().next() == Some(kind));
        let row = rows.next()?;
        if rows.next().is_some() {
            return None;
        }
        let mut values = row
            .split_whitespace()
            .filter_map(|word| word.strip_prefix("avg10="));
        let value: f64 = values.next()?.parse().ok()?;
        if values.next().is_some() || !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return None;
        }
        Some(value)
    }

    fn count_fds(path: &Path) -> Option<(usize, bool)> {
        let mut count = 0;
        for entry in fs::read_dir(path).ok()?.take(MAX_FDS + 1) {
            entry.ok()?;
            if count == MAX_FDS {
                return Some((count, true));
            }
            count += 1;
        }
        Some((count, false))
    }

    fn cgroup_directory(membership: &str, root: &Path, pid: u32) -> Option<PathBuf> {
        let mut unified = membership
            .lines()
            .filter_map(|line| line.strip_prefix("0::"));
        let path = unified.next()?;
        if unified.next().is_some()
            || !path.starts_with('/')
            || path.len() > 4096
            || path.chars().any(char::is_control)
            || path.split('/').any(|part| part == "." || part == "..")
        {
            return None;
        }
        let root = root.canonicalize().ok()?;
        let directory = root
            .join(path.trim_start_matches('/'))
            .canonicalize()
            .ok()?;
        if !directory.starts_with(&root) {
            return None;
        }
        // Nonstandard mount roots/namespaces must not accidentally report a
        // different group's counters. Never fall back to whole-host memory.
        let members = read_bounded(&directory.join("cgroup.procs"))?;
        if !members.lines().any(|line| line.parse::<u32>() == Ok(pid)) {
            return None;
        }
        Some(directory)
    }

    #[cfg(test)]
    mod tests;
}

#[cfg(test)]
mod tests;
