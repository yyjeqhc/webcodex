//! Closed Computer key/chord contract shared by schemas, admission and native input.

use crate::runner_protocol::RunnerCapabilityId;
use serde::Deserialize;

/// Navigation keys supported by the original key-input capability.

pub const COMPUTER_KEY_INPUT_KEYS: &[&str] = &[
    "enter",
    "escape",
    "tab",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "page_up",
    "page_down",
    "home",
    "end",
];

/// Windows virtual-key shortcuts. Each requires exactly the Control modifier.
pub const COMPUTER_LETTER_SHORTCUT_KEYS: &[&str] = &["j", "p"];

/// Canonical modifier order; platform adapters may reject unsupported chords.
pub const COMPUTER_KEY_INPUT_MODIFIERS: &[&str] = &["shift", "control", "option", "command"];

pub fn is_computer_letter_shortcut(key: &str) -> bool {
    COMPUTER_LETTER_SHORTCUT_KEYS.contains(&key)
}

pub fn validate_computer_key_input(key: &str, modifiers: &[String]) -> Result<(), &'static str> {
    if !COMPUTER_KEY_INPUT_KEYS.contains(&key) && !is_computer_letter_shortcut(key) {
        return Err("computer key input key is outside the closed vocabulary");
    }
    validate_computer_key_modifiers(modifiers)?;
    if is_computer_letter_shortcut(key) && (modifiers.len() != 1 || modifiers[0] != "control") {
        return Err("computer letter shortcuts require exactly modifiers=[control]");
    }
    Ok(())
}

pub fn validate_computer_key_modifiers(modifiers: &[String]) -> Result<(), &'static str> {
    if modifiers.len() > COMPUTER_KEY_INPUT_MODIFIERS.len() {
        return Err("computer key input has too many modifiers");
    }
    for (index, modifier) in modifiers.iter().enumerate() {
        if !COMPUTER_KEY_INPUT_MODIFIERS.contains(&modifier.as_str())
            || modifiers[..index].contains(modifier)
        {
            return Err("computer key input modifiers are invalid or duplicated");
        }
    }
    Ok(())
}

pub fn computer_key_input_capabilities(key: &str) -> &'static [RunnerCapabilityId] {
    if is_computer_letter_shortcut(key) {
        &[
            RunnerCapabilityId::ComputerKeyInput,
            RunnerCapabilityId::ComputerLetterShortcuts,
        ]
    } else {
        &[RunnerCapabilityId::ComputerKeyInput]
    }
}

/// Canonical bounded Runner payload; modifiers are always explicit on the wire.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerKeyInputPayload {
    pub surface_id: String,
    pub key: String,
    pub modifiers: Vec<String>,
}

impl ComputerKeyInputPayload {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_computer_key_input(&self.key, &self.modifiers)
    }
}

pub fn computer_key_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let keys: Vec<_> = COMPUTER_KEY_INPUT_KEYS
        .iter()
        .chain(COMPUTER_LETTER_SHORTCUT_KEYS)
        .collect();
    schemars::json_schema!({
        "type": "string",
        "enum": keys,
    })
}

/// Applied to the key action object so schema validation checks the complete chord.
pub fn computer_key_chord_constraints() -> serde_json::Value {
    serde_json::json!([{
        "anyOf": [
            {"properties": {"key": {"enum": COMPUTER_KEY_INPUT_KEYS}}},
            {
                "required": ["modifiers"],
                "properties": {
                    "key": {"enum": COMPUTER_LETTER_SHORTCUT_KEYS},
                    "modifiers": {"const": ["control"]}
                }
            }
        ]
    }])
}

/// Preserve explicit null through the OpenAPI input-schema normalization path.
pub fn computer_key_modifiers_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "anyOf": [
            {
                "type": "array",
                "maxItems": COMPUTER_KEY_INPUT_MODIFIERS.len(),
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": COMPUTER_KEY_INPUT_MODIFIERS,
                },
            },
            {"type": "null"}
        ]
    })
}
