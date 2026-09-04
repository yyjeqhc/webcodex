use std::io;
use std::os::unix::process::CommandExt as _;
use tokio::process::Command;

pub fn configure_child(command: &mut Command) {
    // Create a new process group whose pgid is the spawned child's pid. If the
    // process-group setup cannot be performed, spawning fails rather than
    // leaving a Desktop-owned child without a safe tree identity.
    command.as_std_mut().process_group(0);
}

pub fn terminate_owned_tree(root_pid: u32) -> bool {
    signal_owned_group(root_pid, libc::SIGTERM)
}

pub fn force_stop_owned_tree(root_pid: u32) -> bool {
    signal_owned_group(root_pid, libc::SIGKILL)
}

pub fn owned_tree_is_running(root_pid: u32) -> bool {
    let Some(pgid) = process_group_id(root_pid) else {
        return false;
    };
    let result = unsafe { libc::killpg(pgid, 0) };
    if result == 0 {
        return true;
    }
    !matches!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH))
}

fn signal_owned_group(root_pid: u32, signal: libc::c_int) -> bool {
    let Some(pgid) = process_group_id(root_pid) else {
        return false;
    };
    let result = unsafe { libc::killpg(pgid, signal) };
    if result == 0 {
        return true;
    }
    matches!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH))
}

fn process_group_id(root_pid: u32) -> Option<libc::pid_t> {
    i32::try_from(root_pid).ok().filter(|value| *value > 0)
}
