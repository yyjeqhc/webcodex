//! Additive wire types. Unknown command types are decoded but never dispatched.
use crate::deadline::ResponseTimeout;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

pub type Headers = BTreeMap<String, Vec<String>>;
#[derive(Deserialize)]
pub struct Command {
    pub request_id: String,
    pub shard_token: String,
    pub command_type: String,
    #[serde(default = "main_channel")]
    pub channel: String,
    #[serde(default)]
    pub headers: Headers,
    #[serde(default)]
    pub response_timeout: ResponseTimeout,
    #[serde(default)]
    pub jsonrpc: Option<Box<RawValue>>,
}
fn main_channel() -> String {
    "main".into()
}
#[derive(Deserialize)]
pub(crate) struct Envelope {
    pub commands: Vec<Command>,
}
#[derive(Serialize)]
pub(crate) struct Response<'a> {
    pub request_id: &'a str,
    pub channel: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resp_json: Option<Box<RawValue>>,
    pub resp_headers: Headers,
    pub resp_code: u16,
    pub resp_type: &'static str,
}
