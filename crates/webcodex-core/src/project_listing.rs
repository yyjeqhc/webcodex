//! Pure listing logic for `list_project_tracked_files`.
//!
//! The agent runs one deterministic `git ls-files -z` and returns raw bytes.
//! Everything that decides what the model sees — scope, glob filter, directory
//! rollup, pagination — happens here, so it is unit-testable without an agent.
//!
//! Rollup is what keeps this usable on a large repository. A flat list of
//! 50,000 tracked paths is not a project structure, it is a wall. When the
//! flat list does not fit the caller's limit, the deepest directory depth that
//! *does* fit is chosen automatically and reported back, so one call always
//! returns a complete picture at some resolution instead of an arbitrary
//! prefix of an incomplete one.
//!
//! That guarantee depends on complete source acquisition. Callers that hit a
//! producer/source bound must expose `list_truncated=true` and must not present
//! this ordinary offset pagination as a way to traverse the full repository.

use serde_json::{json, Value};

/// Deepest rollup depth the automatic search will consider. Beyond this, extra
/// depth stops buying legibility and only costs entries.
const MAX_AUTO_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dir => "dir",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingEntry {
    pub path: String,
    pub kind: EntryKind,
    /// Number of tracked files beneath a rolled-up directory. `None` for files.
    pub file_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<ListingEntry>,
    /// Files matching scope + globs, before rollup and pagination.
    pub total_files: usize,
    /// Entries after rollup, before pagination.
    pub total_entries: usize,
    /// Effective rollup depth; `None` means every file is listed individually.
    pub depth: Option<usize>,
    /// True when `depth` was chosen automatically rather than requested.
    pub depth_auto: bool,
    pub truncated: bool,
    pub next_offset: Option<usize>,
}

/// Split a `git ls-files -z` byte stream into paths.
///
/// Returns `(paths, truncated)`. Every complete record ends with NUL, so a
/// non-empty tail without one is an incomplete source-acquisition final path
/// and is dropped rather than reported as a real file — a half path would send the
/// model to read something that does not exist.
pub fn parse_nul_separated(raw: &str) -> (Vec<String>, bool) {
    if raw.is_empty() {
        return (Vec::new(), false);
    }
    let truncated = !raw.ends_with('\0');
    let mut paths: Vec<String> = raw
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect();
    if truncated {
        paths.pop();
    }
    (paths, truncated)
}

/// Normalize a caller-supplied scope into a `dir/`-shaped prefix.
/// `""`, `"."`, and `"./"` all mean the project root.
pub fn normalize_scope(path: Option<&str>) -> String {
    let raw = path.unwrap_or("").trim();
    let trimmed = raw.trim_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return String::new();
    }
    format!("{}/", trimmed.trim_start_matches("./"))
}

/// Match one project-relative path against a glob.
///
/// Supported: `*` (any run except `/`), `?` (one byte except `/`), `**`
/// (any run including `/`). Deliberately no character classes or braces —
/// a pattern language the model can predict beats an expressive one it cannot.
///
/// A pattern containing no `/` also matches the basename, so `*.py` works
/// without writing `**/*.py`.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    if !pattern.contains('/') {
        let basename = path.rsplit('/').next().unwrap_or(path);
        let matched = glob_match_segment(pattern.as_bytes(), basename.as_bytes());
        if matched || basename == path {
            return matched;
        }
    }
    glob_match_segment(pattern.as_bytes(), path.as_bytes())
}

