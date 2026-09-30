//! Read fences use one positive JSON-safe integer domain. Some MCP adapters
//! serialize an integral JSON number with `.0`; accept that representation,
//! never round a fractional value, parse a string, clamp, or invent a fence.
use serde::de::{self, Deserializer, Visitor};
use std::fmt;

const MAX: u64 = 9_007_199_254_740_991;

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    struct Revision;
    impl<'de> Visitor<'de> for Revision {
        type Value = u64;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a positive JSON-safe integer read revision")
        }
        fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
            if (1..=MAX).contains(&value) {
                Ok(value)
            } else {
                Err(E::custom(
                    "expected_read_revision must be a positive JSON-safe integer",
                ))
            }
        }
        fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
            u64::try_from(value)
                .map_err(|_| E::custom("read revision must be positive"))
                .and_then(|v| self.visit_u64(v))
        }
        fn visit_f64<E: de::Error>(self, value: f64) -> Result<u64, E> {
            if !value.is_finite() || value < 1.0 || value > MAX as f64 || value.fract() != 0.0 {
                return Err(E::custom(
                    "read revision must be an exact positive JSON-safe integer",
                ));
            }
            // Every integer in this domain has an exact binary64 representation.
            self.visit_u64(value as u64)
        }
    }
    deserializer.deserialize_any(Revision)
}

#[cfg(test)]
mod tests;
