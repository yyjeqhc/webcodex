//! Nonblocking stdout, stderr, and private control readers.

use super::*;

#[cfg(unix)]
fn set_nonblocking(fd: RawFd) -> Result<(), ShellError> {
    // SAFETY: `fd` belongs to a live pipe owned by the caller.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(ShellError::new(
            "persistent_shell_reader_failed",
            format!(
                "failed to configure persistent shell pipe: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn spawn_output_reader(
    name: &'static str,
    mut pipe: impl Read + AsRawFd + Send + 'static,
    buffer: Arc<Mutex<BoundedBuffer>>,
    expected_token: Arc<Mutex<Option<String>>>,
    sync_sender: mpsc::SyncSender<String>,
    sync_magic: &'static [u8],
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, ShellError> {
    set_nonblocking(pipe.as_raw_fd())?;
    thread::Builder::new()
        .name(format!("wc-persistent-shell-{name}"))
        .spawn(move || {
            let mut chunk = [0_u8; 8192];
            let mut pending = Vec::new();
            let mut last_synced_token: Option<String> = None;
            while !stop.load(Ordering::SeqCst) {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        pending.extend_from_slice(&chunk[..read]);
                        process_output_pending(
                            &mut pending,
                            &buffer,
                            &expected_token,
                            &sync_sender,
                            sync_magic,
                            &mut last_synced_token,
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(OUTPUT_READ_SLEEP);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            lock_unpoison(&buffer).append(&pending);
        })
        .map_err(|error| {
            ShellError::new(
                "persistent_shell_reader_failed",
                format!("failed to start persistent shell {name} reader: {error}"),
            )
        })
}

#[cfg(unix)]
pub(super) fn spawn_control_reader(
    mut pipe: File,
    expected_token: Arc<Mutex<Option<String>>>,
    sender: mpsc::SyncSender<ControlFrame>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, ShellError> {
    set_nonblocking(pipe.as_raw_fd())?;
    thread::Builder::new()
        .name("wc-persistent-shell-control".to_string())
        .spawn(move || {
            let mut chunk = [0_u8; 1024];
            let mut field = Vec::new();
            let mut stage = 0_u8;
            let mut token = String::new();
            let mut status = 0_i32;
            while !stop.load(Ordering::SeqCst) {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        for byte in &chunk[..read] {
                            if *byte != 0 {
                                if field.len() < CONTROL_FIELD_MAX_BYTES {
                                    field.push(*byte);
                                } else {
                                    field.clear();
                                    stage = 0;
                                }
                                continue;
                            }
                            match stage {
                                0 if field == CONTROL_MAGIC => stage = 1,
                                0 => {}
                                1 => {
                                    let candidate = String::from_utf8_lossy(&field).into_owned();
                                    if lock_unpoison(&expected_token).as_deref()
                                        == Some(candidate.as_str())
                                    {
                                        token = candidate;
                                        stage = 2;
                                    } else {
                                        stage = u8::from(field == CONTROL_MAGIC);
                                    }
                                }
                                2 => match String::from_utf8_lossy(&field).parse::<i32>() {
                                    Ok(value) => {
                                        status = value;
                                        stage = 3;
                                    }
                                    Err(_) => stage = 0,
                                },
                                3 => {
                                    if field.last() == Some(&b'\n') {
                                        field.pop();
                                    }
                                    let cwd = PathBuf::from(OsString::from_vec(std::mem::take(
                                        &mut field,
                                    )));
                                    let _ = sender.try_send(ControlFrame {
                                        token: std::mem::take(&mut token),
                                        status,
                                        cwd,
                                    });
                                    stage = 0;
                                }
                                _ => stage = 0,
                            }
                            field.clear();
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(OUTPUT_READ_SLEEP);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| {
            ShellError::new(
                "persistent_shell_reader_failed",
                format!("failed to start persistent shell control reader: {error}"),
            )
        })
}