/// Byte glob matcher with O(pattern length * text length) work and O(text
/// length) memory. Literal and one-star-run patterns avoid the DP allocation.
fn glob_match_segment(pattern: &[u8], text: &[u8]) -> bool {
    let Some(first_star) = pattern.iter().position(|byte| *byte == b'*') else {
        return pattern.len() == text.len()
            && pattern.iter().zip(text).all(|(&expected, &actual)| {
                expected == actual || (expected == b'?' && actual != b'/')
            });
    };
    let mut after_star = first_star + 1;
    while pattern.get(after_star) == Some(&b'*') {
        after_star += 1;
    }
    let prefix = &pattern[..first_star];
    let suffix = &pattern[after_star..];
    if !prefix.contains(&b'?') && !suffix.iter().any(|byte| matches!(*byte, b'*' | b'?')) {
        if prefix.len() + suffix.len() > text.len()
            || !text.starts_with(prefix)
            || !text.ends_with(suffix)
        {
            return false;
        }
        return after_star - first_star >= 2
            || !text[prefix.len()..text.len() - suffix.len()].contains(&b'/');
    }

    // Each cell means the consumed pattern prefix matches this text prefix.
    // Stars propagate left-to-right; literals read the previous row in reverse.
    let mut reachable = vec![false; text.len() + 1];
    reachable[0] = true;
    let mut p = 0;
    while p < pattern.len() {
        if pattern[p] == b'*' {
            let start = p;
            while pattern.get(p) == Some(&b'*') {
                p += 1;
            }
            // Consecutive stars can be empty; any pair permits crossing '/'.
            let crosses_slash = p - start >= 2;
            for t in 1..=text.len() {
                reachable[t] =
                    reachable[t] || (reachable[t - 1] && (crosses_slash || text[t - 1] != b'/'));
            }
        } else {
            for t in (1..=text.len()).rev() {
                reachable[t] = reachable[t - 1]
                    && (pattern[p] == text[t - 1] || (pattern[p] == b'?' && text[t - 1] != b'/'));
            }
            reachable[0] = false;
            p += 1;
        }
    }
    reachable[text.len()]
}

/// Number of entries a rollup at `depth` would produce for `paths`, where each
/// path is already relative to the requested scope.
fn entry_count_at_depth(relatives: &[&str], depth: usize) -> usize {
    let mut keys: Vec<&str> = relatives.iter().map(|rel| rollup_key(rel, depth)).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.len()
}

/// The entry a scope-relative path collapses into at `depth`.
///
/// A path with more than `depth` segments collapses onto its `depth`-segment
/// ancestor directory (trailing slash kept, so the caller can tell a rolled-up
/// directory from a file). A path already at or above the depth keeps its own
/// identity — `README.md` stays a file even at depth 1.
fn rollup_key(relative: &str, depth: usize) -> &str {
    // Segment count exceeds `depth` exactly when a `depth`-th separator exists.
    match relative
        .match_indices('/')
        .nth(depth.saturating_sub(1))
        .map(|(index, _)| index)
    {
        Some(index) => &relative[..=index],
        None => relative,
    }
}

/// Build the listing the model sees.
///
/// `paths` are project-relative tracked files. `scope` is a `dir/`-shaped
/// prefix (or empty for the root). When `depth` is `None` the deepest depth
/// that fits `limit` is chosen automatically.
pub fn build_listing(
    paths: &[String],
    scope: &str,
    globs: &[String],
    depth: Option<usize>,
    limit: usize,
    offset: usize,
) -> Listing {
    build_listing_with_query(paths, scope, globs, None, depth, limit, offset)
}

/// Literal path filtering precedes pagination. Query mode keeps exact file identities,
/// including paths deeper than the presentation rollup ceiling.
#[allow(clippy::too_many_arguments)]
pub fn build_listing_with_query(
    paths: &[String],
    scope: &str,
    globs: &[String],
    query: Option<&str>,
    depth: Option<usize>,
    limit: usize,
    offset: usize,
) -> Listing {
    let limit = limit.max(1);
    let mut matched: Vec<&str> = paths
        .iter()
        // Use the shared policy: sensitive paths stay hidden like they do for
        // reads, writes, and edits, while bulk trees stay hidden like they do
        // for `files_search`. A tracked `.env` is a real thing: without this
        // the listing advertises a path that `files_read` then refuses, which
        // costs a round trip and gives the model a contradictory signal.
        .filter(|path| !crate::sensitive_paths::is_bulk_skipped_path(path))
        .filter(|path| scope.is_empty() || path.starts_with(scope))
        .filter(|path| {
            globs.is_empty() || globs.iter().any(|glob| glob_matches(glob, path.as_str()))
        })
        .filter(|path| query.is_none_or(|q| path.contains(q)))
        .map(String::as_str)
        .collect();
    matched.sort_unstable();
    matched.dedup();
    let total_files = matched.len();

    // Depth is counted inside the requested scope, so `path=src, depth=1`
    // shows `src/db/` rather than collapsing everything back to `src/`.
    let relatives: Vec<&str> = matched
        .iter()
        .map(|path| &path[scope.len().min(path.len())..])
        .collect();

    let (effective_depth, depth_auto) = match (query, depth) {
        // Queries return exact files even when paging or depth was requested.
        // None already represents this mode; never serialize a usize::MAX sentinel.
        (Some(_), _) => (None, false),
        (None, Some(requested)) => (Some(requested.max(1)), false),
        (None, None) if total_files <= limit => (None, false),
        (None, None) => (Some(auto_depth(&relatives, limit)), true),
    };

    let mut entries: Vec<ListingEntry> = match effective_depth {
        None => matched
            .iter()
            .map(|path| ListingEntry {
                path: (*path).to_string(),
                kind: EntryKind::File,
                file_count: None,
            })
            .collect(),
        Some(depth) => {
            let mut rolled: Vec<(&str, usize)> = Vec::new();
            for relative in &relatives {
                let key = rollup_key(relative, depth);
                match rolled.last_mut() {
                    Some((last, count)) if *last == key => *count += 1,
                    _ => rolled.push((key, 1)),
                }
            }
            rolled
                .into_iter()
                .map(|(key, count)| {
                    let is_dir = key.ends_with('/');
                    ListingEntry {
                        path: format!("{scope}{key}"),
                        kind: if is_dir {
                            EntryKind::Dir
                        } else {
                            EntryKind::File
                        },
                        file_count: is_dir.then_some(count),
                    }
                })
                .collect()
        }
    };

    let total_entries = entries.len();
    let start = offset.min(total_entries);
    entries.drain(..start);
    let truncated = entries.len() > limit;
    entries.truncate(limit);
    Listing {
        entries,
        total_files,
        total_entries,
        depth: effective_depth,
        depth_auto,
        truncated,
        next_offset: truncated.then_some(start + limit),
    }
}

