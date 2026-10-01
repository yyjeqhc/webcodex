//! Detached Job ownership substrate for Runner-restart survival.
//!
//! This is deliberately separate from `webcodex-process::ManagedChild`'s
//! ordinary ownership path. A detached execution is prepared into bounded
//! Runner-owned durable state, then handed once to a narrow supervisor process.
//! The supervisor becomes the only payload process-tree owner after the durable
//! `OwnershipAccepted` transition. No public/general detach API lives here.

use super::output_text::{OutputTextDecoder, OutputTextSource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(unix, windows))]
use std::fs::{File, OpenOptions};
#[cfg(any(unix, windows))]
use std::io::{Read, Write};

#[cfg(any(unix, windows))]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(any(unix, windows))]
use std::sync::mpsc;
#[cfg(any(unix, windows))]
use std::time::Instant;
use uuid::Uuid;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runner_protocol::{
    validate_process_argv, ShellCommandExecutionState, ShellJobActivity, ShellJobActivityPhase,
    ShellJobActivitySource, ShellJobActivityState, ShellJobContext, ShellJobSnapshot,
    ShellJobStreamSnapshot, ShellProcessArgv, JOB_INVENTORY_MAX_JOBS,
    JOB_SNAPSHOT_STREAM_MAX_BYTES, JOB_TERMINAL_RETENTION_SECS, PROCESS_CWD_MAX_BYTES,
    PROCESS_STDIN_MAX_BYTES, PROCESS_TIMEOUT_MAX_SECS, STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};

#[cfg(any(unix, windows))]
use webcodex_process::ManagedChild;

#[cfg(test)]
mod tests;

mod model;
use model::*;
mod store;
use store::*;
mod supervisor;
use supervisor::*;
mod platform;
use platform::*;

pub(crate) use model::snapshot_from_detached_record;
pub(crate) use model::DetachedHandoffOutcome;
pub(crate) use model::DetachedJobPhase;
pub(crate) use model::DetachedJobRecord;
pub(crate) use model::DetachedLaunchSpec;
pub(crate) use model::DetachedOutputState;
pub(crate) use model::DetachedProcessIdentity;
pub(crate) use model::DetachedStartRequest;
pub(crate) use model::DETACHED_CHECKPOINT_INTERVAL;
pub(crate) use model::DETACHED_HANDOFF_TIMEOUT;
pub(crate) use model::DETACHED_LAUNCH_MAX_BYTES;
pub(crate) use model::DETACHED_STATE_MAX_BYTES;
pub(crate) use model::DETACHED_STATE_MAX_RECORDS;
pub(crate) use model::DETACHED_STATE_SCHEMA_VERSION;
pub(crate) use store::DetachedJobStore;
pub(crate) use supervisor::handoff_detached_job;
pub(crate) use supervisor::maybe_run_internal_mode;
