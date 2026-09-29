//! Stable validation target identity parsing used across audit and Workflow Session layers.
//! These identities cover executable target parameters, NEVER workspace source state.
//! Source observations are a separate, deliberately weaker `validation_source` contract.

use crate::runner_protocol::{
    normalize_cargo_packages, normalize_cargo_value, normalize_go_test_packages,
    normalize_rust_test_filter,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const STRUCTURED_VALIDATION_TARGET_PREFIX: &str = "target:";
pub const VALIDATION_IDENTITY_HEX_LEN: usize = 24;
pub const GENERIC_VALIDATION_IDENTITY_PREFIX: &str = "command:";
const ASSERTION_VALIDATION_IDENTITY_PREFIX: &str = "assertion:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolValidationIdentityKind {
    None,
    CargoFmt,
    CargoCheck,
    CargoTest,
    GoTest,
    GoVet,
    Project,
}

impl ToolValidationIdentityKind {
    pub const fn tool_name(self) -> Option<&'static str> {
        match self {
            Self::None | Self::Project => None,
            Self::CargoFmt => Some("cargo_fmt"),
            Self::CargoCheck => Some("cargo_check"),
            Self::CargoTest => Some("cargo_test"),
            Self::GoTest => Some("go_test"),
            Self::GoVet => Some("go_vet"),
        }
    }
}

