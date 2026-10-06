//! The historical request decoder has a closed EnvironmentMode enum but ignores
//! unknown struct fields. Use an explicit new mode tag as well as scope, so an
//! old CLI cannot ignore `service_scope=user` and operate the system namespace.
//! These are encoding variants only; runtime business modes stay Create/Join.
use super::*;
use crate::service::ServiceScope;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ModeWire {
    Create { listen: String },
    Join,
    UserCreate { listen: String },
    UserJoin,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct SetupRequestWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_scope: Option<ServiceScope>,
    mode: ModeWire,
    server_url: String,
    project: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runner: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runner_display_name: Option<String>,
    account: LocalAccount,
    binaries: RuntimeBinaries,
}
impl From<SetupRequest> for SetupRequestWire {
    fn from(request: SetupRequest) -> Self {
        let SetupRequest {
            service_scope,
            mode,
            server_url,
            project,
            runner,
            runner_display_name,
            account,
            binaries,
        } = request;
        let mode = match (service_scope, mode) {
            (ServiceScope::System, EnvironmentMode::Create { listen }) => {
                ModeWire::Create { listen }
            }
            (ServiceScope::System, EnvironmentMode::Join) => ModeWire::Join,
            (ServiceScope::User, EnvironmentMode::Create { listen }) => {
                ModeWire::UserCreate { listen }
            }
            (ServiceScope::User, EnvironmentMode::Join) => ModeWire::UserJoin,
        };
        Self {
            service_scope: (!service_scope.is_system()).then_some(service_scope),
            mode,
            server_url,
            project,
            runner,
            runner_display_name,
            account,
            binaries,
        }
    }
}
impl TryFrom<SetupRequestWire> for SetupRequest {
    type Error = String;
    fn try_from(wire: SetupRequestWire) -> Result<Self, String> {
        let SetupRequestWire {
            service_scope,
            mode,
            server_url,
            project,
            runner,
            runner_display_name,
            account,
            binaries,
        } = wire;
        let (encoded_scope, mode) = match mode {
            ModeWire::Create { listen } => {
                (ServiceScope::System, EnvironmentMode::Create { listen })
            }
            ModeWire::Join => (ServiceScope::System, EnvironmentMode::Join),
            ModeWire::UserCreate { listen } => {
                (ServiceScope::User, EnvironmentMode::Create { listen })
            }
            ModeWire::UserJoin => (ServiceScope::User, EnvironmentMode::Join),
        };
        if service_scope.is_some_and(|selected| selected != encoded_scope) {
            return Err(
                "Saved service scope conflicts with its versioned mode; refusing manager fallback"
                    .into(),
            );
        }
        Ok(Self {
            service_scope: encoded_scope,
            mode,
            server_url,
            project,
            runner,
            runner_display_name,
            account,
            binaries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(scope: ServiceScope, mode: EnvironmentMode) -> SetupRequest {
        SetupRequest {
            runner_display_name: None,
            service_scope: scope,
            mode,
            server_url: "http://127.0.0.1:18880".into(),
            project: None,
            runner: Some(true),
            account: LocalAccount {
                name: "owner".into(),
                identity: "1000".into(),
                home: "/home/owner".into(),
            },
            binaries: RuntimeBinaries {
                cli: "/app/webcodex".into(),
                server: "/app/server".into(),
                runner: "/app/runner".into(),
            },
        }
    }
    #[test]
    fn runner_name_round_trips_and_old_records_default_to_none() {
        let mut request = fixture(ServiceScope::User, EnvironmentMode::Join);
        let old = serde_json::to_value(&request).unwrap();
        assert!(old.get("runner_display_name").is_none());
        assert_eq!(
            serde_json::from_value::<SetupRequest>(old)
                .unwrap()
                .runner_display_name,
            None
        );
        request.runner_display_name = Some("My laptop".into());
        assert_eq!(
            serde_json::from_value::<SetupRequest>(serde_json::to_value(&request).unwrap())
                .unwrap(),
            request
        );
    }
    #[test]
    fn user_request_cannot_be_decoded_as_a_legacy_system_mode() {
        for mode in [
            EnvironmentMode::Create {
                listen: "127.0.0.1:18880".into(),
            },
            EnvironmentMode::Join,
        ] {
            let user = fixture(ServiceScope::User, mode.clone());
            let encoded = serde_json::to_value(&user).unwrap();
            // EnvironmentMode is the exact old enum used by pre-scope binaries.
            assert!(serde_json::from_value::<EnvironmentMode>(encoded["mode"].clone()).is_err());
            assert_eq!(
                serde_json::from_value::<SetupRequest>(encoded.clone()).unwrap(),
                user
            );
            let mut wrong = encoded;
            wrong["service_scope"] = serde_json::json!("system");
            assert!(serde_json::from_value::<SetupRequest>(wrong).is_err());
            let old = fixture(ServiceScope::System, mode);
            let mut old_encoded = serde_json::to_value(&old).unwrap();
            assert!(old_encoded.get("service_scope").is_none());
            assert!(serde_json::from_value::<EnvironmentMode>(old_encoded["mode"].clone()).is_ok());
            assert_eq!(
                serde_json::from_value::<SetupRequest>(old_encoded.clone()).unwrap(),
                old
            );
            old_encoded["service_scope"] = serde_json::json!("user");
            assert!(serde_json::from_value::<SetupRequest>(old_encoded).is_err());
        }
    }
}
