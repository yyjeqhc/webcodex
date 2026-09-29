//! A persisted service-manager selection, not a change of credential or user.
//! Missing scope in old records always means the historical system manager.
use super::*;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceScope {
    User,
    #[default]
    System,
}

impl ServiceScope {
    pub fn is_system(&self) -> bool {
        *self == Self::System
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
        }
    }
    /// This describes native lifecycle, never a claim that a service is live.
    pub fn lifecycle(self) -> &'static str {
        match self {
            Self::System => "system_startup",
            Self::User if cfg!(target_os = "linux") => "user_manager_linger_dependent",
            Self::User => "signed_in_user_session",
        }
    }
}
impl std::str::FromStr for ServiceScope {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            _ => Err("Service scope must be user or system".into()),
        }
    }
}

pub(super) fn user_account(spec: &ServiceSpec) -> Result<(&str, &Path), ServiceError> {
    match &spec.account {
        ServiceAccount::SystemUser { expected_identity, home: Some(home), group: None, .. }
            if !matches!(expected_identity.as_str(), "0" | "S-1-5-18" | "S-1-5-19" | "S-1-5-20")
                && !expected_identity.is_empty() && home.is_absolute() => Ok((expected_identity, home)),
        _ => Err(ServiceError::new(ServiceErrorCode::InvalidSpec,
            "User services require a real non-system owner with an absolute home, no Group or virtual account")),
    }
}

/// Never use sudo/runas to impersonate a user's service manager, nor rely on
/// caller-supplied HOME/USER as account authority. Used before all user effects.
pub(super) fn verify_current_user(spec: &ServiceSpec) -> Result<(), ServiceError> {
    let (id, home) = user_account(spec)?;
    let current = current_account()?;
    if id != current.identity || home != current.home {
        return Err(ServiceError::new(ServiceErrorCode::PermissionDenied,
            "Run this user service operation as the saved environment owner; no system fallback is allowed"));
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn ensure_user_service_directory(path: &Path, uid: u32) -> Result<(), ServiceError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() && meta.uid() == uid && meta.mode() & 0o022 == 0 => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| ServiceError::new(ServiceErrorCode::InvalidSpec,"User service directory has no parent"))?;
            ensure_user_service_directory(parent, uid)?;
            std::fs::DirBuilder::new().mode(0o700).create(path).map_err(|_| ServiceError::new(ServiceErrorCode::OperationFailed,"Could not create the owner service directory"))?;
            ensure_user_service_directory(path, uid)
        }
        _ => Err(ServiceError::new(ServiceErrorCode::PermissionDenied,"User service directory must be an owner-controlled directory, not a link or shared writable location")),
    }
}
