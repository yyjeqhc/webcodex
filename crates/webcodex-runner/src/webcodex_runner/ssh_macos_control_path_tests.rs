use super::*;
use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};

#[test]
fn macos_control_socket_path_fits_with_openssh_temporary_suffix() {
    let mut state = SshPoolState::default();
    let root = ensure_control_root(&mut state).unwrap();
    // OpenSSH creates a temporary mux socket by appending '.' + 16 random
    // characters. Checking only the final ControlPath misses this constraint.
    let path = root.join("cffffffffffffffff.0123456789abcdef");
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(path.as_os_str().as_bytes().len() < 104);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(listener);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(!std::fs::symlink_metadata(&root)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(ensure_control_root(&mut state).unwrap(), root);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn macos_control_roots_remain_unique_and_private() {
    let mut first = SshPoolState::default();
    let mut second = SshPoolState::default();
    let a = ensure_control_root(&mut first).unwrap();
    let b = ensure_control_root(&mut second).unwrap();
    assert_ne!(a, b);
    assert_eq!(
        std::fs::metadata(&a).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&b).unwrap().permissions().mode() & 0o777,
        0o700
    );
    std::fs::remove_dir(a).unwrap();
    std::fs::remove_dir(b).unwrap();
}
