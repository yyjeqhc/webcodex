//! Pure bounded Computer input validation and normalization.

use super::*;
pub(super) fn effective_snapshot_dimension_bound(value: Option<u32>) -> Result<Option<u32>, ()> {
    match value {
        None => Ok(None),
        Some(0) => Err(()),
        Some(value) => Ok(Some(value.min(MAX_IMAGE_DIMENSION as u32))),
    }
}

pub(super) fn effective_snapshot_dimension_bounds(
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<(Option<u32>, Option<u32>), ()> {
    Ok((
        effective_snapshot_dimension_bound(max_width)?,
        effective_snapshot_dimension_bound(max_height)?,
    ))
}

pub(super) fn normalize_computer_key_input(
    key: &str,
    modifiers: Option<Vec<String>>,
) -> Result<Vec<String>, &'static str> {
    if !COMPUTER_KEY_INPUT_KEYS.contains(&key) {
        return Err("computer key input key is outside the closed vocabulary");
    }
    let mut modifiers = modifiers.unwrap_or_default();
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
    modifiers.sort_by_key(|modifier| {
        COMPUTER_KEY_INPUT_MODIFIERS
            .iter()
            .position(|allowed| *allowed == modifier)
            .unwrap_or(COMPUTER_KEY_INPUT_MODIFIERS.len())
    });
    Ok(modifiers)
}

pub(super) fn validate_clipboard_write_text(text: &str) -> Result<usize, &'static str> {
    if text.is_empty() {
        return Err("clipboard text must not be empty");
    }
    if text.contains('\0') {
        return Err("clipboard text must not contain NUL");
    }
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err("clipboard text exceeds the 16 KiB UTF-8 bound");
    }
    Ok(text.len())
}

pub(super) fn valid_application_id(application_id: &str) -> bool {
    let Some(suffix) = application_id.strip_prefix("application_") else {
        return false;
    };
    application_id.len() <= MAX_APPLICATION_ID_BYTES
        && webcodex_core::compact::decode::<12>(suffix).is_some()
}

pub(super) fn valid_display_id(display_id: &str) -> bool {
    let Some(suffix) = display_id.strip_prefix("display_") else {
        return false;
    };
    display_id.len() <= MAX_DISPLAY_ID_BYTES
        && webcodex_core::compact::decode::<12>(suffix).is_some()
}

pub(super) fn validate_input_text(text: &str) -> Result<usize, &'static str> {
    let text_bytes = text.len();
    if text_bytes == 0 || text_bytes > MAX_INPUT_TEXT_BYTES || text.contains('\0') {
        return Err(
            "computer text input must be non-empty, NUL-free, and within the UTF-8 byte limit",
        );
    }
    Ok(text_bytes)
}
