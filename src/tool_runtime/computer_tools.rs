//! Computer gateways, shared limits and private operation modules.

use super::files::{validate_artifact_file_path, validate_artifact_mime_for_path};
use super::sessions::SessionTransport;
use super::shell::{dispatch_uncertainty_lifecycle, runner_command_lifecycle};
use super::specialized::{
    SpecializedGovernanceDenial, SpecializedOperationPolicy, SpecializedSource,
};
use super::tool_call::{ComputerControlToolCall, ComputerObserveToolCall, ComputerSnapshotRegion};
use super::{RecoveryKind, SuggestedToolCall, ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::auth::{
    AuthContext, SCOPE_COMPUTER_CLIPBOARD_READ, SCOPE_COMPUTER_CLIPBOARD_WRITE,
    SCOPE_COMPUTER_CONTROL, SCOPE_COMPUTER_DISPLAY_READ, SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_POINTER_CONTROL, SCOPE_COMPUTER_READ,
};
use crate::runner_http::RunnerFeature;
use crate::runner_protocol::{ShellCommandExecutionState, ShellFileOpRequest};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

const MAX_WINDOWS: usize = 64;
const MAX_APPLICATIONS: usize = 64;
const MAX_DISPLAYS: usize = 16;
const MAX_APPLICATION_ID_BYTES: usize = 128;
const MAX_DISPLAY_ID_BYTES: usize = 128;
const MAX_RAW_CAPTURE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_INPUT_TEXT_BYTES: usize = 2048;
const MAX_CLIPBOARD_TEXT_BYTES: usize = 16 * 1024;
const MAX_ACCESSIBILITY_DEPTH: usize = 8;
const MAX_ACCESSIBILITY_NODES: usize = 256;
const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
const MAX_ACCESSIBILITY_CHILD_COUNT: u64 = 1_000_000;
const MAX_IMAGE_DIMENSION: u64 = 4096;
const COMPUTER_WAIT_SECS: u64 = 30;
const MAX_COMPUTER_TARGETS: usize = 64;
const DEFAULT_FIND_ELEMENTS_LIMIT: usize = 8;
const MAX_FIND_ELEMENTS_LIMIT: usize = 32;
const COMPUTER_KEY_INPUT_KEYS: &[&str] = &[
    "enter",
    "escape",
    "tab",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "page_up",
    "page_down",
    "home",
    "end",
];
const COMPUTER_KEY_INPUT_MODIFIERS: &[&str] = &["shift", "control", "option", "command"];
mod accessibility;
mod control;
mod effects;
mod gateway;
mod input_receipts;
mod inputs;
mod observe;
mod request;
mod responses;
mod screens;
mod snapshot_receipts;
mod snapshots;
#[cfg(test)]
use accessibility::{
    filter_accessibility_tree, node_matches_find_query, validate_accessibility_status,
    validate_accessibility_tree, validate_computer_element_state,
};
#[cfg(test)]
use effects::{
    classify_runner_error, computer_application_effect_delivery_failure,
    computer_application_effect_not_started, computer_application_effect_outcome_unknown,
    computer_application_launch_runner_error, computer_clipboard_write_delivery_failure,
    computer_clipboard_write_runner_error, computer_effect_delivery_failure,
    computer_effect_outcome_unknown, computer_effect_validated_result, computer_error,
    computer_error_with_client, computer_pointer_effect_delivery_failure,
    computer_pointer_runner_error, computer_request_is_effect, computer_text_input_runner_error,
    ClipboardWriteContext, PointerRequestContext,
};
#[cfg(test)]
use gateway::{computer_control_policy, computer_observe_policy};
#[cfg(test)]
use input_receipts::{
    validate_computer_activate_window, validate_computer_control, validate_computer_input_text,
    validate_computer_key_input, validate_computer_launch_application, validate_computer_pointer,
    validate_computer_read_clipboard, validate_computer_scroll_to_element,
    validate_computer_write_clipboard,
};
#[cfg(test)]
use inputs::{
    effective_snapshot_dimension_bound, normalize_computer_key_input, valid_application_id,
    valid_display_id, validate_clipboard_write_text, validate_input_text,
};
#[cfg(test)]
use screens::{validate_application_list, validate_display_list, validate_window_list};
#[cfg(test)]
use snapshot_receipts::{sha256_hex, validate_display_snapshot, validate_snapshot};
#[cfg(test)]
use snapshots::{
    computer_snapshot_artifact_definite_failure, computer_snapshot_artifact_lifecycle_failure,
};

#[cfg(test)]
#[path = "computer_tools_tests.rs"]
mod tests;
