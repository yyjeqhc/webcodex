use serde::{Deserialize, Serialize};

use super::{
    RunnerJobUpdateRequest, RunnerPersistentShellResultRequest, RunnerRegisterRequest,
    RunnerRequest, RunnerResultPayload, RunnerView, ShellProjectInventoryPage,
    ShellProjectInventoryStatus, ToolProvidersStatus,
};

// ============================================================================
// Transport-neutral Runner message envelope
// ============================================================================
//
// A single message format used by the WebSocket Runner transport (and future
// QUIC transport). It wraps the existing polling protocol payloads so the
// server and Runner never duplicate business logic: register/request/result/
// job_update reuse the same structs as the HTTP polling endpoints.
//
// Wire format is JSON with an internal `type` tag:
//
//   {"type":"register","client_id":"...","projects":[...]}
//   {"type":"registered","success":true,"client":{...}}
//   {"type":"request","request_id":"...","client_id":"...","kind":"run_shell",...}
//   {"type":"result","client_id":"...","request_id":"...","exit_code":0,...}
//   {"type":"job_update","client_id":"...","job_id":"...","status":"running",...}
//   {"type":"ping","ts":1700000000}
//   {"type":"pong","ts":1700000000}
//   {"type":"goodbye","reason":"shutdown"}
//   {"type":"error","code":"bad_request","message":"..."}
//
// The envelope is transport-neutral: it carries no WebSocket-specific fields
// and could be framed over QUIC streams unchanged.