/// Deepest depth whose entry count still fits `limit`.
///
/// Falls back to depth 1 when even the top level overflows: the caller then
/// pages through top-level entries, which is the honest answer for a
/// repository with more top-level directories than the page size.
fn auto_depth(relatives: &[&str], limit: usize) -> usize {
    let deepest = relatives
        .iter()
        .map(|relative| relative.matches('/').count() + 1)
        .max()
        .unwrap_or(1)
        .min(MAX_AUTO_DEPTH);
    for depth in (1..=deepest).rev() {
        if entry_count_at_depth(relatives, depth) <= limit {
            return depth;
        }
    }
    1
}

impl Listing {
    pub fn to_json(&self, project: &str, scope: &str, list_truncated: bool) -> Value {
        // Offset pagination is authoritative only when the source acquisition
        // was complete. A bounded source prefix may still provide useful
        // structure, but advertising `next_offset` over that prefix would imply
        // the caller can eventually traverse files that were never acquired.
        let page_truncated = self.truncated && !list_truncated;
        let next_offset = if list_truncated {
            None
        } else {
            self.next_offset
        };
        json!({
            "project": project,
            "path": scope.trim_end_matches('/'),
            "entries": self
                .entries
                .iter()
                .map(|entry| {
                    let mut value = json!({ "path": entry.path, "kind": entry.kind.as_str() });
                    if let Some(count) = entry.file_count {
                        value["file_count"] = json!(count);
                    }
                    value
                })
                .collect::<Vec<_>>(),
            "returned": self.entries.len(),
            "total_files": self.total_files,
            "total_entries": self.total_entries,
            "depth": self.depth,
            "depth_auto": self.depth_auto,
            "truncated": page_truncated,
            "next_offset": next_offset,
            // Source acquisition ended before the Git index did, so the totals
            // undercount. Distinct from `truncated`, which is safe paging over a
            // complete source and is therefore suppressed while this is true.
            "list_truncated": list_truncated,
            "source": "git_index",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| (*entry).to_string()).collect()
    }

    #[test]
    fn literal_query_filters_before_paging_without_rollup() {
        let files = vec![
            "aaa.txt".into(),
            "src/%_中文.rs".into(),
            "src/long/deep/nested/path/with/more/than/sixteen/parts/a/b/c/d/e/f/g/h/中文.rs".into(),
            "zz.txt".into(),
        ];
        for depth in [None, Some(1), Some(2)] {
            let first = build_listing_with_query(&files, "", &[], Some("中文"), depth, 1, 0);
            assert_eq!(first.total_files, 2);
            assert_eq!(first.entries[0].kind, EntryKind::File);
            assert_eq!(first.entries[0].path, "src/%_中文.rs");
            assert_eq!(first.next_offset, Some(1));
            assert_eq!(first.depth, None);
            assert!(!first.depth_auto);
            assert_eq!(
                first.to_json("demo", "", false).get("depth"),
                Some(&Value::Null)
            );
            let last = build_listing_with_query(&files, "", &[], Some("中文"), depth, 1, 1);
            assert_eq!(last.entries[0].kind, EntryKind::File);
            assert_eq!(last.entries[0].path, files[2]);
            assert_eq!(last.next_offset, None);
            assert_eq!(last.depth, None);
        }
        let empty_query = build_listing_with_query(&files, "", &[], Some(""), Some(1), 1, 0);
        assert_eq!(empty_query.total_files, 4);
        assert_eq!(empty_query.depth, None);
        assert!(!empty_query.depth_auto);
        let literal = build_listing_with_query(&files, "", &[], Some("%_"), None, 10, 0);
        assert_eq!(literal.total_files, 1);
        assert_eq!(literal.entries[0].path, "src/%_中文.rs");
        assert_eq!(
            build_listing_with_query(&files, "", &[], Some("*.rs"), None, 10, 0).total_files,
            0
        );
    }

    #[test]
    fn credentials_and_bulk_trees_never_reach_the_model() {
        // Git tracks whatever it was told to track, including secrets. The
        // listing must not be the one file tool that hands them over.
        let tracked = paths(&[
            "src/main.rs",
            ".env",
            ".env.production",
            "deploy/runner.toml",
            "deploy/agent.toml",
            "certs/server.pem",
            "certs/server.key",
            "secrets/db.txt",
            "project-registry/one.toml",
            "projects.d/one.toml",
            "node_modules/left-pad/index.js",
            "target/debug/build.rs",
        ]);
        let listing = build_listing(&tracked, "", &[], None, 100, 0);
        let shown: Vec<&str> = listing
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert_eq!(shown, vec!["src/main.rs"], "leaked: {shown:?}");
        // The count is what survived the filter, not what git printed —
        // otherwise `total_files` silently advertises hidden paths.
        assert_eq!(listing.total_files, 1);
    }

    #[test]
    fn an_explicit_glob_does_not_reopen_the_filtered_paths() {
        // Asking for the secret by name is still asking for the secret.
        let tracked = paths(&["src/main.rs", ".env", "certs/server.key"]);
        let listing = build_listing(
            &tracked,
            "",
            &[".env".to_string(), "**/*.key".to_string()],
            None,
            100,
            0,
        );
        assert!(listing.entries.is_empty(), "{:?}", listing.entries);
        assert_eq!(listing.total_files, 0);
    }

    #[test]
    fn a_rolled_up_directory_does_not_count_files_it_hides() {
        // file_count is a claim about what a caller would find by descending.
        // Counting filtered paths would make that claim false.
        let tracked = paths(&["app/main.rs", "app/util.rs", "app/.env", "app/id.pem"]);
        let listing = build_listing(&tracked, "", &[], Some(1), 100, 0);
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "app/");
        assert_eq!(listing.entries[0].file_count, Some(2));
    }