pub fn is_structured_validation_target_identity(value: &str) -> bool {
    let Some(hex) = value.strip_prefix(STRUCTURED_VALIDATION_TARGET_PREFIX) else {
        return false;
    };
    hex.len() == VALIDATION_IDENTITY_HEX_LEN && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn is_validation_execution_identity(value: &str) -> bool {
    if is_structured_validation_target_identity(value) {
        return true;
    }
    [
        GENERIC_VALIDATION_IDENTITY_PREFIX,
        ASSERTION_VALIDATION_IDENTITY_PREFIX,
    ]
    .into_iter()
    .any(|prefix| {
        value.strip_prefix(prefix).is_some_and(|suffix| {
            suffix.len() == VALIDATION_IDENTITY_HEX_LEN
                && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

pub fn assertion_validation_identity(assertion_name: &str) -> String {
    let assertion_name = assertion_name.trim();
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-validation-assertion-v1\0");
    hasher.update((assertion_name.len() as u64).to_le_bytes());
    hasher.update(assertion_name.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "{ASSERTION_VALIDATION_IDENTITY_PREFIX}{}",
        &digest[..VALIDATION_IDENTITY_HEX_LEN]
    )
}

fn canonicalize_json_value(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(canonicalize_json_value).collect())
        }
        Value::Object(object) => {
            let mut entries = object.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize_json_value(value));
            }
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

pub fn structured_validation_target_identity(
    kind: ToolValidationIdentityKind,
    arguments: &Value,
) -> Option<String> {
    let tool_name = kind.tool_name()?;
    let obj = arguments.as_object()?;
    let cwd = normalized_validation_target_cwd(obj.get("cwd"))?;
    let semantic = match kind {
        ToolValidationIdentityKind::CargoFmt => serde_json::json!({
            "tool": tool_name,
            "kind": "format",
            "cwd": cwd,
            "check": obj.get("check").and_then(Value::as_bool).unwrap_or(false),
        }),
        ToolValidationIdentityKind::CargoCheck => {
            if obj.get("features_present").and_then(Value::as_bool) == Some(true)
                && obj.get("features").is_none()
            {
                return None;
            }
            let features = normalized_cargo_target_value(obj.get("features"))?;
            let packages =
                normalized_cargo_target_packages(obj.get("package"), obj.get("packages"))?;
            let mut semantic = serde_json::json!({
                "tool": tool_name,
                "kind": "check",
                "cwd": cwd,
                "features": features,
                "all_targets": obj.get("all_targets").and_then(Value::as_bool).unwrap_or(true),
                "all_features": obj.get("all_features").and_then(Value::as_bool).unwrap_or(false),
                "no_default_features": obj.get("no_default_features").and_then(Value::as_bool).unwrap_or(false),
            });
            // Preserve the established identity encoding for omitted and
            // single-package requests. Multi-package scopes use the additive
            // canonical array shape.
            if packages.as_ref().is_some_and(|packages| packages.len() > 1) {
                semantic["packages"] = serde_json::json!(packages);
            } else {
                semantic["package"] =
                    serde_json::json!(packages.and_then(|mut values| values.pop()));
            }
            semantic
        }
        ToolValidationIdentityKind::CargoTest => {
            if obj.get("filter_present").and_then(Value::as_bool) == Some(true)
                && obj.get("filter").is_none()
            {
                return None;
            }
            if obj.get("features_present").and_then(Value::as_bool) == Some(true)
                && obj.get("features").is_none()
            {
                return None;
            }
            let filter = normalized_rust_test_target_filter(obj.get("filter"))?;
            let features = normalized_cargo_target_value(obj.get("features"))?;
            let packages =
                normalized_cargo_target_packages(obj.get("package"), obj.get("packages"))?;
            let mut semantic = serde_json::json!({
                "tool": tool_name,
                "kind": "test",
                "cwd": cwd,
                "filter": filter,
                "features": features,
                "all_targets": obj.get("all_targets").and_then(Value::as_bool).unwrap_or(false),
                "all_features": obj.get("all_features").and_then(Value::as_bool).unwrap_or(false),
                "no_default_features": obj.get("no_default_features").and_then(Value::as_bool).unwrap_or(false),
                "no_run": obj.get("no_run").and_then(Value::as_bool).unwrap_or(false),
            });
            if packages.as_ref().is_some_and(|packages| packages.len() > 1) {
                semantic["packages"] = serde_json::json!(packages);
            } else {
                semantic["package"] =
                    serde_json::json!(packages.and_then(|mut values| values.pop()));
            }
            if obj.get("lib").and_then(Value::as_bool) == Some(true) {
                semantic["lib"] = Value::Bool(true);
            }
            semantic
        }
        ToolValidationIdentityKind::GoTest => {
            if obj.get("packages_present").and_then(Value::as_bool) == Some(true)
                && obj.get("packages").is_none()
            {
                return None;
            }
            let packages = normalized_go_test_target_packages(obj.get("packages"))?;
            if obj.get("filter_present").and_then(Value::as_bool) == Some(true)
                && obj.get("filter").is_none()
            {
                return None;
            }
            let filter = match obj.get("filter") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    crate::runner_protocol::normalize_go_test_filter(value.as_str()?).ok()?
                }
            };
            let mut semantic = serde_json::json!({
                "tool": tool_name,
                "kind": "test",
                "cwd": cwd,
                "packages": packages,
            });
            // Preserve all unfiltered identities exactly, including omitted or
            // empty filters. Count postconditions never change executable target.
            if let Some(filter) = filter {
                semantic["filter"] = Value::String(filter);
            }
            semantic
        }
        ToolValidationIdentityKind::GoVet => {
            if obj.get("packages_present").and_then(Value::as_bool) == Some(true)
                && obj.get("packages").is_none()
            {
                return None;
            }
            let packages = normalized_go_test_target_packages(obj.get("packages"))?;
            // Preserve the project_validate v1 default Go vet target identity
            // byte-for-byte. Explicit "./..." is semantically the same scope.
            if packages.as_slice() == ["./..."] {
                let digest = format!("{:x}", Sha256::digest(format!("go_vet\0{cwd}")));
                return Some(format!(
                    "{STRUCTURED_VALIDATION_TARGET_PREFIX}{}",
                    &digest[..VALIDATION_IDENTITY_HEX_LEN]
                ));
            }
            serde_json::json!({
                "tool": tool_name,
                "kind": "check",
                "cwd": cwd,
                "packages": packages,
            })
        }
        ToolValidationIdentityKind::None | ToolValidationIdentityKind::Project => return None,
    };
    // `serde_json::Map` switches from sorted-map semantics to insertion-order
    // semantics when any workspace dependency enables `preserve_order`.
    // Validation identities are durable evidence keys, so Cargo feature
    // unification must not change them. Canonicalize object-key order before
    // hashing while preserving the existing sorted-key identity contract.
    let encoded = serde_json::to_vec(&canonicalize_json_value(semantic)).ok()?;
    let digest = format!("{:x}", Sha256::digest(encoded));
    Some(format!(
        "{STRUCTURED_VALIDATION_TARGET_PREFIX}{}",
        &digest[..VALIDATION_IDENTITY_HEX_LEN]
    ))
}

fn normalized_validation_target_cwd(value: Option<&Value>) -> Option<String> {
    let Some(value) = value else {
        return Some(".".to_string());
    };
    if value.is_null() {
        return Some(".".to_string());
    }
    let raw = value.as_str()?;
    let trimmed = raw.trim().trim_start_matches("./").trim_end_matches('/');
    Some(if trimmed.is_empty() || trimmed == "." {
        ".".to_string()
    } else {
        trimmed.to_string()
    })
}

fn normalized_cargo_target_value(value: Option<&Value>) -> Option<Option<String>> {
    let Some(value) = value else {
        return Some(None);
    };
    if value.is_null() {
        return Some(None);
    }
    normalize_cargo_value(value.as_str()?).ok()
}

fn normalized_cargo_target_packages(
    package: Option<&Value>,
    packages: Option<&Value>,
) -> Option<Option<Vec<String>>> {
    let package = match package {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.as_str()),
        Some(_) => return None,
    };
    let packages = match packages {
        None | Some(Value::Null) => None,
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ),
        Some(_) => return None,
    };
    normalize_cargo_packages(package, packages.as_deref()).ok()
}

fn normalized_rust_test_target_filter(value: Option<&Value>) -> Option<Option<String>> {
    let Some(value) = value else {
        return Some(None);
    };
    if value.is_null() {
        return Some(None);
    }
    normalize_rust_test_filter(value.as_str()?).ok()
}