/// One Runner transport message. Used by both the server WebSocket handler and
/// the `webcodex-runner` WebSocket client mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerEnvelope {
    /// Runner -> server registration application envelope. WebSocket sends this
    /// after its authenticated HTTP handshake. QUIC keeps authentication in its
    /// transport-specific first-register codec and enters the shared envelope
    /// lifecycle only after transport authentication succeeds.
    Register {
        #[serde(flatten)]
        payload: RunnerRegisterRequest,
    },
    /// Server -> Runner. Acknowledgement of `Register`.
    Registered {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        client: Option<RunnerView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Server -> Runner. A pending shell/file/job request pushed to the Runner.
    /// Same payload as the `request` field of the polling response.
    Request {
        #[serde(flatten)]
        request: RunnerRequest,
    },
    /// Runner -> server. Result of a synchronous shell/file request. Same
    /// payload as `POST /api/shell/agent/result`.
    Result {
        #[serde(flatten)]
        payload: RunnerResultPayload,
    },
    /// Runner -> server. Incremental or final update for an async job. Same
    /// payload as `POST /api/shell/agent/job_update`.
    JobUpdate {
        #[serde(flatten)]
        payload: RunnerJobUpdateRequest,
    },
    /// Runner -> server result for a persistent-shell lifecycle request.
    PersistentShellResult {
        #[serde(flatten)]
        payload: RunnerPersistentShellResultRequest,
    },
    /// Either direction. Liveness keepalive.
    Ping { ts: i64 },
    /// Runner -> server changed-only sanitized runtime metadata. It reuses the
    /// active transport and never requires an acknowledgement round trip.
    RuntimeMetadata {
        tool_providers: ToolProvidersStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp_gateway_providers: Option<Vec<crate::mcp_gateway::McpGatewayProvider>>,
    },
    /// Runner -> Server bounded page of one project-inventory snapshot. New
    /// Runners send this only after the Registered view proved support.
    ProjectInventoryPage {
        #[serde(flatten)]
        page: ShellProjectInventoryPage,
    },
    /// Server -> Runner status acknowledgement for a project inventory page.
    ProjectInventoryStatus { status: ShellProjectInventoryStatus },
    /// Either direction. Reply to `Ping`.
    Pong { ts: i64 },
    /// Runner -> server. Best-effort graceful shutdown notice. Older Runners do
    /// not send this frame; transports still reconcile on observed disconnect.
    Goodbye {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Server -> Runner. Fatal protocol error; the Runner should reconnect.
    Error { code: String, message: String },
}

impl RunnerEnvelope {
    /// Short discriminator string for a variant, e.g. `"register"`. Useful
    /// for logging and tests.
    pub fn kind(&self) -> &'static str {
        match self {
            RunnerEnvelope::Register { .. } => "register",
            RunnerEnvelope::Registered { .. } => "registered",
            RunnerEnvelope::Request { .. } => "request",
            RunnerEnvelope::Result { .. } => "result",
            RunnerEnvelope::JobUpdate { .. } => "job_update",
            RunnerEnvelope::PersistentShellResult { .. } => "persistent_shell_result",
            RunnerEnvelope::Ping { .. } => "ping",
            RunnerEnvelope::RuntimeMetadata { .. } => "runtime_metadata",
            RunnerEnvelope::ProjectInventoryPage { .. } => "project_inventory_page",
            RunnerEnvelope::ProjectInventoryStatus { .. } => "project_inventory_status",
            RunnerEnvelope::Pong { .. } => "pong",
            RunnerEnvelope::Goodbye { .. } => "goodbye",
            RunnerEnvelope::Error { .. } => "error",
        }
    }

    /// Encode the envelope as a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Decode an envelope from a JSON byte slice.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// QUIC-v1 transport registration wire. Authentication remains transport-owned
/// while the first frame carries `type=register`, the complete canonical 0.4
/// registration payload, and an optional `auth_token`.
///
/// Deliberately does not implement `Debug`: the credential must not become
/// printable through routine transport diagnostics.
#[derive(Serialize, Deserialize)]
pub struct QuicRegisterFrame {
    #[serde(rename = "type")]
    frame_type: QuicRegisterFrameType,
    #[serde(flatten)]
    payload: RunnerRegisterRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_token: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum QuicRegisterFrameType {
    #[serde(rename = "register")]
    Register,
}

impl QuicRegisterFrame {
    pub fn new(payload: RunnerRegisterRequest, auth_token: Option<String>) -> Self {
        Self {
            frame_type: QuicRegisterFrameType::Register,
            payload,
            auth_token,
        }
    }

    pub fn payload_mut(&mut self) -> &mut RunnerRegisterRequest {
        &mut self.payload
    }

    pub fn into_parts(self) -> (RunnerRegisterRequest, Option<String>) {
        (self.payload, self.auth_token)
    }
}

// ============================================================================
// QUIC length-prefixed frame codec
// ============================================================================
//
// The custom QUIC Runner transport frames each [`RunnerEnvelope`] as:
//
//   u32_be length (big-endian)
//   JSON bytes
//
// Length-prefixing (rather than newline-delimited JSON) avoids boundary
// problems when a payload contains embedded newlines. The codec lives in this
// shared module so the server (`runner_quic.rs`) and the `webcodex-runner`
// binary (which inlines this file) use byte-identical framing.
//
// This is a custom QUIC *stream* transport, NOT HTTP/3. It is transport-
// neutral framing over a single QUIC bidirectional stream.

/// Shared upper bound for one serialized Runner transport envelope.
///
/// WebSocket and QUIC use this directly. Polling keeps the ordinary global HTTP
/// body limit for other routes but gives the authenticated Runner result route
/// this same bounded allowance so native image results do not depend on the
/// smaller general-purpose text request budget.
pub const RUNNER_ENVELOPE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Maximum QUIC frame body size. Keep this identical to the shared Runner
/// envelope bound so transport choice does not change result capacity.
pub const QUIC_FRAME_MAX_BYTES: usize = RUNNER_ENVELOPE_MAX_BYTES;

/// Errors produced by the QUIC frame codec.
#[derive(Debug)]
pub enum QuicFrameError {
    /// Underlying I/O error reading/writing the stream.
    Io(std::io::Error),
    /// JSON encode/decode failure.
    Json(serde_json::Error),
    /// Announced frame length exceeds `QUIC_FRAME_MAX_BYTES`. `len` is the
    /// announced (attacker-controlled) length; rejected before allocation.
    Oversized { len: usize, max: usize },
    /// The peer closed the stream cleanly before any frame was read.
    EmptyStream,
    /// A frame header announced a length but the body was short / invalid.
    Malformed(&'static str),
}

impl std::fmt::Display for QuicFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuicFrameError::Io(e) => write!(f, "quic frame io error: {}", e),
            QuicFrameError::Json(e) => write!(f, "quic frame json error: {}", e),
            QuicFrameError::Oversized { len, max } => write!(
                f,
                "quic frame oversized: announced {} bytes, max {}",
                len, max
            ),
            QuicFrameError::EmptyStream => write!(f, "quic stream closed before any frame"),
            QuicFrameError::Malformed(msg) => write!(f, "quic frame malformed: {}", msg),
        }
    }
}

