//! Validation identity from normalized execution semantics, not raw bodies.
use super::*;

pub(super) fn normalized_exact_git_commit_for_audit(value: &str) -> Option<String> {
    (value.len() == 40 && value.as_bytes().iter().all(u8::is_ascii_hexdigit))
        .then(|| value.to_ascii_lowercase())
}

pub(super) fn insert_structured_validation_target(
    tool_name: &str,
    arguments: &serde_json::Map<String, Value>,
    out: &mut serde_json::Map<String, Value>,
) {
    let identity_kind = webcodex_tool_contracts::runtime_tool_session_evidence_policy(tool_name)
        .validation_identity;
    if let Some(identity) =
        structured_validation_target_identity(identity_kind, &Value::Object(arguments.clone()))
    {
        out.insert("validation_target_id".to_string(), Value::String(identity));
    }
}

use webcodex_core::validation_identity::{
    GENERIC_VALIDATION_IDENTITY_PREFIX, VALIDATION_IDENTITY_HEX_LEN,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericValidationIdentity {
    pub identity: String,
    pub validation_tool: Option<&'static str>,
}

pub(super) fn validation_like_purpose(purpose: Option<&str>) -> bool {
    purpose.is_some_and(is_validation_like_execution_purpose)
}

pub(super) fn generic_validation_digest<'a>(
    source: &str,
    purpose: &str,
    cwd: Option<&str>,
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-generic-validation-v1\0");
    hasher.update(source.as_bytes());
    hasher.update(b"\0");
    hasher.update(purpose.as_bytes());
    hasher.update(b"\0");
    hasher.update(cwd.unwrap_or(".").as_bytes());
    for part in parts {
        hasher.update(b"\0");
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "{GENERIC_VALIDATION_IDENTITY_PREFIX}{}",
        &digest[..VALIDATION_IDENTITY_HEX_LEN]
    )
}

pub(super) fn canonical_cargo_validation_target(
    argv: &[String],
    cwd: Option<&str>,
) -> Option<(&'static str, String)> {
    let (subcommand, rest) = argv.split_first()?;
    let mut input = serde_json::Map::new();
    input.insert(
        "cwd".to_string(),
        Value::String(cwd.unwrap_or(".").to_string()),
    );
    let tool = match subcommand.as_str() {
        "fmt" => {
            let check = match rest {
                [] => false,
                [separator, check] if separator == "--" && check == "--check" => true,
                _ => return None,
            };
            input.insert("check".to_string(), Value::Bool(check));
            "cargo_fmt"
        }
        "check" | "test" => {
            let is_test = subcommand == "test";
            let mut packages = Vec::new();
            let mut features: Option<String> = None;
            let mut filter: Option<String> = None;
            let mut all_targets = false;
            let mut all_features = false;
            let mut no_default_features = false;
            let mut no_run = false;
            let mut lib = false;
            let mut index = 0;
            while index < rest.len() {
                let arg = &rest[index];
                match arg.as_str() {
                    "-p" | "--package" | "--features" => {
                        let value = rest.get(index + 1)?.clone();
                        if arg == "--features" {
                            if features.replace(value).is_some() {
                                return None;
                            }
                        } else {
                            if is_test && !packages.is_empty() {
                                return None;
                            }
                            packages.push(value);
                        }
                        index += 2;
                        continue;
                    }
                    "--all-targets" if !all_targets => all_targets = true,
                    "--lib" if is_test && !lib => lib = true,
                    "--all-features" if !all_features => all_features = true,
                    "--no-default-features" if !no_default_features => no_default_features = true,
                    "--no-run" if is_test && !no_run => no_run = true,
                    _ if arg.starts_with("--package=") && (!is_test || packages.is_empty()) => {
                        packages.push(arg.trim_start_matches("--package=").to_string());
                    }
                    _ if arg.starts_with("--features=") && features.is_none() => {
                        features = Some(arg.trim_start_matches("--features=").to_string());
                    }
                    _ if is_test && !arg.starts_with('-') && filter.is_none() => {
                        filter = Some(arg.to_string());
                    }
                    _ => return None,
                }
                index += 1;
            }
            let packages = normalize_cargo_packages(
                None,
                (!packages.is_empty()).then_some(packages.as_slice()),
            )
            .ok()?;
            let features = match features {
                Some(value) => normalize_cargo_value(&value).ok()?,
                None => None,
            };
            if is_test {
                input.insert(
                    "package".to_string(),
                    serde_json::json!(packages.and_then(|mut values| values.pop())),
                );
            } else {
                input.insert("packages".to_string(), serde_json::json!(packages));
            }
            input.insert("features".to_string(), serde_json::json!(features));
            input.insert("all_targets".to_string(), Value::Bool(all_targets));
            input.insert("all_features".to_string(), Value::Bool(all_features));
            input.insert(
                "no_default_features".to_string(),
                Value::Bool(no_default_features),
            );
            if is_test {
                let filter = match filter {
                    Some(value) => normalize_rust_test_filter(&value).ok()?,
                    None => None,
                };
                input.insert("filter".to_string(), serde_json::json!(filter));
                if lib {
                    input.insert("lib".to_string(), Value::Bool(true));
                }
                input.insert("no_run".to_string(), Value::Bool(no_run));
                "cargo_test"
            } else {
                "cargo_check"
            }
        }
        _ => return None,
    };
    let identity_kind =
        webcodex_tool_contracts::runtime_tool_session_evidence_policy(tool).validation_identity;
    let identity = structured_validation_target_identity(identity_kind, &Value::Object(input))?;
    Some((tool, identity))
}

