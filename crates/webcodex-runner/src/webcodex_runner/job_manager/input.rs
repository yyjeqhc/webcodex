//! One bounded writer per interactive Job. Receipts are retained with that Job;
//! retries reconcile the same key, never resend bytes. No persistent stdin replay.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
use webcodex_core::job_input::{
    validate_input, InputWriteState, JobInputReceipt, JobInputRequest, MAX_WRITES,
};

#[derive(Debug)]
pub(super) struct InputChannel {
    shared: Arc<(Mutex<State>, Condvar)>,
    sender: Mutex<Option<mpsc::SyncSender<WriteInput>>>,
}
#[derive(Debug, Default)]
struct State {
    stopped: bool,
    pending: bool,
    records: HashMap<String, ([u8; 32], JobInputReceipt)>,
}
struct WriteInput {
    id: String,
    bytes: Vec<u8>,
    close: bool,
}

impl InputChannel {
    pub(super) fn attach(stdin: std::process::ChildStdin) -> std::io::Result<Arc<Self>> {
        Self::with_writer(stdin)
    }
    fn with_writer(writer: impl Write + Send + 'static) -> std::io::Result<Arc<Self>> {
        let mut writer = Some(writer);
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let (tx, rx) = mpsc::sync_channel::<WriteInput>(1);
        let result = Arc::new(Self {
            shared: shared.clone(),
            sender: Mutex::new(Some(tx)),
        });
        std::thread::Builder::new()
            .name("job-stdin".into())
            .spawn(move || {
                while let Ok(input) = rx.recv() {
                    {
                        let mut state = lock_unpoison(&shared.0);
                        if state.stopped {
                            drop(writer.take());
                            if let Some((_, receipt)) = state.records.get_mut(&input.id) {
                                receipt.state = InputWriteState::OutcomeUnknown;
                                receipt.stdin_closed = true;
                            }
                            state.pending = false;
                            shared.1.notify_all();
                            break;
                        }
                    }
                    let mut written = 0;
                    let mut failed = false;
                    while written < input.bytes.len() {
                        match writer
                            .as_mut()
                            .expect("writer remains until EOF")
                            .write(&input.bytes[written..])
                        {
                            Ok(0) => {
                                failed = true;
                                break;
                            }
                            Ok(count) => written += count,
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                continue
                            }
                            Err(_) => {
                                failed = true;
                                break;
                            }
                        }
                    }
                    let close = failed || input.close;
                    if close {
                        drop(writer.take());
                    } // Report EOF only after closing the handle.
                      // A successful write means pipe delivery only. No stdout parsing
                      // guesses whether the application understood or consumed it.
                    let mut state = lock_unpoison(&shared.0);
                    let (_, receipt) = state
                        .records
                        .get_mut(&input.id)
                        .expect("admitted input receipt");
                    receipt.bytes_written = written;
                    receipt.stdin_closed = close;
                    receipt.state = if failed {
                        InputWriteState::OutcomeUnknown
                    } else if close {
                        InputWriteState::Closed
                    } else {
                        InputWriteState::Written
                    };
                    state.pending = false;
                    if close {
                        state.stopped = true;
                    }
                    shared.1.notify_all();
                    drop(state);
                    if close {
                        break;
                    }
                }
                // Dropping the sole writer emits EOF. A blocked write is released by
                // the existing managed process-tree shutdown, not by detaching it.
                drop(writer);
                let mut state = lock_unpoison(&shared.0);
                state.stopped = true;
                state.pending = false;
                for (_, receipt) in state.records.values_mut() {
                    if receipt.state == InputWriteState::Pending {
                        receipt.state = InputWriteState::OutcomeUnknown;
                        receipt.stdin_closed = true;
                    }
                }
                shared.1.notify_all();
            })?;
        Ok(result)
    }

    pub(super) fn submit(
        &self,
        id: &str,
        data: &str,
        close: bool,
    ) -> Result<JobInputReceipt, String> {
        validate_input(id, data, close)?;
        let mut digest = Sha256::new();
        digest.update(b"webcodex-job-input-v1\0");
        digest.update([u8::from(close)]);
        digest.update(data.as_bytes());
        let digest: [u8; 32] = digest.finalize().into();
        let mut state = lock_unpoison(&self.shared.0);
        if let Some((old, receipt)) = state.records.get(id) {
            if old != &digest {
                return Err("job_input_conflict".into());
            }
            return Ok(receipt.clone());
        }
        if state.stopped {
            return Err("job_input_closed".into());
        }
        if state.pending {
            return Err("job_input_busy".into());
        }
        // Reserve one additional bounded record for EOF after the data-write
        // quota. Reaching the quota must not trap a program awaiting EOF.
        if state.records.len() >= MAX_WRITES
            && !(data.is_empty() && close && state.records.len() == MAX_WRITES)
        {
            return Err("job_input_capacity".into());
        }
        let receipt = JobInputReceipt {
            input_id: id.into(),
            state: InputWriteState::Pending,
            bytes_written: 0,
            stdin_closed: false,
        };
        state.records.insert(id.into(), (digest, receipt));
        state.pending = true;
        let send = lock_unpoison(&self.sender)
            .as_ref()
            .ok_or(())
            .and_then(|tx| {
                tx.try_send(WriteInput {
                    id: id.into(),
                    bytes: data.as_bytes().to_vec(),
                    close,
                })
                .map_err(|_| ())
            });
        if send.is_err() {
            state.records.remove(id);
            state.pending = false;
            return Err("job_input_closed".into());
        }
        // Bound the request worker, not the process's lifetime or pipe write.
        // Exact replay returns pending/terminal receipt without another write.
        let (state, _) = self
            .shared
            .1
            .wait_timeout_while(state, Duration::from_secs(1), |state| {
                state.records[id].1.state == InputWriteState::Pending
            })
            .unwrap_or_else(|e| e.into_inner());
        Ok(state.records[id].1.clone())
    }

    pub(super) fn finish(&self) {
        let mut state = lock_unpoison(&self.shared.0);
        state.stopped = true;
        for (_, receipt) in state.records.values_mut() {
            if receipt.state == InputWriteState::Pending {
                receipt.state = InputWriteState::OutcomeUnknown;
            }
        }
        lock_unpoison(&self.sender).take();
        self.shared.1.notify_all();
    }
}
impl Drop for InputChannel {
    fn drop(&mut self) {
        self.finish();
    }
}