impl std::error::Error for QuicFrameError {}

fn encode_quic_json<T: Serialize>(value: &T) -> Result<Vec<u8>, QuicFrameError> {
    let json = serde_json::to_vec(value).map_err(QuicFrameError::Json)?;
    // u32 cap is far above QUIC_FRAME_MAX_BYTES, but guard anyway so a
    // pathological payload can never overflow the length prefix.
    if json.len() > QUIC_FRAME_MAX_BYTES {
        return Err(QuicFrameError::Oversized {
            len: json.len(),
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let len = u32::try_from(json.len()).expect("checked against MAX");
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Encode an envelope as a length-prefixed frame: `u32_be(len) || json`.
pub fn encode_quic_frame(env: &RunnerEnvelope) -> Result<Vec<u8>, QuicFrameError> {
    encode_quic_json(env)
}

/// Encode the QUIC-v1 transport-owned register frame without routing its
/// credential through [`RunnerEnvelope`].
pub fn encode_quic_register_frame(frame: &QuicRegisterFrame) -> Result<Vec<u8>, QuicFrameError> {
    encode_quic_json(frame)
}

/// Write a single length-prefixed frame to an async sink.
pub async fn write_quic_frame<W>(w: &mut W, env: &RunnerEnvelope) -> Result<(), QuicFrameError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_quic_frame(env)?;
    w.write_all(&buf).await.map_err(QuicFrameError::Io)?;
    Ok(())
}

/// Write the transport-owned QUIC-v1 registration frame.
pub async fn write_quic_register_frame<W>(
    w: &mut W,
    frame: &QuicRegisterFrame,
) -> Result<(), QuicFrameError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_quic_register_frame(frame)?;
    w.write_all(&buf).await.map_err(QuicFrameError::Io)?;
    Ok(())
}

async fn read_quic_frame_body<R>(r: &mut R) -> Result<Vec<u8>, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(QuicFrameError::EmptyStream);
        }
        Err(e) => return Err(QuicFrameError::Io(e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > QUIC_FRAME_MAX_BYTES {
        // Reject before allocating. `len` is peer-controlled.
        return Err(QuicFrameError::Oversized {
            len,
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            QuicFrameError::Malformed("announced frame length but stream ended early")
        } else {
            QuicFrameError::Io(e)
        }
    })?;
    Ok(buf)
}

/// Read a single length-prefixed shared application envelope.
pub async fn read_quic_frame<R>(r: &mut R) -> Result<RunnerEnvelope, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let buf = read_quic_frame_body(r).await?;
    RunnerEnvelope::from_slice(&buf).map_err(QuicFrameError::Json)
}

/// Read the transport-owned first QUIC-v1 registration frame. A structurally
/// valid registration with a missing token decodes successfully so the Server
/// can preserve the existing external `unauthorized` behavior.
pub async fn read_quic_register_frame<R>(r: &mut R) -> Result<QuicRegisterFrame, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let buf = read_quic_frame_body(r).await?;
    serde_json::from_slice(&buf).map_err(QuicFrameError::Json)
}