    #[test]
    fn a_truncated_final_record_is_dropped_not_reported_as_a_file() {
        let (complete, truncated) = parse_nul_separated("a.rs\0b.rs\0");
        assert_eq!(complete, vec!["a.rs", "b.rs"]);
        assert!(!truncated);

        // No trailing NUL: the last path is a fragment of a real one.
        let (partial, truncated) = parse_nul_separated("a.rs\0src/very-long-na");
        assert_eq!(partial, vec!["a.rs"]);
        assert!(truncated);
    }

    #[test]
    fn scope_normalization_treats_root_spellings_alike() {
        for spelling in [None, Some(""), Some("."), Some("/"), Some("./")] {
            assert_eq!(normalize_scope(spelling), "", "{spelling:?}");
        }
        assert_eq!(normalize_scope(Some("src")), "src/");
        assert_eq!(normalize_scope(Some("/src/")), "src/");
        assert_eq!(normalize_scope(Some("src/db")), "src/db/");
    }

    #[test]
    fn a_bare_pattern_matches_the_basename_at_any_depth() {
        assert!(glob_matches("*.py", "train.py"));
        assert!(glob_matches("*.py", "src/models/train.py"));
        assert!(!glob_matches("*.py", "src/train.pyc"));
    }

    #[test]
    fn a_single_star_stops_at_a_slash_and_a_double_star_crosses_it() {
        assert!(glob_matches("src/*.rs", "src/main.rs"));
        assert!(!glob_matches("src/*.rs", "src/db/main.rs"));
        assert!(glob_matches("src/**/*.rs", "src/db/main.rs"));
        assert!(glob_matches("src/**", "src/db/main.rs"));
        assert!(glob_matches("?.rs", "a.rs"));
        assert!(!glob_matches("?.rs", "ab.rs"));
    }