pub fn run_process_validation_identity(
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&str>,
    purpose: Option<&str>,
) -> Option<GenericValidationIdentity> {
    if !validation_like_purpose(purpose) {
        return None;
    }
    if executable == "cargo" && stdin.is_none() {
        if let Some((validation_tool, identity)) = canonical_cargo_validation_target(args, cwd) {
            return Some(GenericValidationIdentity {
                identity,
                validation_tool: Some(validation_tool),
            });
        }
    }
    let purpose = purpose?;
    let mut parts = Vec::with_capacity(args.len() + 2);
    parts.push(executable);
    parts.extend(args.iter().map(String::as_str));
    if let Some(stdin) = stdin {
        parts.push(stdin);
    }
    Some(GenericValidationIdentity {
        identity: generic_validation_digest("run_process", purpose, cwd, parts),
        validation_tool: None,
    })
}

pub(super) fn simple_script_argv(script: &str) -> Option<Vec<String>> {
    let trimmed = script.trim();
    if trimmed.is_empty()
        || trimmed.lines().count() != 1
        || trimmed.chars().any(|character| {
            matches!(
                character,
                ';' | '|' | '&' | '$' | '`' | '\\' | '\'' | '"' | '<' | '>' | '(' | ')' | '{' | '}'
            )
        })
    {
        return None;
    }
    let argv = trimmed
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!argv.is_empty()).then_some(argv)
}

pub fn run_script_validation_identity(
    language: &str,
    script: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&str>,
    purpose: Option<&str>,
) -> Option<GenericValidationIdentity> {
    if !validation_like_purpose(purpose) {
        return None;
    }
    if matches!(language, "sh" | "bash") && args.is_empty() && stdin.is_none() {
        if let Some(argv) = simple_script_argv(script) {
            if argv.first().is_some_and(|program| program == "cargo") {
                if let Some((validation_tool, identity)) =
                    canonical_cargo_validation_target(&argv[1..], cwd)
                {
                    return Some(GenericValidationIdentity {
                        identity,
                        validation_tool: Some(validation_tool),
                    });
                }
            }
        }
    }
    let purpose = purpose?;
    let mut parts = Vec::with_capacity(args.len() + 3);
    parts.push(language);
    parts.push(script);
    parts.extend(args.iter().map(String::as_str));
    if let Some(stdin) = stdin {
        parts.push(stdin);
    }
    Some(GenericValidationIdentity {
        identity: generic_validation_digest("run_script", purpose, cwd, parts),
        validation_tool: None,
    })
}
