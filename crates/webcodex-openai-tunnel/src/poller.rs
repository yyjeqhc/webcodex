use crate::{command::validate, control_plane::backoff, Error, TunnelClient};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{
    collections::{HashSet, VecDeque},
    future::Future,
    sync::atomic::Ordering,
};
use tokio::time::{sleep_until, Instant};

impl TunnelClient {
    pub(crate) async fn poller(&self, stop: impl Future<Output = ()>) -> Result<(), Error> {
        struct ReadinessGuard(crate::Health);
        impl Drop for ReadinessGuard {
            fn drop(&mut self) {
                self.0.ready(false);
            }
        }
        let _readiness_guard = ReadinessGuard(self.health.clone());
        tokio::pin!(stop);
        let mut receipts = HashSet::new();
        let mut queue = VecDeque::new();
        let mut work = FuturesUnordered::new();
        let mut next_poll = Instant::now();
        let mut failures = 0u32;
        // Keep a poll future alive while workers complete: dropping it on each
        // completion could lose commands already removed from the server queue.
        let result = 'run: loop {
            while work.len() < self.limits.concurrency {
                let Some((command, received)) = queue.pop_front() else {
                    break;
                };
                work.push(self.execute(command, received));
            }
            if queue.is_empty()
                && receipts.len() > self.limits.lifetime_commands - self.limits.ingress_commands
            {
                if work.is_empty() {
                    break Err(Error::Capacity);
                }
            } else if queue.is_empty() {
                let hint = self.limits.concurrency.saturating_sub(work.len()).max(1);
                let poll = async move {
                    sleep_until(next_poll).await;
                    self.poll(hint).await
                };
                // Snapshot size above instead of borrowing work throughout poll.
                tokio::pin!(poll);
                loop {
                    tokio::select! {
                        biased;
                        _ = &mut stop => break 'run Ok(()),
                        result = &mut poll => {
                            match result {
                                Ok((received, commands)) => {
                                    self.health.ready(true); failures = 0; next_poll = Instant::now();
                                    // Validate the batch's correlation keys before admitting any item.
                                    if commands.iter().any(|c| c.request_id.is_empty() || c.request_id.len() > 256) { break 'run Err(Error::Protocol); }
                                    for command in commands {
                                        if !receipts.insert(command.request_id.clone()) { continue; }
                                        match validate(&command) {
                                            Ok(true) => queue.push_back((command, received)),
                                            _ => { self.health.0.rejected.fetch_add(1, Ordering::Relaxed); },
                                        }
                                    }
                                }
                                Err(failure) => {
                                    self.health.ready(false);
                                    if failure.error != Error::Transport { break 'run Err(failure.error); }
                                    next_poll = Instant::now() + backoff(failures).max(failure.delay);
                                    failures = failures.saturating_add(1);
                                }
                            }
                            continue 'run;
                        }
                        Some(result) = work.next(), if !work.is_empty() => {
                            if let Err(error) = result { break 'run Err(error); }
                        }
                    }
                }
            }
            tokio::select! {
                biased;
                _ = &mut stop => break Ok(()),
                Some(result) = work.next(), if !work.is_empty() => {
                    if let Err(error) = result { break Err(error); }
                }
            }
        };
        // Futures are owned here, never detached Tokio tasks. Drop closes HTTP work.
        drop(work);
        self.health.ready(false);
        if result.is_ok() && self.health.has_uncertain_work() {
            Err(Error::Uncertain)
        } else {
            result
        }
    }
}
