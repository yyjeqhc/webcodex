//! Two CDP transports, one action/observation implementation and one deadline /
//! certainty policy. External sockets are authenticated lease-scoped channels,
//! not caller-supplied endpoints or a parallel Browser automation API.
use super::{BrowserError, BrowserResult};
use crate::bridge::BridgeSocket;
use std::{io, net::TcpStream, time::Duration};
use tungstenite::{Message, WebSocket};

pub(super) enum CdpSocket {
    Loopback(Box<WebSocket<TcpStream>>),
    Bridge(Box<BridgeSocket>),
}
impl CdpSocket {
    pub(super) fn timeout(&mut self, timeout: Duration) -> BrowserResult<()> {
        match self {
            Self::Loopback(socket) => socket
                .get_mut()
                .set_read_timeout(Some(timeout))
                .and_then(|_| socket.get_mut().set_write_timeout(Some(timeout)))
                .map_err(|_| {
                    BrowserError::not_started(
                        "cdp_timeout_config_failed",
                        "Could not bound the CDP transport timeout",
                    )
                }),
            Self::Bridge(socket) => {
                socket.timeout = timeout;
                Ok(())
            }
        }
    }
    pub(super) fn send(&mut self, message: Message) -> tungstenite::Result<()> {
        match self {
            Self::Loopback(socket) => socket.send(message),
            Self::Bridge(socket) => {
                let Message::Text(text) = message else {
                    return Err(tungstenite::Error::Io(io::ErrorKind::InvalidData.into()));
                };
                let value = serde_json::from_str(&text)
                    .map_err(|_| tungstenite::Error::Io(io::ErrorKind::InvalidData.into()))?;
                socket.send(value).map_err(tungstenite::Error::Io)
            }
        }
    }
    pub(super) fn read(&mut self) -> tungstenite::Result<Message> {
        match self {
            Self::Loopback(socket) => socket.read(),
            Self::Bridge(socket) => socket
                .read()
                .map(|text| Message::Text(text.into()))
                .map_err(tungstenite::Error::Io),
        }
    }
}
