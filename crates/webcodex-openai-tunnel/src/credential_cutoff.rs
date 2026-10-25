//! Enqueue-time authority, independent from response deadlines.
use chrono::{DateTime, FixedOffset};
use serde_json::value::RawValue;

/// An operator-confirmed future UTC boundary, not a machine-verified clock or
/// control-plane drain acknowledgement. See `attest_future_utc`.
#[derive(Clone, Copy, Debug)]
pub struct OperatorEnqueueCutoff(pub(crate) u64);

impl OperatorEnqueueCutoff {
    /// The caller explicitly attests that this UTC timestamp was checked against
    /// a trusted time source and is after this transition operation began.
    /// Syntax validation alone cannot prove that a wall clock is correct.
    /// Retrying an already authorized exact transition retains its original
    /// attestation; it must not invent a later cutoff or a new operation.
    pub fn attest_future_utc(value: &str) -> Result<Self, &'static str> {
        let date = timestamp(value).ok_or("tunnel_cutoff_invalid")?;
        if date.offset().local_minus_utc() != 0
            || date.timestamp_subsec_nanos() != 0
            || date.timestamp() <= 0
        {
            return Err("tunnel_cutoff_requires_whole_utc_second");
        }
        Ok(Self(date.timestamp() as u64))
    }
}

fn timestamp(value: &str) -> Option<DateTime<FixedOffset>> {
    if value.len() > 64 {
        return None;
    }
    let date = DateTime::parse_from_rfc3339(value).ok()?;
    // Leap seconds and expanded years have no unambiguous representation in
    // the persisted Unix-second boundary; reject rather than round them.
    (date.timestamp_subsec_nanos() < 1_000_000_000
        && (0..=253_402_300_799).contains(&date.timestamp()))
    .then_some(date)
}

fn enqueue_time(raw: Option<&RawValue>) -> Option<DateTime<FixedOffset>> {
    let raw = raw.filter(|r| r.get().len() <= 80)?;
    timestamp(&serde_json::from_str::<String>(raw.get()).ok()?)
}

/// (timestamp verified syntactically, admitted by the optional authority cutoff).
pub(crate) fn admission(raw: Option<&RawValue>, cutoff: Option<u64>) -> (bool, bool) {
    let date = enqueue_time(raw);
    let admitted = cutoff.is_none_or(|cutoff| {
        date.is_some_and(|date| {
            let seconds = date.timestamp() as u64;
            seconds > cutoff || (seconds == cutoff && date.timestamp_subsec_nanos() > 0)
        })
    });
    (date.is_some(), admitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_boundary_requires_unambiguous_whole_utc_seconds() {
        for invalid in [
            "invalid",
            "1970-01-01T00:00:00Z",
            "1969-12-31T23:59:59Z",
            "2099-01-01T00:00:00.1Z",
            "2099-01-01T01:00:00+01:00",
            "2016-12-31T23:59:60Z",
            "+10000-01-01T00:00:00Z",
        ] {
            assert!(OperatorEnqueueCutoff::attest_future_utc(invalid).is_err());
        }
        assert_eq!(
            OperatorEnqueueCutoff::attest_future_utc("2099-01-01T00:00:00Z")
                .unwrap()
                .0,
            OperatorEnqueueCutoff::attest_future_utc("2099-01-01T00:00:00+00:00")
                .unwrap()
                .0,
        );
    }

    #[test]
    fn admission_preserves_offsets_and_precision_without_rounding_old_requests() {
        let cutoff = OperatorEnqueueCutoff::attest_future_utc("2099-01-01T00:00:00Z")
            .unwrap()
            .0;
        for (stamp, expected) in [
            ("2099-01-01T00:00:00Z", false),
            ("2099-01-01T00:00:00.000000001Z", true),
            ("2099-01-01T01:00:00+01:00", false),
            ("2099-01-01T01:00:00.000000001+01:00", true),
            ("2098-12-31T23:59:59.999999999Z", false),
            ("2098-12-31T23:59:60Z", false),
        ] {
            let raw = RawValue::from_string(serde_json::to_string(stamp).unwrap()).unwrap();
            assert_eq!(admission(Some(&raw), Some(cutoff)).1, expected);
        }
        for raw in ["null", "42", "{}", "[]", "true", "\"invalid\""] {
            let raw = RawValue::from_string(raw.to_owned()).unwrap();
            assert_eq!(admission(Some(&raw), Some(cutoff)), (false, false));
        }
        assert_eq!(admission(None, Some(cutoff)), (false, false));
        assert_eq!(admission(None, None), (false, true));
    }
}
