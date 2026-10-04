//! Decode and sanitize one ledger row at a time. Retain only final Hot/Cold
//! records, never a full source String or a second whole-ledger JSON Value tree.
use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

#[derive(Default)]
pub(super) struct Rows {
    pub records: Vec<StoredSession>,
    pub tombstones: Vec<SessionRetentionTombstone>,
}

struct RowsSeed(usize);
impl<'de> DeserializeSeed<'de> for RowsSeed {
    type Value = Rows;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Rows, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for RowsSeed {
    type Value = Rows;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a Session row array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut rows: A) -> Result<Rows, A::Error> {
        let mut out = Rows::default();
        while let Some(value) = rows.next_element::<Value>()? {
            if value.get("retention_tombstone").is_some() {
                if let Some(tombstone) = parse_retention_tombstone_row(&value) {
                    out.tombstones.push(tombstone);
                } else {
                    tracing::warn!("discarding malformed v2 retention tombstone row");
                }
                continue;
            }
            if !v2_record_has_canonical_logical_invocation_shape(&value) {
                tracing::warn!(
                    "discarding malformed v2 Session row: event correlation shape is partial"
                );
                continue;
            }
            let record = match serde_json::from_value::<PersistedSessionRecord>(value) {
                Ok(record) => record,
                Err(_) => {
                    tracing::warn!("discarding malformed v2 Session row");
                    continue;
                }
            };
            let Some(record) = record.into_record(self.0) else {
                continue;
            };
            // Validate/sanitize one row before publication, then release its
            // expanded tree regardless of lifecycle. Startup never retains every
            // Active record hot; exact mutation will hydrate only its target.
            let stored = match cold_session_from_record(&record, self.0) {
                Ok(cold) => StoredSession::Cold(cold),
                Err(_) => StoredSession::Hot(record),
            };
            out.records.push(stored);
        }
        Ok(out)
    }
}

struct LedgerSeed(usize);
impl<'de> DeserializeSeed<'de> for LedgerSeed {
    type Value = Rows;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Rows, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for LedgerSeed {
    type Value = Rows;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("the v2 Session ledger")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Rows, A::Error> {
        let mut version = None;
        let mut rows = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "version" => {
                    if version.is_some() {
                        return Err(de::Error::duplicate_field("version"));
                    }
                    let value = map.next_value::<u32>()?;
                    if value != SESSION_LEDGER_VERSION {
                        return Err(de::Error::custom(format!(
                            "unsupported session ledger version {value}"
                        )));
                    }
                    version = Some(value);
                }
                "sessions" => {
                    if rows.is_some() {
                        return Err(de::Error::duplicate_field("sessions"));
                    }
                    rows = Some(map.next_value_seed(RowsSeed(self.0))?);
                }
                _ => return Err(de::Error::custom("unknown Session ledger field")),
            }
        }
        if version.is_none() {
            return Err(de::Error::missing_field("version"));
        }
        rows.ok_or_else(|| de::Error::missing_field("sessions"))
    }
}

pub(super) fn load(reader: impl io::Read, max_events: usize) -> Result<Rows, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let rows = LedgerSeed(max_events).deserialize(&mut deserializer)?;
    // A complete bad document never publishes a valid-looking partial prefix.
    deserializer.end()?;
    Ok(rows)
}

#[cfg(test)]
mod tests;
