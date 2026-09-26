//! Project instructions auto-load.
//!
//! When a session is started for a project, WebPi best-effort loads
//! project-local instruction files (e.g. `AGENTS.md`) so GPT Action / MCP /
//! Codex / GLM callers see project-local development rules at session start.
//!
//! These files are project-local guidance only; they never override system,
//! platform, or WebPi safety policy. Only a fixed candidate whitelist is
//! read; arbitrary caller-supplied paths and secrets are never read. Read
//! failures never cause startup itself to fail. `start_session` retains its
//! first-match behavior; coding startup observes every fixed candidate and
//! marks an incomplete scan unavailable so a transient read failure cannot be
//! mistaken for a source deletion.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Conservative total character cap across all loaded instruction files.
pub const MAX_TOTAL_CHARS: usize = 32 * 1024;
/// Conservative per-file line cap.
pub const MAX_LINES_PER_FILE: usize = 400;

/// Fixed, ordered candidate instruction file paths tried at session start.
pub const INSTRUCTION_CANDIDATE_PATHS: &[&str] = &[
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    ".codex/AGENTS.md",
    ".github/copilot-instructions.md",
];

const PROJECT_INSTRUCTIONS_NOTE: &str = "Runner-configured and project-local instructions are model guidance only; they do not override system, platform, or WebPi safety policy.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionSourceScope {
    Runner,
    Project,
}

/// Hint for reading the remainder of a truncated instruction file via
/// `read_file` (`path` / `start_line` / `limit`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadMoreHint {
    pub path: String,
    pub start_line: usize,
    pub limit: usize,
}

/// One bounded loaded instruction source retained only in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstructionFile {
    pub source_scope: InstructionSourceScope,
    pub path: String,
    pub fingerprint: String,
    pub content: String,
    pub chars: usize,
    pub total_lines: usize,
    pub start_line: usize,
    pub limit: usize,
    pub truncated: bool,
    pub read_more: Option<ReadMoreHint>,
}

/// Summary projection of one instruction file (no content). Returned by
/// `session_summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstructionFileSummary {
    pub source_scope: InstructionSourceScope,
    pub path: String,
    pub fingerprint: String,
    pub chars: usize,
    pub total_lines: usize,
    pub start_line: usize,
    pub limit: usize,
    pub truncated: bool,
    pub read_more: Option<ReadMoreHint>,
}

/// Control-owned observation state. Never serialized into model projections or
/// durable Session records. Generation is unknown during transport failures when
/// the Runner has not advertised config status.
#[derive(Debug, Clone)]
pub struct RunnerInstructionObservation {
    pub instance_id: String,
    pub generation: Option<u64>,
    pub started_at: std::time::Instant,
    /// Latest successful live-instance check, independent of request age.
    pub instance_verified_at: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct InstructionScanState {
    pub runner_complete: bool,
    pub project_complete: bool,
    pub runner: Option<RunnerInstructionObservation>,
    /// Captured before Project reads, including when Runner discovery fails.
    pub project_started_at: Option<std::time::Instant>,
    /// Independently bounded Runner sources before the shared projection budget.
    /// This state is Session-local and skipped with the rest of `scan`.
    pub runner_source_files: Vec<ProjectInstructionFile>,
}

/// Bounded snapshot of loaded project instructions (with content). Stored only
/// on the in-memory `SessionRecord`; durable persistence deliberately drops it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstructionsSnapshot {
    pub loaded: bool,
    pub files: Vec<ProjectInstructionFile>,
    pub candidate_paths: Vec<String>,
    pub total_chars: usize,
    pub max_total_chars: usize,
    pub truncated: bool,
    /// False when one or more fixed candidates could not be observed because
    /// the owning runner/capability/read operation was unavailable. Missing
    /// files and empty files are successful observations.
    pub scan_complete: bool,
    #[serde(skip)]
    pub scan: Option<InstructionScanState>,
    pub note: String,
}

/// Summary-only snapshot (no file content) used by `session_summary` so the
/// summary does not echo large instruction bodies back on every call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInstructionsSummarySnapshot {
    pub loaded: bool,
    pub files: Vec<ProjectInstructionFileSummary>,
    pub candidate_paths: Vec<String>,
    pub total_chars: usize,
    pub max_total_chars: usize,
    pub truncated: bool,
    pub scan_complete: bool,
    pub note: String,
}

