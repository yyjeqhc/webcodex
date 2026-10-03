//! One-directory live pagination. Offsets are not immutable snapshot cursors.
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_ENTRIES: usize = 500;
pub const MAX_PAGE_BYTES: usize = 48 * 1024;
pub const MAX_LISTING_MEMORY: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectoryPageRequest {
    pub offset: usize,
    pub limit: usize,
}
impl DirectoryPageRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.limit == 0 || self.limit > MAX_PAGE_ENTRIES {
            return Err("directory page limit is out of bounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectoryEntry {
    pub name: String,
    pub directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryPage {
    pub offset: usize,
    pub total_entries: usize,
    pub entries: Vec<DirectoryEntry>,
    pub next_offset: Option<usize>,
}
impl DirectoryPage {
    /// Validate producer facts before turning leaf names into Project-relative paths.
    pub fn validate(&self, request: &DirectoryPageRequest) -> Result<(), String> {
        request.validate()?;
        let start = request.offset.min(self.total_entries);
        let end = start
            .checked_add(self.entries.len())
            .ok_or("directory page overflow")?;
        if self.offset != request.offset
            || self.entries.len() > request.limit
            || end > self.total_entries
            || self.next_offset != (end < self.total_entries).then_some(end)
            || (end < self.total_entries && self.entries.is_empty())
        {
            return Err("inconsistent directory page".into());
        }
        let mut previous: Option<&str> = None;
        for entry in &self.entries {
            let name = entry.name.as_str();
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains(['/', '\\', '\0'])
                || previous.is_some_and(|value| value >= name)
            {
                return Err("invalid directory page entry".into());
            }
            previous = Some(name);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_page_rejects_scope_drift_traversal_and_nonprogressing_pages() {
        let request = DirectoryPageRequest {
            offset: 3,
            limit: 2,
        };
        let valid = DirectoryPage {
            offset: 3,
            total_entries: 5,
            entries: vec![
                DirectoryEntry {
                    name: "a.txt".into(),
                    directory: false,
                },
                DirectoryEntry {
                    name: "b".into(),
                    directory: true,
                },
            ],
            next_offset: None,
        };
        assert!(valid.validate(&request).is_ok());
        let mut bad = valid.clone();
        bad.offset = 0;
        assert!(bad.validate(&request).is_err());
        let mut bad = valid.clone();
        bad.entries[0].name = "../secret".into();
        assert!(bad.validate(&request).is_err());
        let mut bad = valid.clone();
        bad.entries.reverse();
        assert!(bad.validate(&request).is_err());
        let mut bad = valid;
        bad.entries.clear();
        bad.next_offset = Some(3);
        assert!(bad.validate(&request).is_err());
        assert!(DirectoryPageRequest {
            offset: 0,
            limit: 0
        }
        .validate()
        .is_err());
        assert!(serde_json::from_str::<DirectoryPageRequest>(
            r#"{"offset":0,"limit":2,"ignored":1}"#
        )
        .is_err());
    }
}
