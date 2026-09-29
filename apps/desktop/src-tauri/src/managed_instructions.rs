//! Desktop-owned instructions only. No caller-supplied file path, no discovery
//! rule, and no implicit activation in Runner configuration.
use crate::error::{DesktopError, DesktopResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod io;
#[cfg(test)]
mod tests;

pub(crate) const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub path: PathBuf,
    pub exists: bool,
    pub content: String,
    pub revision: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_revision: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnableRequest {
    pub target: crate::webcodex::settings::SettingsTarget,
    pub expected: crate::webcodex::settings::RunnerPaths,
    pub expected_revision: String,
}

pub struct ManagedInstructions {
    root: PathBuf,
    gate: Mutex<()>,
}

impl ManagedInstructions {
    pub fn new(effective_desktop_root: PathBuf) -> Self {
        Self {
            root: effective_desktop_root,
            gate: Mutex::new(()),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.root.join("instructions").join("AGENTS.md")
    }

    pub fn read(&self) -> DesktopResult<Snapshot> {
        let _guard = self.gate.lock().map_err(|_| unavailable())?;
        self.read_locked()
    }

    fn read_locked(&self) -> DesktopResult<Snapshot> {
        let content = match io::Directory::open(&self.root, false)? {
            Some(directory) => directory.read()?,
            None => None,
        };
        Ok(snapshot(self.path(), content))
    }

    pub fn save(&self, request: SaveRequest) -> DesktopResult<Snapshot> {
        let _guard = self.gate.lock().map_err(|_| unavailable())?;
        if request.content.len() > MAX_BYTES {
            return Err(DesktopError::new(
                "managed_instructions_too_large",
                "Global instructions exceed 1 MiB",
                "Shorten the instructions before saving. Your draft has not been written.",
            ));
        }
        let current = self.read_locked()?;
        if current.revision != request.expected_revision {
            return Err(conflict());
        }
        let directory = io::Directory::open(&self.root, true)?.ok_or_else(unavailable)?;
        directory.replace(&request.content, &request.expected_revision)?;
        Ok(snapshot(self.path(), Some(request.content)))
    }

    /// Explicit enable creates an empty file only when the observed file was
    /// missing; existing content is never replaced. Configuration is separate.
    pub fn ensure(&self, expected_revision: &str) -> DesktopResult<Snapshot> {
        let observed = self.read()?;
        if observed.revision != expected_revision {
            return Err(conflict());
        }
        if observed.exists {
            return Ok(observed);
        }
        self.save(SaveRequest {
            expected_revision: expected_revision.into(),
            content: String::new(),
        })
    }
}

fn snapshot(path: PathBuf, content: Option<String>) -> Snapshot {
    let revision = revision(content.as_deref());
    Snapshot {
        path,
        exists: content.is_some(),
        content: content.unwrap_or_default(),
        revision,
    }
}
fn revision(content: Option<&str>) -> String {
    match content {
        None => "missing".into(),
        Some(content) => format!("sha256:{:x}", Sha256::digest(content.as_bytes())),
    }
}
fn conflict() -> DesktopError {
    DesktopError::new(
        "managed_instructions_conflict",
        "Global instructions changed outside this editor",
        "Keep your draft and reload the file before merging and saving again.",
    )
}
fn unavailable() -> DesktopError {
    DesktopError::new("managed_instructions_unavailable", "Managed global instructions could not be accessed safely", "Check the Desktop data directory. The managed directory and file must not be links, reparse points, or special files; the file must be UTF-8 and at most 1 MiB.")
}
fn invalid_io(_: std::io::Error) -> DesktopError {
    unavailable()
}
fn safe_metadata(metadata: &std::fs::Metadata, directory: bool) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink()
        && if directory {
            metadata.is_dir()
        } else {
            metadata.is_file() && metadata.len() <= MAX_BYTES as u64
        }
}
fn decode(mut file: std::fs::File) -> DesktopResult<String> {
    use std::io::Read;
    if !safe_metadata(&file.metadata().map_err(invalid_io)?, false) {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(invalid_io)?;
    if bytes.len() > MAX_BYTES {
        return Err(unavailable());
    }
    String::from_utf8(bytes).map_err(|_| unavailable())
}