impl JobManager {
    pub(crate) fn write_input(
        &self,
        request: &JobInputRequest,
        policy: &RunnerPolicy,
        instance: &str,
    ) -> Result<JobInputReceipt, String> {
        request.validate()?;
        if request.runner_instance_id != instance {
            return Err("job_input_instance_changed".into());
        }
        if !policy.allow_raw_shell {
            return Err("job_input_disabled".into());
        }
        let channel = {
            let jobs = lock_unpoison(&self.jobs);
            let job = jobs.get(&request.job_id).ok_or("job_input_unavailable")?;
            if job.runner_instance_id != instance
                || job.snapshot.context.runtime_project_id.as_deref()
                    != Some(request.project.as_str())
            {
                return Err("job_input_target_mismatch".into());
            }
            let cwd = job
                .snapshot
                .context
                .cwd
                .as_deref()
                .ok_or("job_input_target_mismatch")?;
            cwd_allowed(policy, Path::new(cwd))?;
            job.input.clone().ok_or(
                if matches!(job.snapshot.status.as_str(), "queued" | "pending") {
                    "job_input_not_ready"
                } else {
                    "job_input_unavailable"
                },
            )?
        };
        channel.submit(&request.input_id, &request.data, request.close)
    }
}

#[cfg(test)]
mod tests;