/// One successfully observed, non-empty fixed instruction candidate before
/// the shared per-file and aggregate snapshot bounds are applied.
#[derive(Debug, Clone)]
pub struct LoadedInstructionCandidate {
    pub source_scope: InstructionSourceScope,
    pub path: String,
    pub content: String,
    pub total_lines: usize,
    /// Full-file SHA-256 reported by the trusted runner range-read envelope.
    /// The snapshot fingerprint also incorporates the returned bounded body,
    /// so a malformed/stale test double cannot hide a visible content change.
    pub full_sha256: Option<String>,
}

fn candidate_paths() -> Vec<String> {
    INSTRUCTION_CANDIDATE_PATHS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl ProjectInstructionsSnapshot {
    /// Empty snapshot (`loaded = false`) used when no candidate could be read.
    pub fn empty() -> Self {
        Self {
            loaded: false,
            files: Vec::new(),
            candidate_paths: candidate_paths(),
            total_chars: 0,
            max_total_chars: MAX_TOTAL_CHARS,
            truncated: false,
            scan_complete: true,
            scan: None,
            note: PROJECT_INSTRUCTIONS_NOTE.to_string(),
        }
    }

    /// Empty, conservatively unavailable snapshot used when one or more fixed
    /// candidates could not be observed and no rule body was recovered.
    pub fn unavailable() -> Self {
        Self {
            scan_complete: false,
            ..Self::empty()
        }
    }

    /// Build a snapshot from a single successfully-read instruction file,
    /// applying the per-file line cap and the total char cap. The first
    /// successful candidate wins, so `files` holds at most one entry.
    #[cfg(test)]
    pub fn from_single_file(path: &str, content: String, total_lines: usize) -> Self {
        Self::from_candidates(
            vec![LoadedInstructionCandidate {
                source_scope: InstructionSourceScope::Project,
                path: path.to_string(),
                content,
                total_lines,
                full_sha256: None,
            }],
            true,
        )
    }

    /// Build one ordered, aggregate-bounded snapshot from all successfully
    /// observed fixed candidates. This is used by coding startup so additions,
    /// removals, and changes in any supported repository rule source can be
    /// detected without reading caller-selected paths.
    pub fn from_candidates(
        candidates: Vec<LoadedInstructionCandidate>,
        scan_complete: bool,
    ) -> Self {
        let mut remaining_chars = MAX_TOTAL_CHARS;
        let mut files = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let file = build_instruction_file(
                candidate.source_scope,
                &candidate.path,
                candidate.content,
                candidate.total_lines,
                candidate.full_sha256.as_deref(),
                remaining_chars,
            );
            remaining_chars = remaining_chars.saturating_sub(file.chars);
            files.push(file);
        }
        let total_chars = files.iter().map(|file| file.chars).sum();
        let truncated = files.iter().any(|file| file.truncated);
        Self {
            loaded: !files.is_empty(),
            files,
            candidate_paths: candidate_paths(),
            total_chars,
            max_total_chars: MAX_TOTAL_CHARS,
            truncated,
            scan_complete,
            scan: None,
            note: PROJECT_INSTRUCTIONS_NOTE.to_string(),
        }
    }

    /// Project to a content-less summary for `session_summary`.
    pub fn to_summary(&self) -> ProjectInstructionsSummarySnapshot {
        ProjectInstructionsSummarySnapshot {
            loaded: self.loaded,
            files: self
                .files
                .iter()
                .map(|f| ProjectInstructionFileSummary {
                    source_scope: f.source_scope,
                    path: f.path.clone(),
                    fingerprint: f.fingerprint.clone(),
                    chars: f.chars,
                    total_lines: f.total_lines,
                    start_line: f.start_line,
                    limit: f.limit,
                    truncated: f.truncated,
                    read_more: f.read_more.clone(),
                })
                .collect(),
            candidate_paths: self.candidate_paths.clone(),
            total_chars: self.total_chars,
            max_total_chars: self.max_total_chars,
            truncated: self.truncated,
            scan_complete: self.scan_complete,
            note: self.note.clone(),
        }
    }
}