    #[test]
    fn an_earlier_double_star_can_widen_after_a_later_single_star_hits_a_slash() {
        assert!(glob_matches("**/*", "a/b/c"));
        assert!(glob_matches("src/**/*", "src/a/b/c.rs"));
        assert!(!glob_matches("src/*/*", "src/a/b/c.rs"));
    }

    #[test]
    #[ignore = "manual glob complexity baseline; compare identical profiles on one machine"]
    fn glob_matcher_measurements() {
        use std::hint::black_box;
        use std::time::{Duration, Instant};

        for (n, k) in [(4, 2), (8, 4), (12, 6), (16, 8), (20, 10)] {
            let pattern = format!("**{}b", "a*".repeat(k));
            let text = format!("{}/b", "a".repeat(n));
            assert!(pattern.len() <= 32 && text.len() <= 32);
            let mut samples = [Duration::ZERO; 3];
            for elapsed in &mut samples {
                let start = Instant::now();
                let matched =
                    glob_match_segment(black_box(pattern.as_bytes()), black_box(text.as_bytes()));
                *elapsed = start.elapsed();
                assert!(!black_box(matched));
            }
            samples.sort_unstable();
            eprintln!(
                "glob_adversarial n={n} k={k} pattern_bytes={} text_bytes={} min_ns={} median_ns={} max_ns={}",
                pattern.len(), text.len(), samples[0].as_nanos(),
                samples[1].as_nanos(), samples[2].as_nanos(),
            );
            // Stop escalation, rather than making machine speed a CI gate.
            if samples[1] >= Duration::from_millis(250) {
                eprintln!("stopped before larger inputs: current case is already slow");
                break;
            }
        }

        let paths = [
            "src/main.rs",
            "src/server/router.rs",
            "README.md",
            "tests/ui.rs",
            "src/app.ts",
            "crates/core/lib.rs",
            "src/assets/logo.svg",
            "lib.rs",
        ];
        let repeats = 50_000;
        let mut samples = [Duration::ZERO; 3];
        for elapsed in &mut samples {
            let start = Instant::now();
            let mut matched = 0;
            for _ in 0..repeats {
                for path in paths {
                    matched += usize::from(glob_matches(black_box("*.rs"), black_box(path)));
                }
            }
            *elapsed = start.elapsed();
            assert_eq!(black_box(matched), 5 * repeats);
        }
        samples.sort_unstable();
        eprintln!(
            "glob_common calls={} min_ns={} median_ns={} max_ns={}",
            repeats * paths.len(),
            samples[0].as_nanos(),
            samples[1].as_nanos(),
            samples[2].as_nanos(),
        );
    }

    #[test]
    fn glob_matching_agrees_with_an_independent_bounded_reference() {
        fn reference(pattern: &[u8], text: &[u8]) -> bool {
            let Some((&first, rest)) = pattern.split_first() else {
                return text.is_empty();
            };
            match first {
                b'*' => {
                    let (remaining, max_consumed) = if rest.first() == Some(&b'*') {
                        (&rest[1..], text.len())
                    } else {
                        (
                            rest,
                            text.iter()
                                .position(|byte| *byte == b'/')
                                .unwrap_or(text.len()),
                        )
                    };
                    (0..=max_consumed).any(|count| reference(remaining, &text[count..]))
                }
                b'?' => !text.is_empty() && text[0] != b'/' && reference(rest, &text[1..]),
                literal => !text.is_empty() && text[0] == literal && reference(rest, &text[1..]),
            }
        }
        fn words(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
            let mut words = vec![Vec::new()];
            let mut level = 0..1;
            for _ in 0..max_len {
                let start = words.len();
                for i in level.clone() {
                    for byte in alphabet {
                        let mut next = words[i].clone();
                        next.push(*byte);
                        words.push(next);
                    }
                }
                level = start..words.len();
            }
            words
        }
        let patterns = words(b"a/*?", 5);
        let texts = words(b"ab/", 4);
        assert_eq!(patterns.len() * texts.len(), 165_165);
        for pattern in &patterns {
            for text in &texts {
                assert_eq!(
                    glob_match_segment(pattern, text),
                    reference(pattern, text),
                    "pattern={:?} text={:?}",
                    String::from_utf8_lossy(pattern),
                    String::from_utf8_lossy(text),
                );
            }
        }
    }

