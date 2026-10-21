use super::super::SessionStore;
use super::tests::persistent_fixture;
use super::*;
use crate::{
    ListSessionMessagesFilter, PostSessionMessageInput, SessionMessageDelivery, SessionMessageKind,
    SessionMessagePriority,
};
use std::future::{poll_fn, Future};
use std::sync::mpsc;
use std::task::Poll;

fn delivery_input(session_id: &str) -> PostSessionMessageInput {
    PostSessionMessageInput {
        session_id: session_id.to_owned(),
        kind: SessionMessageKind::Note,
        message: "durable delivery".to_owned(),
        tags: Vec::new(),
        reply_to: None,
        priority: SessionMessagePriority::Normal,
    }
}

fn delivery(key: &str) -> Option<SessionMessageDelivery> {
    Some(SessionMessageDelivery {
        sender_scope: "a".repeat(64),
        delivery_key: key.to_owned(),
    })
}

// Hold disk admission on another thread, with a watchdog so a regression that
// blocks the current-thread executor fails instead of hanging the test suite.
fn block_writes(store: &SessionStore) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let mutex = Arc::clone(&store.persistence_write_mutex);
    let (ready, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let join = std::thread::spawn(move || {
        let _guard = mutex.lock().unwrap();
        ready.send(()).unwrap();
        released
            .recv_timeout(Duration::from_secs(10))
            .expect("async request blocked its executor before releasing the write gate");
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    (release, join)
}

async fn assert_pending(future: std::pin::Pin<&mut impl Future>) {
    let mut future = future;
    poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_delivery_keeps_notification_and_restart_safe_replay() {
    for background_writer in [true, false] {
        let (temporary, mut store) = persistent_fixture(Duration::from_secs(60));
        if !background_writer {
            drop(store.writer.take());
        }
        let session_id = store.start_session(None, None).session_id;
        store.flush_persistence();
        let mut notifications = store.message_observation_notify.subscribe();
        let (release, write_gate) = block_writes(&store);
        let mut pending = Box::pin(store.post_message_with_ack_and_delivery(
            delivery_input(&session_id),
            false,
            delivery("cancelled"),
        ));
        assert_pending(pending.as_mut()).await;

        // This unrelated task must run while durability is still blocked.
        assert_eq!(tokio::spawn(async { 42 }).await.unwrap(), 42);
        assert!(notifications.has_changed().unwrap());
        notifications.changed().await.unwrap();
        let messages = store
            .list_messages(&session_id, ListSessionMessagesFilter::default())
            .unwrap();
        assert_eq!(messages.len(), 1);
        let message_id = messages[0].message_id.clone();

        if !background_writer {
            // Fallback admission happens before mutation. It neither consumes a
            // second blocking worker nor accepts an unqueued second delivery.
            let mut queued = Box::pin(store.post_message_with_ack_and_delivery(
                delivery_input(&session_id),
                false,
                delivery("queued"),
            ));
            assert_pending(queued.as_mut()).await;
            drop(queued);
            assert_eq!(
                store
                    .list_messages(&session_id, ListSessionMessagesFilter::default())
                    .unwrap()
                    .len(),
                1
            );
        }

        drop(pending);
        release.send(()).unwrap();
        write_gate.join().unwrap();
        // Observe the already scheduled write; do not create a replacement
        // dirty generation that could hide cancellation losing the first write.
        if let Some(writer) = &store.writer {
            let generation = writer.shared.state.lock().unwrap().dirty_generation;
            writer.flush_through(generation).await;
        } else {
            drop(
                Arc::clone(&store.persistence_fallback)
                    .acquire_owned()
                    .await
                    .unwrap(),
            );
        }
        assert!(store.status().last_persist_error.is_none());
        drop(store);

        let restored =
            SessionStore::with_persistence(temporary.path().join("sessions.json"), 16, 20);
        let replay = restored
            .post_message_with_ack_and_delivery(
                delivery_input(&session_id),
                false,
                delivery("cancelled"),
            )
            .await
            .unwrap();
        assert!(replay.replayed);
        assert!(!replay.state_changed);
        assert_eq!(replay.message.message_id, message_id);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn async_flush_waits_only_for_its_generation_and_handles_precompleted_writes() {
    let shared = Arc::new(LedgerWriterShared {
        state: Mutex::new(LedgerWriterState::default()),
        cvar: Condvar::new(),
        completed: tokio::sync::Notify::new(),
    });
    let writer = LedgerWriterGuard {
        shared: Arc::clone(&shared),
        join: Mutex::new(None),
    };
    let first = writer.mark_dirty();
    let mut first_wait = Box::pin(writer.flush_through(first));
    assert_pending(first_wait.as_mut()).await;
    let mut same_generation_wait = Box::pin(writer.flush_through(first));
    assert_pending(same_generation_wait.as_mut()).await;
    let second = writer.mark_dirty();
    let mut second_wait = Box::pin(writer.flush_through(second));
    assert_pending(second_wait.as_mut()).await;

    shared.state.lock().unwrap().writes_completed = first;
    shared.completed.notify_waiters();
    tokio::time::timeout(Duration::from_secs(10), async {
        first_wait.await;
        same_generation_wait.await;
    })
    .await
    .expect("all waiters for the completed generation must wake");
    assert_pending(second_wait.as_mut()).await;

    shared.state.lock().unwrap().writes_completed = second;
    shared.completed.notify_waiters();
    tokio::time::timeout(Duration::from_secs(10), second_wait)
        .await
        .expect("later generation must wake when its write finishes");
    // No retained notification is necessary once the predicate already holds.
    writer.flush_through(first).await;
    writer.flush_through(second).await;
}
