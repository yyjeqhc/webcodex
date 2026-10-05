use std::fmt;

/// Deliberately contains no URLs, credentials, payloads, or upstream error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Protocol,
    Capacity,
    Authentication,
    Redirect,
    Transport,
    Deadline,
    Delivery,
    Uncertain,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Configuration => "invalid tunnel configuration",
            Self::Protocol => "invalid or unsupported tunnel exchange",
            Self::Capacity => "tunnel capacity exhausted",
            Self::Authentication => "control-plane authorization rejected",
            Self::Redirect => "tunnel redirect rejected",
            Self::Transport => "tunnel transport unavailable",
            Self::Deadline => "command deadline expired",
            Self::Delivery => "response delivery unconfirmed",
            Self::Uncertain => {
                "previous execution or delivery remains uncertain; automatic restart forbidden"
            }
        })
    }
}
impl std::error::Error for Error {}