    #[test]
    fn glob_matching_keeps_byte_and_literal_separator_boundaries() {
        for (pattern, text, expected) in [
            ("?", "é", false),
            ("??", "é", true),
            ("?*", "é", true),
            ("*?", "é", true),
            ("*/*", "/", true),
            ("**?", "/", false),
            ("?**", "/", false),
            ("**/a", "a", false),
            ("**/a", "/a", true),
            ("ab*bc", "abc", false),
            ("ab*bc", "abbc", true),
            ("ab***bc", "ab/x/bc", true),
            ("a*/b", "a/x/b", false),
            ("a**/b", "a/x/b", true),
        ] {
            assert_eq!(
                glob_match_segment(pattern.as_bytes(), text.as_bytes()),
                expected,
                "pattern={pattern:?} text={text:?}"
            );
        }
        // A slash forces the old multi-star matcher to enumerate allocations.
        assert!(!glob_match_segment(
            b"**a*a*a*a*a*b",
            format!("{}/b", "a".repeat(2048)).as_bytes()
        ));
    }

    #[test]
    fn a_pathological_pattern_terminates_without_recursing() {
        // Would be exponential under naive backtracking recursion.
        let pattern = "**a**a**a**a**a**a**b";
        let text = "a".repeat(2048);
        assert!(!glob_matches(pattern, &text));
    }

    #[test]
    fn a_small_project_is_returned_whole_and_flat() {
        let files = paths(&["README.md", "src/main.rs", "src/db/mod.rs"]);
        let listing = build_listing(&files, "", &[], None, 50, 0);

        assert_eq!(listing.depth, None, "a list that fits must not roll up");
        assert!(!listing.depth_auto);
        assert_eq!(listing.entries.len(), 3);
        assert_eq!(listing.total_files, 3);
        assert!(!listing.truncated);
        assert!(listing.entries.iter().all(|e| e.kind == EntryKind::File));
    }

    #[test]
    fn a_project_too_large_to_list_rolls_up_to_the_deepest_depth_that_fits() {
        // 3 top-level dirs; 60 files spread two levels deep.
        let mut files = Vec::new();
        for top in ["alpha", "beta", "gamma"] {
            for mid in 0..4 {
                for leaf in 0..5 {
                    files.push(format!("{top}/m{mid}/f{leaf}.rs"));
                }
            }
        }
        assert_eq!(files.len(), 60);

        // Depth 2 yields 12 dirs, depth 1 yields 3. With limit 12 the deeper
        // one still fits and must win — more resolution for the same budget.
        let listing = build_listing(&files, "", &[], None, 12, 0);
        assert_eq!(listing.depth, Some(2));
        assert!(listing.depth_auto);
        assert_eq!(listing.entries.len(), 12);
        assert!(!listing.truncated);
        assert_eq!(listing.total_files, 60);
        assert_eq!(listing.entries[0].path, "alpha/m0/");
        assert_eq!(listing.entries[0].kind, EntryKind::Dir);
        assert_eq!(listing.entries[0].file_count, Some(5));
        // Every file is accounted for by exactly one rolled-up directory.
        let counted: usize = listing.entries.iter().filter_map(|e| e.file_count).sum();
        assert_eq!(counted, 60);

        // A tighter budget forces the shallower depth rather than a truncated
        // deep list, so the model still sees the whole project.
        let listing = build_listing(&files, "", &[], None, 5, 0);
        assert_eq!(listing.depth, Some(1));
        assert_eq!(listing.entries.len(), 3);
        assert!(!listing.truncated);
    }

