use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let proc = tmp.path().join("proc");
    let cg = tmp.path().join("cgroup");
    fs::create_dir_all(proc.join("fd")).unwrap();
    fs::create_dir_all(cg.join("service")).unwrap();
    fs::write(proc.join("status"), "Name:\tDO_NOT_EXPOSE\nVmRSS:\t10 kB\nVmHWM:\t12 kB\nRssAnon:\t6 kB\nRssFile:\t4 kB\nRssShmem:\t0 kB\nVmSwap:\t0 kB\nThreads:\t5\n").unwrap();
    fs::write(proc.join("cgroup"), "0::/service\n").unwrap();
    fs::write(cg.join("service/cgroup.procs"), "42\n").unwrap();
    fs::write(cg.join("service/memory.current"), "40960\n").unwrap();
    fs::write(cg.join("service/memory.peak"), "81920\n").unwrap();
    fs::write(cg.join("service/memory.swap.current"), "0\n").unwrap();
    fs::write(
        cg.join("service/memory.pressure"),
        "some avg10=0.12 avg60=0.01 total=10\nfull avg10=0.0 avg60=0.0 total=0\n",
    )
    .unwrap();
    fs::write(cg.join("service/memory.events"), "oom 0\noom_kill 0\n").unwrap();
    (tmp, proc, cg)
}

#[test]
fn resources_keep_process_and_cgroup_separate_and_whitelist_output() {
    let (tmp, proc, cg) = fixture();
    let value = collect_at(&proc, &cg, 42);
    assert_eq!(value["process"]["rss_bytes"], 10240);
    assert_eq!(value["process"]["rss_anon_bytes"], 6144);
    assert_eq!(value["process"]["threads"], 5);
    assert_eq!(value["process"]["private_anon_swap_bytes"], 0);
    assert_eq!(value["process"]["fd_count"], 0);
    assert_eq!(value["cgroup_v2"]["memory_current_bytes"], 40960);
    assert_eq!(value["cgroup_v2"]["pressure_some_avg10"], 0.12);
    assert!(!value.to_string().contains("DO_NOT_EXPOSE"));
    assert!(!value.to_string().contains(tmp.path().to_str().unwrap()));
}

#[test]
fn resources_missing_or_malformed_is_unknown_not_zero() {
    let (_, proc, cg) = fixture(); // TempDir dropped: every input is now absent.
    let value = collect_at(&proc, &cg, 42);
    assert_eq!(value["status"], "unavailable");
    assert!(value["process"]["rss_bytes"].is_null());
    assert!(value["process"]["fd_count"].is_null());
    for bad in [
        "VmRSS: -1 kB",
        "VmRSS: 1 MB",
        "VmRSS: 2 kB extra",
        "VmRSS: 18446744073709551615 kB",
        "VmRSS: 2 kB\nVmRSS: 3 kB",
    ] {
        assert_eq!(status_number(bad, "VmRSS", true), None, "{bad}");
    }
    assert_eq!(status_number("VmRSS: 0 kB", "VmRSS", true), Some(0));
    assert_eq!(status_number("Threads: 3 kB", "Threads", false), None);
    assert_eq!(pressure_avg10("some avg10=NaN", "some"), None);
    assert_eq!(pressure_avg10("full avg10=101", "full"), None);
    assert_eq!(flat_number("oom 1\noom 2", "oom"), None);
}

#[test]
fn resources_never_fallback_to_foreign_or_escaped_cgroup() {
    let (tmp, proc, cg) = fixture();
    assert!(cgroup_directory("0::/service", &cg, 42).is_some());
    assert!(cgroup_directory("0::/service", &cg, 99).is_none());
    for path in [
        "0::/../service",
        "0::/./service",
        "0::relative",
        "0::/service\n0::/service",
        "1:memory:/service",
    ] {
        assert!(cgroup_directory(path, &cg, 42).is_none());
    }
    let outside = tmp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("cgroup.procs"), "42\n").unwrap();
    std::os::unix::fs::symlink(&outside, cg.join("escape")).unwrap();
    assert!(cgroup_directory("0::/escape", &cg, 42).is_none());
    fs::write(proc.join("cgroup"), "0::/not-mounted").unwrap();
    assert_eq!(
        collect_at(&proc, &cg, 42)["cgroup_v2"]["status"],
        "unavailable"
    );
}

#[test]
fn resources_enforce_file_and_descriptor_bounds_without_reading_targets() {
    let (tmp, proc, _) = fixture();
    let huge = tmp.path().join("huge");
    fs::write(&huge, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(read_bounded(&huge).is_none());
    for i in 0..MAX_FDS + 1 {
        std::os::unix::fs::symlink("/not/read/secret", proc.join("fd").join(i.to_string()))
            .unwrap();
    }
    assert_eq!(count_fds(&proc.join("fd")), Some((MAX_FDS, true)));
}
