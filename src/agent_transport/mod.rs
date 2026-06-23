//! Agent transport — connection layer for remote agents.
//!
//! This module handles communication with remote agent instances.
//! Currently uses HTTP polling; will migrate to WebSocket.

pub mod protocol;
pub mod registry;