    #[test]
    fn shallow_files_keep_their_identity_when_deeper_siblings_roll_up() {
        let files = paths(&["README.md", "src/main.rs", "src/db/mod.rs", "src/db/row.rs"]);
        let listing = build_listing(&files, "", &[], Some(1), 50, 0);

        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].path, "README.md");
        assert_eq!(listing.entries[0].kind, EntryKind::File);
        assert_eq!(listing.entries[1].path, "src/");
        assert_eq!(listing.entries[1].file_count, Some(3));
    }

    #[test]
    fn depth_is_counted_inside_the_requested_scope() {
        let files = paths(&[
            "src/db/mod.rs",
            "src/db/row.rs",
            "src/http/mod.rs",
            "top.rs",
        ]);
        let listing = build_listing(&files, "src/", &[], Some(1), 50, 0);

        // Scoped to src/, depth 1 shows src's children, not src itself.
        assert_eq!(listing.total_files, 3, "top.rs is outside the scope");
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|e| e.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/db/", "src/http/"]
        );
        assert_eq!(listing.entries[0].file_count, Some(2));
    }

    #[test]
    fn globs_filter_before_rollup_so_counts_describe_what_matched() {
        let files = paths(&[
            "src/a.rs",
            "src/b.py",
            "src/deep/c.rs",
            "src/deep/d.py",
            "docs/e.py",
        ]);
        let listing = build_listing(&files, "", &["*.py".to_string()], Some(1), 50, 0);

        assert_eq!(listing.total_files, 3);
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|e| (e.path.as_str(), e.file_count))
                .collect::<Vec<_>>(),
            vec![("docs/", Some(1)), ("src/", Some(2))]
        );
    }

    #[test]
    fn pagination_reports_the_offset_that_continues_the_page() {
        let files: Vec<String> = (0..10).map(|index| format!("f{index:02}.rs")).collect();

        let first = build_listing(&files, "", &[], None, 4, 0);
        assert_eq!(first.entries.len(), 4);
        assert!(first.truncated);
        assert_eq!(first.next_offset, Some(4));
        assert_eq!(first.total_entries, 10);

        let second = build_listing(&files, "", &[], None, 4, first.next_offset.unwrap());
        assert_eq!(second.entries[0].path, "f04.rs");
        assert_eq!(second.next_offset, Some(8));

        let last = build_listing(&files, "", &[], None, 4, 8);
        assert_eq!(last.entries.len(), 2);
        assert!(!last.truncated);
        assert_eq!(last.next_offset, None);
    }

    #[test]
    fn an_offset_past_the_end_returns_nothing_rather_than_wrapping() {
        let files = paths(&["a.rs", "b.rs"]);
        let listing = build_listing(&files, "", &[], None, 10, 99);
        assert!(listing.entries.is_empty());
        assert!(!listing.truncated);
        assert_eq!(listing.next_offset, None);
    }

    #[test]
    fn more_top_level_entries_than_the_limit_pages_instead_of_hiding_them() {
        let files: Vec<String> = (0..30).map(|index| format!("d{index:02}/f.rs")).collect();
        let listing = build_listing(&files, "", &[], None, 10, 0);

        assert_eq!(listing.depth, Some(1), "cannot fit even at depth 1");
        assert_eq!(listing.total_entries, 30);
        assert_eq!(listing.entries.len(), 10);
        assert!(listing.truncated);
        assert_eq!(listing.next_offset, Some(10));
    }

    #[test]
    fn source_incomplete_json_never_advertises_offset_continuation() {
        let files: Vec<String> = (0..10).map(|index| format!("f{index:02}.rs")).collect();
        let listing = build_listing(&files, "", &[], None, 4, 0);
        assert!(listing.truncated);
        assert_eq!(listing.next_offset, Some(4));

        let value = listing.to_json("proj", "", true);
        assert_eq!(value["returned"], 4);
        assert_eq!(value["list_truncated"], true);
        assert_eq!(value["truncated"], false);
        assert_eq!(value["next_offset"], Value::Null);
    }

    #[test]
    fn json_reports_the_scope_without_a_trailing_slash_and_flags_list_truncation() {
        let files = paths(&["src/a.rs"]);
        let value = build_listing(&files, "src/", &[], None, 10, 0).to_json("proj", "src/", true);

        assert_eq!(value["path"], "src");
        assert_eq!(value["project"], "proj");
        assert_eq!(value["list_truncated"], true);
        assert_eq!(value["depth"], Value::Null);
        assert_eq!(value["entries"][0]["path"], "src/a.rs");
        assert_eq!(value["entries"][0]["kind"], "file");
        assert!(value["entries"][0].get("file_count").is_none());
        assert_eq!(value["source"], "git_index");
    }
}
