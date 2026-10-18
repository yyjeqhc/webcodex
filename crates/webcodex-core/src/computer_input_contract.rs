//! Closed Computer key vocabulary shared by model schemas and runtime validation.

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

/// Canonical modifier order; platform adapters may reject unsupported chords.
pub const COMPUTER_KEY_INPUT_MODIFIERS: &[&str] = &["shift", "control", "option", "command"];

pub fn computer_key_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": COMPUTER_KEY_INPUT_KEYS,
    })
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