impl ProjectInstructionsSnapshot {
    pub fn scope_complete(&self, scope: InstructionSourceScope) -> bool {
        self.scan
            .as_ref()
            .map_or(self.scan_complete, |scan| match scope {
                InstructionSourceScope::Runner => scan.runner_complete,
                InstructionSourceScope::Project => scan.project_complete,
            })
    }

    /// Retain each unavailable scope independently, under the Session store
    /// lock. A new Runner/config must never inherit the old global rule body.
    pub fn retain_unavailable_scopes(mut self, previous: Option<&Self>) -> Self {
        let Some(previous) = previous else {
            return self;
        };
        let incoming_observation = self.scan.as_ref().and_then(|scan| scan.runner.clone());
        let incoming = incoming_observation.as_ref();
        let prior = previous.scan.as_ref().and_then(|scan| scan.runner.as_ref());
        let runner_stale = incoming.zip(prior).is_some_and(|(new, old)| {
            if new.instance_id != old.instance_id {
                // A different identity alone cannot prove replacement: an old
                // process's response can arrive after its successor committed.
                return new.instance_verified_at <= old.instance_verified_at;
            }
            match (new.generation, old.generation) {
                (Some(new), Some(old)) if new != old => new < old,
                (None, Some(_)) => true,
                (Some(_), None) => false,
                _ => new.started_at < old.started_at,
            }
        });
        let same_runner = incoming
            .zip(prior)
            .map_or(incoming.is_none(), |(new, old)| {
                new.instance_id == old.instance_id
                    && new
                        .generation
                        .map_or(true, |generation| Some(generation) == old.generation)
            });
        let retain_runner =
            runner_stale || (!self.scope_complete(InstructionSourceScope::Runner) && same_runner);
        let project_started_at = self.scan.as_ref().and_then(|scan| scan.project_started_at);
        let prior_project_started_at = previous
            .scan
            .as_ref()
            .and_then(|scan| scan.project_started_at);
        let project_stale = prior_project_started_at
            .is_some_and(|old| project_started_at.is_none_or(|new| new < old));
        let retain_project = project_stale || !self.scope_complete(InstructionSourceScope::Project);
        let runner_files = if retain_runner {
            previous.runner_source_files()
        } else {
            self.runner_source_files()
        }
        .iter()
        .filter(|file| file.source_scope == InstructionSourceScope::Runner)
        .cloned()
        .collect();
        let project_files = if retain_project {
            &previous.files
        } else {
            &self.files
        }
        .iter()
        .filter(|file| file.source_scope == InstructionSourceScope::Project)
        .cloned()
        .collect::<Vec<_>>();
        let mut scan = self.scan.take().unwrap_or(InstructionScanState {
            runner_complete: self.scan_complete,
            project_complete: self.scan_complete,
            runner: None,
            project_started_at: None,
            runner_source_files: Vec::new(),
        });
        if runner_stale || incoming.is_none() {
            scan.runner = prior.cloned();
        }
        // Even when body/order selection keeps the prior observation, preserve
        // the latest check of that same live instance for replacement fencing.
        if let Some(selected) = scan.runner.as_mut() {
            for observed in [incoming, prior].into_iter().flatten() {
                if observed.instance_id == selected.instance_id {
                    selected.instance_verified_at = selected
                        .instance_verified_at
                        .max(observed.instance_verified_at);
                }
            }
        }
        if runner_stale {
            scan.runner_complete = false;
        }
        if project_stale {
            scan.project_started_at = prior_project_started_at;
            scan.project_complete = false;
        }
        let project = Self {
            loaded: !project_files.is_empty(),
            total_chars: project_files.iter().map(|file| file.chars).sum(),
            truncated: project_files.iter().any(|file| file.truncated),
            files: project_files,
            scan_complete: scan.project_complete,
            ..Self::empty()
        };
        let mut combined = Self::with_runner_files(runner_files, project, scan.runner_complete);
        let combined_scan = combined.scan.as_mut().expect("combined scan");
        combined_scan.runner = scan.runner;
        combined_scan.project_started_at = scan.project_started_at;
        combined
    }

    fn runner_source_files(&self) -> &[ProjectInstructionFile] {
        self.scan
            .as_ref()
            .map_or(&self.files, |scan| &scan.runner_source_files)
    }

