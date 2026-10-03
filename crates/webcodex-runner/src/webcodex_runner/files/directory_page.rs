use super::{CommandResult, RunnerFilePayload};
use std::path::Path;
use std::time::Instant;
use webcodex_core::directory_page::{
    DirectoryEntry, DirectoryPage, DirectoryPageRequest, MAX_LISTING_MEMORY, MAX_PAGE_BYTES,
};

/// Live offset pagination needs only the smallest offset+limit names. The
/// heap keeps first-page memory proportional to the requested page even in a
/// huge directory. Deep offsets still have an explicit memory ceiling.
fn select_entries(
    entries: impl Iterator<Item = Result<(String, bool), String>>,
    request: &DirectoryPageRequest,
) -> Result<(Vec<DirectoryEntry>, usize), String> {
    use std::collections::BinaryHeap;
    let keep = request.offset.saturating_add(request.limit);
    let mut heap: BinaryHeap<(String, bool)> = BinaryHeap::new();
    let mut memory = 0usize;
    let mut total = 0usize;
    for entry in entries {
        let entry = entry?;
        total += 1;
        if heap.len() == keep && heap.peek().is_some_and(|largest| largest <= &entry) {
            continue;
        }
        if heap.len() == keep {
            let old = heap.pop().expect("nonzero validated page capacity");
            memory -= old.0.len() + std::mem::size_of::<DirectoryEntry>();
        }
        memory += entry.0.len() + std::mem::size_of::<DirectoryEntry>();
        if memory > MAX_LISTING_MEMORY {
            return Err("directory offset exceeds listing memory bound; narrow the search".into());
        }
        heap.push(entry);
    }
    Ok((
        heap.into_sorted_vec()
            .into_iter()
            .map(|(name, directory)| DirectoryEntry { name, directory })
            .collect(),
        total,
    ))
}

fn read_page(root: &Path, request: &DirectoryPageRequest) -> Result<String, String> {
    request.validate()?;
    let entries = std::fs::read_dir(root)
        .map_err(|_| "cannot read directory")?
        .map(|entry| {
            let entry = entry.map_err(|_| "directory changed or entry is unreadable")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "directory contains a non-UTF-8 name")?;
            if name.contains('\\') {
                return Err("directory contains an unrepresentable path separator".into());
            }
            let directory = entry
                .file_type()
                .map_err(|_| "cannot inspect directory entry")?
                .is_dir();
            Ok((name, directory))
        });
    let (mut all, total_entries) = select_entries(entries, request)?;
    let start = request.offset.min(total_entries);
    let end = start.saturating_add(request.limit).min(total_entries);
    // Reserve enough for field names and usize counters, then measure each
    // bounded entry once. No quadratic serialize/pop loop for long filenames.
    let mut page_bytes = 256usize;
    let mut entries = Vec::new();
    for entry in all.drain(start..end) {
        let bytes = serde_json::to_string(&entry)
            .map_err(|_| "cannot encode directory entry")?
            .len()
            + 1;
        if page_bytes + bytes > MAX_PAGE_BYTES {
            break;
        }
        page_bytes += bytes;
        entries.push(entry);
    }
    drop(all);
    let next = start + entries.len();
    let page = DirectoryPage {
        offset: request.offset,
        total_entries,
        entries,
        next_offset: (next < total_entries).then_some(next),
    };
    page.validate(request)?;
    let encoded = serde_json::to_string(&page).map_err(|_| "cannot encode directory page")?;
    if encoded.len() > MAX_PAGE_BYTES {
        return Err("directory page exceeds byte bound".into());
    }
    Ok(encoded)
}

pub(super) fn handle(payload: &RunnerFilePayload, root: &Path, started: Instant) -> CommandResult {
    let result = payload
        .content
        .as_deref()
        .ok_or_else(|| "directory page request missing".to_owned())
        .and_then(|raw| {
            serde_json::from_str::<DirectoryPageRequest>(raw)
                .map_err(|_| "invalid directory page request".to_owned())
        })
        .and_then(|request| read_page(root, &request));
    match result {
        Ok(stdout) => CommandResult {
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: None,
            duration_ms: Some(started.elapsed().as_millis() as u64),
            error: None,
        },
        Err(error) => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(started.elapsed().as_millis() as u64),
            error: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_pages_bound_transport_without_loss_or_filename_ambiguity() {
        let tmp = tempfile::tempdir().unwrap();
        for n in 0..4000 {
            std::fs::write(
                tmp.path().join(format!("{n:05}-{}.txt", "x".repeat(80))),
                "",
            )
            .unwrap();
        }
        let mut offset = 0;
        let mut seen = Vec::new();
        let mut largest = 0;
        loop {
            let request = DirectoryPageRequest { offset, limit: 73 };
            let encoded = read_page(tmp.path(), &request).unwrap();
            largest = largest.max(encoded.len());
            let page: DirectoryPage = serde_json::from_str(&encoded).unwrap();
            page.validate(&request).unwrap();
            assert!(encoded.len() <= MAX_PAGE_BYTES);
            assert_eq!(page.total_entries, 4000);
            seen.extend(page.entries.into_iter().map(|entry| entry.name));
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        assert_eq!(seen.len(), 4000);
        seen.dedup();
        assert_eq!(seen.len(), 4000);
        eprintln!("DIRECTORY_PAGE total=4000 requested=73 largest_response_bytes={largest} full_listing_bytes_gt=340000");
    }
    #[test]
    fn small_page_does_not_materialize_a_large_directory_source() {
        let request = DirectoryPageRequest {
            offset: 0,
            limit: 73,
        };
        let rows = (0..100_000)
            .rev()
            .map(|n| Ok((format!("{n:06}-{}", "x".repeat(180)), false)));
        let (prefix, total) = select_entries(rows, &request).unwrap();
        assert_eq!(total, 100_000);
        assert_eq!(prefix.len(), 73);
        assert!(prefix[0].name.starts_with("000000-"));
        assert!(prefix[72].name.starts_with("000072-"));
        eprintln!(
            "DIRECTORY_SELECTION total=100000 retained_prefix=73 full_name_bytes_gt=18000000"
        );
    }
    #[cfg(unix)]
    #[test]
    fn directory_page_preserves_newlines_in_leaf_names() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a\nb.txt"), "").unwrap();
        let req = DirectoryPageRequest {
            offset: 0,
            limit: 10,
        };
        let page: DirectoryPage =
            serde_json::from_str(&read_page(tmp.path(), &req).unwrap()).unwrap();
        assert_eq!(page.entries[0].name, "a\nb.txt");
        page.validate(&req).unwrap();
    }
}
