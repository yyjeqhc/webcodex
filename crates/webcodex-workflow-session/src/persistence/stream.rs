//! Decode and sanitize one ledger row at a time. Retain only final Hot/Cold
//! records, never a full source String or a second whole-ledger JSON Value tree.
use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

#[derive(Default)]
#[cfg(test)]
pub(super) struct Rows {
    pub records: Vec<StoredSession>,
    pub tombstones: Vec<SessionRetentionTombstone>,
}

#[cfg(test)]
struct RowsSeed(usize);
#[cfg(test)]
impl<'de> DeserializeSeed<'de> for RowsSeed {
    type Value = Rows;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Rows, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
#[cfg(test)]
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

#[cfg(test)]
struct LedgerSeed(usize);
#[cfg(test)]
impl<'de> DeserializeSeed<'de> for LedgerSeed {
    type Value = Rows;
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Rows, D::Error> {
        deserializer.deserialize_map(self)
    }
}
#[cfg(test)]
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

#[cfg(test)]
pub(super) fn load(reader: impl io::Read, max_events: usize) -> Result<Rows, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let rows = LedgerSeed(max_events).deserialize(&mut deserializer)?;
    // A complete bad document never publishes a valid-looking partial prefix.
    deserializer.end()?;
    Ok(rows)
}

/// Migration decodes one row at a time into the caller's SQLite transaction.
/// Invalid input aborts that transaction instead of publishing a partial import.
pub(super) enum ImportRow {
    Session(PersistedSessionRecord),
    Tombstone(SessionRetentionTombstone),
}

pub(super) fn visit(
    reader: impl io::Read,
    mut accept: impl FnMut(ImportRow) -> io::Result<()>,
) -> Result<(), serde_json::Error> {
    struct ImportRows<'a, F>(&'a mut F);
    impl<'de, F: FnMut(ImportRow) -> io::Result<()>> DeserializeSeed<'de> for ImportRows<'_, F> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_seq(self)
        }
    }
    impl<'de, F: FnMut(ImportRow) -> io::Result<()>> Visitor<'de> for ImportRows<'_, F> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("Session rows")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut rows: A) -> Result<(), A::Error> {
            while let Some(value) = rows.next_element::<Value>()? {
                if value.get("retention_tombstone").is_some() {
                    let tombstone = parse_retention_tombstone_row(&value)
                        .ok_or_else(|| de::Error::custom("invalid Session tombstone"))?;
                    (self.0)(ImportRow::Tombstone(tombstone)).map_err(de::Error::custom)?;
                    continue;
                }
                if !v2_record_has_canonical_logical_invocation_shape(&value) {
                    return Err(de::Error::custom("invalid Session correlation"));
                }
                let persisted: PersistedSessionRecord =
                    serde_json::from_value(value).map_err(de::Error::custom)?;
                let record = persisted
                    .into_record(usize::MAX)
                    .ok_or_else(|| de::Error::custom("invalid Session identity"))?;
                (self.0)(ImportRow::Session(PersistedSessionRecord::from_record(
                    &record,
                    usize::MAX,
                )))
                .map_err(de::Error::custom)?;
            }
            Ok(())
        }
    }
    struct ImportLedger<'a, F>(&'a mut F);
    impl<'de, F: FnMut(ImportRow) -> io::Result<()>> Visitor<'de> for ImportLedger<'_, F> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("version-2 Session ledger")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let (mut version, mut rows) = (false, false);
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "version" if !version => {
                        if map.next_value::<u32>()? != SESSION_LEDGER_VERSION {
                            return Err(de::Error::custom("unsupported Session ledger version"));
                        }
                        version = true;
                    }
                    "sessions" if !rows => {
                        map.next_value_seed(ImportRows(self.0))?;
                        rows = true;
                    }
                    _ => {
                        return Err(de::Error::custom(
                            "unknown or duplicate Session ledger member",
                        ))
                    }
                }
            }
            if !version || !rows {
                return Err(de::Error::custom("incomplete Session ledger"));
            }
            Ok(())
        }
    }
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    serde::Deserializer::deserialize_map(&mut deserializer, ImportLedger(&mut accept))?;
    deserializer.end()
}

#[cfg(test)]
mod tests;
