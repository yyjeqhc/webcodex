use super::*;
use std::io;
use std::thread;

#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        lock_unpoison(&self.0).extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn channel() -> (Arc<InputChannel>, Arc<Mutex<Vec<u8>>>) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    (
        InputChannel::with_writer(Buffer(bytes.clone())).unwrap(),
        bytes,
    )
}
#[test]
fn keyed_inputs_are_exactly_once_while_retained_and_eof_is_not_polling() {
    let (channel, bytes) = channel();
    let first = channel.submit("1", "你好\n", false).unwrap();
    assert_eq!(first.state, InputWriteState::Written);
    assert_eq!(first.bytes_written, "你好\n".len());
    assert_eq!(channel.submit("1", "你好\n", false).unwrap(), first);
    assert_eq!(
        channel.submit("1", "different", false).unwrap_err(),
        "job_input_conflict"
    );
    assert_eq!(
        channel.submit("1", "你好\n", true).unwrap_err(),
        "job_input_conflict"
    );
    assert_eq!(
        channel.submit("poll", "", false).unwrap_err(),
        "job_input_empty"
    );
    let eof = channel.submit("eof", "", true).unwrap();
    assert_eq!(eof.state, InputWriteState::Closed);
    assert!(eof.stdin_closed);
    assert_eq!(channel.submit("eof", "", true).unwrap(), eof);
    assert_eq!(
        channel.submit("2", "again", false).unwrap_err(),
        "job_input_closed"
    );
    assert_eq!(&*lock_unpoison(&bytes), "你好\n".as_bytes());
}
#[test]
fn eof_receipt_is_published_only_after_owned_handle_drop() {
    let closed = Arc::new(AtomicBool::new(false));
    struct Pipe {
        closed: Arc<AtomicBool>,
    }
    impl Write for Pipe {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Drop for Pipe {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }
    let channel = InputChannel::with_writer(Pipe {
        closed: closed.clone(),
    })
    .unwrap();
    let receipt = channel.submit("close", "data", true).unwrap();
    assert_eq!(receipt.state, InputWriteState::Closed);
    assert!(closed.load(Ordering::SeqCst));
}

#[test]
fn quota_keeps_all_replay_keys_and_reserves_eof() {
    let (channel, bytes) = channel();
    for i in 0..MAX_WRITES {
        assert_eq!(
            channel.submit(&i.to_string(), "a", false).unwrap().state,
            InputWriteState::Written
        );
    }
    assert_eq!(
        channel.submit("overflow", "b", false).unwrap_err(),
        "job_input_capacity"
    );
    assert_eq!(
        channel.submit("0", "a", false).unwrap().state,
        InputWriteState::Written
    );
    assert_eq!(
        channel.submit("eof", "", true).unwrap().state,
        InputWriteState::Closed
    );
    assert_eq!(lock_unpoison(&bytes).len(), MAX_WRITES);
}
#[test]
fn partial_pipe_failure_is_unknown_and_never_resent() {
    struct Partial(usize);
    impl Write for Partial {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            if self.0 == 1 {
                Ok(1)
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let channel = InputChannel::with_writer(Partial(0)).unwrap();
    let receipt = channel.submit("one", "abc", false).unwrap();
    assert_eq!(receipt.state, InputWriteState::OutcomeUnknown);
    assert_eq!(receipt.bytes_written, 1);
    assert!(receipt.stdin_closed);
    assert_eq!(channel.submit("one", "abc", false).unwrap(), receipt);
    assert!(channel.submit("two", "b", false).is_err());
}
#[test]
fn pending_write_can_be_reconciled_without_duplicate_bytes_and_finish_unblocks_waiter() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let output = Arc::new(Mutex::new(Vec::new()));
    struct Block {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        out: Arc<Mutex<Vec<u8>>>,
    }
    impl Write for Block {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.entered.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            lock_unpoison(&self.out).extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let channel = InputChannel::with_writer(Block {
        entered: entered_tx,
        release: release_rx,
        out: output.clone(),
    })
    .unwrap();
    let worker = channel.clone();
    let handle = thread::spawn(move || worker.submit("slow", "abc", false).unwrap());
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        channel.submit("slow", "abc", false).unwrap().state,
        InputWriteState::Pending
    );
    assert_eq!(
        channel.submit("other", "x", false).unwrap_err(),
        "job_input_busy"
    );
    channel.finish();
    assert_eq!(
        handle.join().unwrap().state,
        InputWriteState::OutcomeUnknown
    );
    release_tx.send(()).unwrap();
    let state = lock_unpoison(&channel.shared.0);
    let (state, timeout) = channel
        .shared
        .1
        .wait_timeout_while(state, Duration::from_secs(5), |s| {
            s.records["slow"].1.bytes_written == 0
        })
        .unwrap();
    assert!(!timeout.timed_out());
    assert_eq!(state.records["slow"].1.bytes_written, 3);
    assert_eq!(&*lock_unpoison(&output), b"abc");
}
