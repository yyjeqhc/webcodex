use super::super::config::{RunnerPolicy, ShellConfig};
use super::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerEnvelope, RunnerJobUpdateRequest, RunnerRequest,
    RUNNER_PROTOCOL_GENERATION_V2,
};
#[cfg(all(unix, feature = "runner-real-process-tests"))]
use crate::POLLING_DISPATCH_MAX_IN_FLIGHT;
use futures_util::{SinkExt, StreamExt};
use std::io::{Read, Write};
use std::net::{TcpListener as StdTcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as WsMessage;
include!("transport_tests/stream_telemetry.rs");
include!("transport_tests/runtime_shutdown.rs");
include!("transport_tests/polling_real_process.rs");
include!("transport_tests/polling_recovery.rs");
include!("transport_tests/transport_semantics.rs");
include!("transport_tests/project_inventory.rs");
include!("transport_tests/streaming_proxy_quic.rs");
