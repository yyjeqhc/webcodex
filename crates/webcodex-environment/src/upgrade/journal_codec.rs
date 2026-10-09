//! Journal-only Runtime encoding. The closed phase string deliberately makes
//! historical readers reject Runtime journals before any recovery mutation.
//! The existing remote derive owns the fields; internal transaction phases and
//! all other Phase consumers retain their ordinary representation.
use super::{PackageFlavor, Phase, UpgradeJournal};
use crate::unified_update::InstallerTarget;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

struct Fields<'a>(&'a UpgradeJournal);

fn valid_target(flavor: PackageFlavor, platform: &str, target: Option<InstallerTarget>) -> bool {
    match (flavor, target) {
        (PackageFlavor::Full, None) => true,
        (PackageFlavor::Runtime, Some(target)) => {
            target.flavor == PackageFlavor::Runtime
                && target.valid()
                && target.platform.as_str() == platform
        }
        _ => false,
    }
}

impl Serialize for Fields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UpgradeJournal::serialize(self.0, serializer)
    }
}

impl Serialize for UpgradeJournal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.schema_version != self.candidate.package_flavor.source_schema()
            || !valid_target(
                self.candidate.package_flavor,
                &self.candidate.platform,
                self.installer_target,
            )
        {
            return Err(serde::ser::Error::custom(
                "upgrade journal schema/flavor/target mismatch",
            ));
        }
        if self.candidate.package_flavor.is_full() {
            // Direct delegation retains the exact legacy fields, defaults,
            // ordering and ordinary Phase encoding for Full schema 1.
            return UpgradeJournal::serialize(self, serializer);
        }
        let mut fields = serde_json::to_value(Fields(self)).map_err(serde::ser::Error::custom)?;
        let phase = fields
            .get("phase")
            .and_then(Value::as_str)
            .ok_or_else(|| serde::ser::Error::custom("upgrade journal phase missing"))?;
        fields["phase"] = Value::String(format!("runtime_{phase}"));
        fields.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UpgradeJournal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let invalid = || serde::de::Error::custom("upgrade journal schema/flavor/phase mismatch");
        // Preserve duplicate-field rejection while buffering the discriminator.
        // Production private journal readers already impose the metadata bound.
        let mut fields = crate::unified_update::deserialize_unique(deserializer)?;
        let schema = fields
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let candidate = fields
            .get("candidate")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let flavor = match candidate.get("package_flavor") {
            None => PackageFlavor::Full,
            Some(value) => {
                serde_json::from_value::<PackageFlavor>(value.clone()).map_err(|_| invalid())?
            }
        };
        let platform = candidate
            .get("platform")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let target = match fields.get("installer_target") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                serde_json::from_value::<InstallerTarget>(value.clone()).map_err(|_| invalid())?,
            ),
        };
        if !valid_target(flavor, platform, target) {
            return Err(invalid());
        }
        let encoded_phase = fields
            .get("phase")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let phase = match (schema, flavor) {
            (1, PackageFlavor::Full) => encoded_phase,
            (2, PackageFlavor::Runtime) => {
                encoded_phase.strip_prefix("runtime_").ok_or_else(invalid)?
            }
            _ => return Err(invalid()),
        };
        let phase: Phase =
            serde_json::from_value(Value::String(phase.into())).map_err(|_| invalid())?;
        fields["phase"] = serde_json::to_value(phase).map_err(serde::de::Error::custom)?;
        UpgradeJournal::deserialize(fields).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