    /// Compose one bounded startup snapshot with Runner-configured sources first,
    /// followed by project-local sources. Runner sources never gain a generic
    /// read-more path; project-local read-more hints remain project-relative.
    pub fn with_runner_files(
        runner_files: Vec<ProjectInstructionFile>,
        project: Self,
        runner_scan_complete: bool,
    ) -> Self {
        // Reserve the already-bounded project body before spending on global
        // guidance. Presentation order remains global-before-project.
        let project_chars: usize = project.files.iter().map(|file| file.chars).sum();
        let runner_source_files = bound_runner_files(runner_files, MAX_TOTAL_CHARS);
        let mut files = bound_runner_files(
            runner_source_files.clone(),
            MAX_TOTAL_CHARS.saturating_sub(project_chars),
        );
        files.extend(project.files);
        let total_chars = files.iter().map(|file| file.chars).sum();
        let truncated = files.iter().any(|file| file.truncated);
        Self {
            loaded: !files.is_empty(),
            files,
            candidate_paths: candidate_paths(),
            total_chars,
            max_total_chars: MAX_TOTAL_CHARS,
            truncated,
            scan: Some(InstructionScanState {
                runner_complete: runner_scan_complete,
                project_complete: project.scan_complete,
                runner: None,
                project_started_at: project
                    .scan
                    .as_ref()
                    .and_then(|scan| scan.project_started_at),
                runner_source_files,
            }),
            scan_complete: runner_scan_complete && project.scan_complete,
            note: PROJECT_INSTRUCTIONS_NOTE.to_string(),
        }
    }
}

fn bound_runner_files(
    mut files: Vec<ProjectInstructionFile>,
    mut remaining_chars: usize,
) -> Vec<ProjectInstructionFile> {
    for file in &mut files {
        file.read_more = None;
        let original_chars = file.content.chars().count();
        if original_chars > remaining_chars {
            let mut kept = String::new();
            let mut chars = 0usize;
            for (index, line) in file.content.lines().enumerate() {
                let line_chars = line.chars().count();
                let separator = usize::from(index > 0);
                if chars + separator + line_chars > remaining_chars {
                    break;
                }
                if index > 0 {
                    kept.push('\n');
                }
                kept.push_str(line);
                chars += separator + line_chars;
            }
            file.content = kept;
            file.chars = chars;
            file.truncated = true;
        }
        remaining_chars = remaining_chars.saturating_sub(file.chars);
    }
    files
}

/// Apply the per-file line cap (`MAX_LINES_PER_FILE`) and the total char cap
/// (`MAX_TOTAL_CHARS`) to a raw instruction file body.
///
/// `content` is the raw text returned by the reader. For agent projects the
/// reader requests `MAX_LINES_PER_FILE + 1` lines so a returned line count
/// strictly greater than `MAX_LINES_PER_FILE` reliably signals line
/// truncation regardless of response format. `total_lines` is the best-known
/// true total line count of the file (exact for local reads and for the
/// `webcodex.file_read_range.v1` JSON format; a lower bound for plain-text
/// agent fallback).
fn build_instruction_file(
    source_scope: InstructionSourceScope,
    path: &str,
    content: String,
    total_lines: usize,
    full_sha256: Option<&str>,
    max_chars: usize,
) -> ProjectInstructionFile {
    let fingerprint =
        instruction_fingerprint(source_scope, path, &content, total_lines, full_sha256);
    let all_lines: Vec<&str> = content.lines().collect();
    let returned_lines = all_lines.len();
    let line_truncated = returned_lines > MAX_LINES_PER_FILE || total_lines > MAX_LINES_PER_FILE;
    let line_cap = if line_truncated {
        MAX_LINES_PER_FILE
    } else {
        returned_lines
    };

    // Apply the total char cap on top of the line cap, keeping only full lines
    // that fit. `lines_kept` tracks the boundary so `read_more` can point the
    // caller at the next line to read.
    let mut kept = String::new();
    let mut lines_kept = 0usize;
    let mut char_count = 0usize;
    let mut char_truncated = false;
    for (idx, line) in all_lines.iter().take(line_cap).enumerate() {
        let line_chars = line.chars().count();
        let separator = if idx == 0 { 0 } else { 1 };
        if char_count + separator + line_chars > max_chars {
            char_truncated = true;
            break;
        }
        if idx > 0 {
            kept.push('\n');
        }
        kept.push_str(line);
        char_count += separator + line_chars;
        lines_kept += 1;
    }

    let truncated = line_truncated || char_truncated;
    let chars = kept.chars().count();
    let reported_total = if total_lines >= returned_lines {
        total_lines
    } else {
        returned_lines
    };
    let read_more = if truncated && source_scope == InstructionSourceScope::Project {
        Some(ReadMoreHint {
            path: path.to_string(),
            start_line: lines_kept.saturating_add(1),
            limit: MAX_LINES_PER_FILE,
        })
    } else {
        None
    };

    ProjectInstructionFile {
        source_scope,
        path: path.to_string(),
        fingerprint,
        content: kept,
        chars,
        total_lines: reported_total,
        start_line: 1,
        limit: MAX_LINES_PER_FILE,
        truncated,
        read_more,
    }
}

