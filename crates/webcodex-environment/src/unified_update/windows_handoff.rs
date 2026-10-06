//! Private, ephemeral response exchange. These files carry selected identities
//! and acknowledgements; only Core's journal and guarded APIs authorize effects.
use super::{PrivateUpdateCache, UpdateError, UpdateResult};
use crate::{PreparedInstallationReceipt, UpgradeTarget};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const WINDOWS_GUARDED_HANDOFF_VERSION: u16 = 1;
const MAX_BYTES: u64 = 4096;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u16,
    launch_nonce: String,
    target: UpgradeTarget,
}
#[derive(Debug, Clone)]
pub struct WindowsHandoff {
    cache: PrivateUpdateCache,
    request: Envelope,
}
fn valid_target(target: &UpgradeTarget) -> bool {
    !target.environment_id.is_empty()
        && target.environment_id.len() <= 128
        && target
            .environment_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        && super::valid_sha256(&target.manifest_sha256)
        && target
            .operation_id
            .as_deref()
            .is_none_or(|id| uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id))
}
impl Envelope {
    fn valid(&self) -> bool {
        self.schema_version == WINDOWS_GUARDED_HANDOFF_VERSION
            && super::lowercase_hex(&self.launch_nonce, 32)
            && valid_target(&self.target)
    }
}
impl WindowsHandoff {
    #[cfg(any(windows, test))]
    pub(crate) fn create(cache: &PrivateUpdateCache, target: &UpgradeTarget) -> UpdateResult<Self> {
        if !valid_target(target) {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let cache = cache.child(&format!("handoff-{nonce}"))?;
        let request = Envelope {
            schema_version: WINDOWS_GUARDED_HANDOFF_VERSION,
            launch_nonce: nonce,
            target: target.clone(),
        };
        cache.write(
            "request.json",
            &serde_json::to_vec(&request).map_err(|_| UpdateError::CacheUnavailable)?,
        )?;
        Ok(Self { cache, request })
    }
    pub fn from_request_file(path: &Path) -> UpdateResult<Self> {
        if !path.is_absolute() || path.file_name().and_then(|n| n.to_str()) != Some("request.json")
        {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        let parent = path.parent().ok_or(UpdateError::UpgradePreflightFailed)?;
        let readonly = PrivateUpdateCache::open_existing(parent.to_path_buf())?
            .ok_or(UpdateError::CacheUnavailable)?;
        let bytes = readonly
            .read("request.json", MAX_BYTES)?
            .ok_or(UpdateError::CacheUnavailable)?;
        let request: Envelope =
            serde_json::from_slice(&bytes).map_err(|_| UpdateError::UpgradePreflightFailed)?;
        if !request.valid()
            || parent.file_name().and_then(|n| n.to_str())
                != Some(format!("handoff-{}", request.launch_nonce).as_str())
        {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        let cache = PrivateUpdateCache::open(parent.to_path_buf())?;
        Ok(Self { cache, request })
    }
    pub fn request_path(&self) -> UpdateResult<PathBuf> {
        self.cache.file("request.json")
    }
    pub fn selected_target(&self) -> &UpgradeTarget {
        &self.request.target
    }
    fn read(&self, name: &str) -> UpdateResult<Option<Envelope>> {
        let value = self
            .cache
            .read(name, MAX_BYTES)?
            .map(|bytes| {
                serde_json::from_slice::<Envelope>(&bytes)
                    .map_err(|_| UpdateError::RecoveryRequired)
            })
            .transpose()?;
        if value.as_ref().is_some_and(|value| {
            !value.valid()
                || value.launch_nonce != self.request.launch_nonce
                || value.target.environment_id != self.request.target.environment_id
                || value.target.manifest_sha256 != self.request.target.manifest_sha256
                || value.target.operation_id.is_none()
                || self
                    .request
                    .target
                    .operation_id
                    .as_ref()
                    .is_some_and(|expected| value.target.operation_id.as_ref() != Some(expected))
        }) {
            return Err(UpdateError::RecoveryRequired);
        }
        Ok(value)
    }
    pub fn prepared_operation(&self) -> UpdateResult<Option<String>> {
        self.read("prepared.json")
            .map(|value| value.and_then(|value| value.target.operation_id))
    }
    pub fn accepted_operation(&self) -> UpdateResult<Option<String>> {
        self.read("accepted.json")
            .map(|value| value.and_then(|value| value.target.operation_id))
    }
    /// Called with the receipt returned under the *preparing* Core lock, never
    /// with an operation inferred from a subsequent journal observation.
    pub fn report_prepared(&self, receipt: &PreparedInstallationReceipt) -> UpdateResult<()> {
        if receipt.environment_id != self.request.target.environment_id
            || receipt.manifest_sha256 != self.request.target.manifest_sha256
            || self
                .request
                .target
                .operation_id
                .as_ref()
                .is_some_and(|expected| expected != &receipt.operation_id)
            || !uuid::Uuid::parse_str(&receipt.operation_id)
                .is_ok_and(|uuid| uuid.to_string() == receipt.operation_id)
        {
            return Err(UpdateError::RecoveryRequired);
        }
        let mut acknowledgement = self.request.clone();
        acknowledgement.target.operation_id = Some(receipt.operation_id.clone());
        self.cache.write(
            "prepared.json",
            &serde_json::to_vec(&acknowledgement).map_err(|_| UpdateError::CacheUnavailable)?,
        )
    }
    /// Parent records the exact operation durably before allowing file replacement.
    #[cfg(any(windows, test))]
    pub(crate) fn accept_prepared(&self, operation_id: &str) -> UpdateResult<()> {
        let acknowledged = self
            .read("prepared.json")?
            .filter(|value| value.target.operation_id.as_deref() == Some(operation_id))
            .ok_or(UpdateError::RecoveryRequired)?;
        self.cache.write(
            "accepted.json",
            &serde_json::to_vec(&acknowledged).map_err(|_| UpdateError::CacheUnavailable)?,
        )
    }
    pub async fn wait_for_acceptance(&self, operation_id: &str) -> UpdateResult<()> {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if let Some(value) = self.read("accepted.json")? {
                    return if value.target.operation_id.as_deref() == Some(operation_id) {
                        Ok(())
                    } else {
                        Err(UpdateError::RecoveryRequired)
                    };
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| UpdateError::RecoveryRequired)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn handoff() -> (tempfile::TempDir, WindowsHandoff) {
        let temp = crate::test_tempdir().unwrap();
        let cache = PrivateUpdateCache::open(temp.path().join("cache")).unwrap();
        let handoff = WindowsHandoff::create(
            &cache,
            &UpgradeTarget {
                environment_id: "selected".into(),
                manifest_sha256: "a".repeat(64),
                operation_id: None,
            },
        )
        .unwrap();
        (temp, handoff)
    }
    fn acknowledgement(handoff: &WindowsHandoff) -> Envelope {
        let mut value = handoff.request.clone();
        value.target.operation_id = Some(uuid::Uuid::new_v4().to_string());
        value
    }
    #[test]
    fn acknowledgement_requires_same_launch_environment_candidate_and_operation() {
        let (_temp, handoff) = handoff();
        assert!(handoff.prepared_operation().unwrap().is_none());
        for field in ["nonce", "environment", "manifest", "operation"] {
            let mut value = acknowledgement(&handoff);
            match field {
                "nonce" => value.launch_nonce = "b".repeat(32),
                "environment" => value.target.environment_id = "different".into(),
                "manifest" => value.target.manifest_sha256 = "b".repeat(64),
                _ => value.target.operation_id = None,
            }
            handoff
                .cache
                .write("prepared.json", &serde_json::to_vec(&value).unwrap())
                .unwrap();
            assert!(handoff.prepared_operation().is_err(), "{field}");
        }
        let value = acknowledgement(&handoff);
        handoff
            .cache
            .write("prepared.json", &serde_json::to_vec(&value).unwrap())
            .unwrap();
        let operation = value.target.operation_id.unwrap();
        assert_eq!(
            handoff.prepared_operation().unwrap(),
            Some(operation.clone())
        );
        assert!(handoff
            .accept_prepared(&uuid::Uuid::new_v4().to_string())
            .is_err());
        handoff.accept_prepared(&operation).unwrap();
    }
    #[test]
    fn exchange_is_bounded_private_and_not_a_reused_receipt() {
        let (_temp, handoff) = handoff();
        let loaded = WindowsHandoff::from_request_file(&handoff.request_path().unwrap()).unwrap();
        assert_eq!(loaded.selected_target(), handoff.selected_target());
        handoff
            .cache
            .write("prepared.json", &vec![b'x'; MAX_BYTES as usize + 1])
            .unwrap();
        assert!(handoff.prepared_operation().is_err());
        assert!(
            WindowsHandoff::from_request_file(&handoff.cache.file("prepared.json").unwrap())
                .is_err()
        );
    }
    #[tokio::test]
    async fn acceptance_binds_exact_operation_before_preparation_can_return() {
        let (_temp, handoff) = handoff();
        let value = acknowledgement(&handoff);
        let operation = value.target.operation_id.clone().unwrap();
        handoff
            .cache
            .write("prepared.json", &serde_json::to_vec(&value).unwrap())
            .unwrap();
        handoff.accept_prepared(&operation).unwrap();
        handoff.wait_for_acceptance(&operation).await.unwrap();
        assert!(handoff
            .wait_for_acceptance(&uuid::Uuid::new_v4().to_string())
            .await
            .is_err());
        let mut resumed = handoff.clone();
        resumed.request.target.operation_id = Some(uuid::Uuid::new_v4().to_string());
        assert!(resumed.prepared_operation().is_err());
    }
}
