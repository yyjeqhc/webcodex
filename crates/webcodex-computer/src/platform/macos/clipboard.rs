use super::*;
#[cfg(target_os = "macos")]
pub(crate) fn read_clipboard() -> Result<Value, String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };
    let Some(native_text) = pasteboard.stringForType(string_type) else {
        return clipboard_read_result("macos", None);
    };
    if native_text.len() > crate::MAX_CLIPBOARD_TEXT_BYTES {
        return Err(
            "clipboard_too_large: clipboard UTF-8 text exceeds the 16 KiB bound".to_string(),
        );
    }
    let text = native_text.to_string();
    clipboard_read_result("macos", Some(&text))
}

#[cfg(target_os = "macos")]
pub(crate) fn write_clipboard(text: &str) -> Result<Value, String> {
    // Complete caller validation and native string/object construction before
    // clearContents crosses the pasteboard mutation boundary.
    let prepared = prepare_clipboard_write_text(text)?;
    let native_text = NSString::from_str(text);
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };

    let effect = run_macos_clipboard_write_effect_steps(
        || pasteboard.clearContents(),
        || pasteboard.setString_forType(&native_text, string_type),
        || pasteboard.changeCount(),
    );
    match effect {
        ClipboardWriteEffectState::Success => Ok(json!({
            "platform": "macos",
            "text_bytes": prepared.text_bytes,
            "success": true,
        })),
        ClipboardWriteEffectState::OutcomeUnknown => Err(
            "outcome_unknown: macOS pasteboard changed after clearContents but the complete NSPasteboardTypeString replacement could not be proven"
                .to_string(),
        ),
        #[cfg(test)]
        ClipboardWriteEffectState::NotStarted => {
            unreachable!("macOS clearContents has no definite native failure result")
        }
    }
}