fn instruction_fingerprint(
    source_scope: InstructionSourceScope,
    path: &str,
    returned_content: &str,
    total_lines: usize,
    full_sha256: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.instruction-source.v2\0");
    hasher.update(match source_scope {
        InstructionSourceScope::Runner => b"runner".as_slice(),
        InstructionSourceScope::Project => b"project".as_slice(),
    });
    for value in [
        path.as_bytes(),
        full_sha256
            .filter(|value| is_lower_hex_sha256(value))
            .unwrap_or("")
            .as_bytes(),
        returned_content.as_bytes(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    hasher.update((total_lines as u64).to_be_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_is_not_loaded() {
        let snap = ProjectInstructionsSnapshot::empty();
        assert!(!snap.loaded);
        assert!(snap.files.is_empty());
        assert_eq!(snap.max_total_chars, MAX_TOTAL_CHARS);
        assert_eq!(
            snap.candidate_paths,
            INSTRUCTION_CANDIDATE_PATHS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert!(snap.note.contains("model guidance only"));
    }

    #[test]
    fn from_single_file_small_file_is_not_truncated() {
        let body = "line one\nline two\n".to_string();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 2);
        assert!(snap.loaded);
        assert_eq!(snap.files.len(), 1);
        let file = &snap.files[0];
        assert_eq!(file.path, "AGENTS.md");
        assert!(!file.truncated);
        assert!(file.read_more.is_none());
        assert_eq!(file.total_lines, 2);
        assert_eq!(file.start_line, 1);
        assert_eq!(file.limit, MAX_LINES_PER_FILE);
        assert!(file.content.contains("line one"));
        assert_eq!(file.chars, file.content.chars().count());
        assert_eq!(snap.total_chars, file.chars);
        assert!(!snap.truncated);
    }

    #[test]
    fn line_truncation_marks_truncated_and_read_more() {
        // 500 lines: reader returns the first MAX_LINES_PER_FILE + 1 lines.
        let body = (0..(MAX_LINES_PER_FILE + 1))
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 500);
        let file = &snap.files[0];
        assert!(file.truncated);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert_eq!(read_more.path, "AGENTS.md");
        assert_eq!(read_more.start_line, MAX_LINES_PER_FILE + 1);
        assert_eq!(read_more.limit, MAX_LINES_PER_FILE);
        // Kept content is exactly MAX_LINES_PER_FILE lines.
        assert_eq!(file.content.lines().count(), MAX_LINES_PER_FILE);
        assert_eq!(file.total_lines, 500);
        assert!(snap.truncated);
    }

    #[test]
    fn line_truncation_plain_text_lower_bound_total_lines() {
        // Simulates the plain-text agent fallback: the reader returns 401 lines
        // and total_lines is only the returned count (a lower bound).
        let body = (0..(MAX_LINES_PER_FILE + 1))
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let returned_lines = body.lines().count();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, returned_lines);
        let file = &snap.files[0];
        assert!(file.truncated);
        assert_eq!(file.total_lines, returned_lines);
        assert!(file.read_more.is_some());
    }

    #[test]
    fn char_truncation_keeps_full_lines_and_points_read_more_at_next_line() {
        // A short file (few lines) but the first line alone exceeds the char cap.
        let big_line = "x".repeat(MAX_TOTAL_CHARS + 1);
        let body = format!("{big_line}\nsecond line\nthird line\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("CLAUDE.md", body, 3);
        let file = &snap.files[0];
        assert!(file.truncated);
        // The first line alone already exceeds the cap, so zero full lines fit.
        assert_eq!(lines_kept_from_content(&file.content), 0);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert_eq!(read_more.start_line, 1);
    }

    #[test]
    fn char_truncation_with_several_fit_lines() {
        // Many small lines whose combined size exceeds the char cap.
        let line = "x".repeat(MAX_TOTAL_CHARS / 10 + 1);
        let body = (0..50).map(|_| line.clone()).collect::<Vec<_>>().join("\n");
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 50);
        let file = &snap.files[0];
        assert!(file.truncated);
        assert!(file.chars <= MAX_TOTAL_CHARS);
        let read_more = file.read_more.as_ref().expect("read_more hint");
        assert!(read_more.start_line > 1);
        assert!(read_more.start_line <= MAX_LINES_PER_FILE + 1);
    }

    #[test]
    fn exactly_at_line_cap_is_not_truncated() {
        let body = (0..MAX_LINES_PER_FILE)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snap =
            ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, MAX_LINES_PER_FILE);
        let file = &snap.files[0];
        assert!(!file.truncated);
        assert!(file.read_more.is_none());
        assert_eq!(file.total_lines, MAX_LINES_PER_FILE);
    }

    #[test]
    fn summary_projection_omits_content() {
        let body = "alpha\nbeta\n".to_string();
        let snap = ProjectInstructionsSnapshot::from_single_file("AGENTS.md", body, 2);
        let summary = snap.to_summary();
        assert!(summary.loaded);
        assert_eq!(summary.files.len(), 1);
        let serialized = serde_json::to_string(&summary.files[0]).unwrap();
        assert!(!serialized.contains("content"));
        assert!(serialized.contains("chars"));
        assert!(serialized.contains("total_lines"));
        assert_eq!(summary.total_chars, snap.total_chars);
        assert_eq!(summary.candidate_paths, snap.candidate_paths);
    }

    #[test]
    fn runner_sources_precede_project_sources_and_never_offer_read_more() {
        let runner = ProjectInstructionsSnapshot::from_candidates(
            vec![LoadedInstructionCandidate {
                source_scope: InstructionSourceScope::Runner,
                path: "runner/0/AGENTS.md".to_string(),
                content: "runner guidance".to_string(),
                total_lines: 1,
                full_sha256: None,
            }],
            true,
        );
        let project = ProjectInstructionsSnapshot::from_single_file(
            "AGENTS.md",
            "project guidance".into(),
            1,
        );
        let combined = ProjectInstructionsSnapshot::with_runner_files(runner.files, project, true);

        assert_eq!(combined.files.len(), 2);
        assert_eq!(
            combined.files[0].source_scope,
            InstructionSourceScope::Runner
        );
        assert_eq!(combined.files[0].path, "runner/0/AGENTS.md");
        assert_eq!(
            combined.files[1].source_scope,
            InstructionSourceScope::Project
        );
        assert_eq!(combined.files[1].path, "AGENTS.md");
        assert!(combined.files[0].read_more.is_none());
    }

    #[test]
    fn truncated_runner_source_has_no_generic_read_more() {
        let body = (0..(MAX_LINES_PER_FILE + 10))
            .map(|i| format!("runner-line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snap = ProjectInstructionsSnapshot::from_candidates(
            vec![LoadedInstructionCandidate {
                source_scope: InstructionSourceScope::Runner,
                path: "runner/0/AGENTS.md".to_string(),
                content: body,
                total_lines: MAX_LINES_PER_FILE + 10,
                full_sha256: None,
            }],
            true,
        );
        assert!(snap.files[0].truncated);
        assert!(snap.files[0].read_more.is_none());
    }

    fn lines_kept_from_content(content: &str) -> usize {
        if content.is_empty() {
            0
        } else {
            content.lines().count()
        }
    }
}

#[cfg(test)]
#[path = "project_instructions_observation_tests.rs"]
mod observation_tests;