fn normalized_go_test_target_packages(value: Option<&Value>) -> Option<Vec<String>> {
    let packages = match value {
        None | Some(Value::Null) => None,
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ),
        Some(_) => return None,
    };
    normalize_go_test_packages(packages.as_deref()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_validation_target_identity_hashes_are_stable() {
        let cases = [
            (
                ToolValidationIdentityKind::CargoFmt,
                serde_json::json!({"cwd": ".", "check": true}),
                "target:dfd175fca2d6e3c744338849",
            ),
            (
                ToolValidationIdentityKind::CargoCheck,
                serde_json::json!({
                    "cwd": ".",
                    "package": "webcodex",
                    "features": "serde",
                    "all_targets": true,
                    "all_features": false,
                    "no_default_features": false
                }),
                "target:1db935c208ff30ab1716b2ab",
            ),
            (
                ToolValidationIdentityKind::CargoTest,
                serde_json::json!({
                    "cwd": ".",
                    "package": "webcodex",
                    "filter": "focused",
                    "features": "serde",
                    "all_targets": false,
                    "all_features": false,
                    "no_default_features": false,
                    "no_run": false
                }),
                "target:fde1a5cfa80b68da00bb489f",
            ),
            (
                ToolValidationIdentityKind::GoTest,
                serde_json::json!({"cwd": ".", "packages": ["./..."]}),
                "target:53578a0709b0ce549e125eb7",
            ),
            (
                ToolValidationIdentityKind::GoVet,
                serde_json::json!({"cwd": "."}),
                "target:6d00fe00bc63c0cd5baaff2d",
            ),
        ];
        for (kind, arguments, expected) in cases {
            assert_eq!(
                structured_validation_target_identity(kind, &arguments).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            structured_validation_target_identity(
                ToolValidationIdentityKind::None,
                &serde_json::json!({})
            ),
            None
        );
    }

    #[test]
    fn cargo_test_multi_package_identity_preserves_single_package_compatibility() {
        let kind = ToolValidationIdentityKind::CargoTest;
        let multi = structured_validation_target_identity(
            kind,
            &serde_json::json!({"packages": ["package-b", "package-a", "package-b"]}),
        )
        .unwrap();
        let distinct = structured_validation_target_identity(
            kind,
            &serde_json::json!({"packages": ["package-a", "package-c"]}),
        )
        .unwrap();
        assert_ne!(multi, distinct);
        assert_eq!(
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"packages": ["package-a"]}),
            ),
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"package": "package-a"}),
            )
        );
    }

    #[test]
    fn go_vet_default_identity_is_legacy_stable_and_scoped_identity_is_distinct() {
        let kind = ToolValidationIdentityKind::GoVet;
        let legacy = structured_validation_target_identity(kind, &serde_json::json!({"cwd": "."}));
        assert_eq!(legacy.as_deref(), Some("target:6d00fe00bc63c0cd5baaff2d"));
        assert_eq!(
            legacy,
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"cwd": ".", "packages": ["./..."]}),
            )
        );
        assert_ne!(
            legacy,
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"cwd": ".", "packages": ["./cmd/...", "./internal"]}),
            )
        );
    }

    #[test]
    fn canonical_validation_identity_json_is_feature_order_independent() {
        let mut inner = serde_json::Map::new();
        inner.insert("d".to_string(), serde_json::json!(4));
        inner.insert("c".to_string(), serde_json::json!(3));
        let mut outer = serde_json::Map::new();
        outer.insert("z".to_string(), serde_json::json!(1));
        outer.insert("a".to_string(), Value::Object(inner));

        let canonical = canonicalize_json_value(Value::Object(outer));
        assert_eq!(
            serde_json::to_string(&canonical).unwrap(),
            r#"{"a":{"c":3,"d":4},"z":1}"#
        );
    }

    #[test]
    fn validation_identity_shapes_are_stable() {
        let assertion = assertion_validation_identity("cargo check");
        assert!(assertion.starts_with("assertion:"));
        assert!(is_validation_execution_identity(&assertion));
        assert!(is_structured_validation_target_identity(
            "target:0123456789abcdef01234567"
        ));
        assert!(!is_validation_execution_identity("target:short"));
    }

    #[test]
    fn cargo_check_multi_package_identity_is_order_and_duplicate_independent() {
        let kind = ToolValidationIdentityKind::CargoCheck;
        let canonical = structured_validation_target_identity(
            kind,
            &serde_json::json!({"packages": ["package-a", "package-b"]}),
        )
        .unwrap();
        let reordered = structured_validation_target_identity(
            kind,
            &serde_json::json!({"packages": [" package-b ", "package-a", "package-b"]}),
        )
        .unwrap();
        let distinct = structured_validation_target_identity(
            kind,
            &serde_json::json!({"packages": ["package-a", "package-c"]}),
        )
        .unwrap();

        assert_eq!(canonical, reordered);
        assert_ne!(canonical, distinct);
        assert_eq!(
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"package": "package-a"}),
            ),
            structured_validation_target_identity(
                kind,
                &serde_json::json!({"packages": ["package-a"]}),
            )
        );
    }
}
