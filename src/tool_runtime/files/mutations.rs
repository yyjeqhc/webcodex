//! File mutation entry points share exact preflight and delivery boundaries.
//! Operation-specific admission stays separate from untrusted receipt validation.

use super::*;

pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum serialized batch payload sent to the owning Runner. Host-only (the
/// agent enforces a per-file cap instead), so it stays local.
pub(crate) const MAX_APPLY_FILE_CHANGES_BYTES: usize = 1024 * 1024;
mod delete;
mod lifecycle;
mod patch;
mod patch_result;
mod preflight;
mod text_edits;
mod text_edits_result;
mod write;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use preflight::validate_edit_file_path;
#[cfg(test)]
pub(crate) use tests::apply_text_edits_to_string;
