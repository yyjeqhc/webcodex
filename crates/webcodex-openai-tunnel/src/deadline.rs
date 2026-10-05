use serde::{Deserialize, Deserializer};
use std::time::Duration;

/// Wire decoding preserves malformed timing metadata without rejecting a command.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ResponseTimeout {
    #[default]
    Absent,
    Valid(Duration),
    Invalid,
}
impl<'de> Deserialize<'de> for ResponseTimeout {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(match value {
            serde_json::Value::Null => Self::Absent,
            serde_json::Value::String(s) => parse(&s).map(Self::Valid).unwrap_or(Self::Invalid),
            _ => Self::Invalid,
        })
    }
}
fn parse(s: &str) -> Option<Duration> {
    let end = s.bytes().position(|b| !b.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let n: u64 = s[..end].parse().ok()?;
    let scale = match &s[end..] {
        "ns" => 1,
        "us" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60_000_000_000,
        "h" => 3_600_000_000_000,
        _ => return None,
    };
    let ns = n.checked_mul(scale)?;
    (ns <= i64::MAX as u64).then(|| Duration::from_nanos(ns))
}
