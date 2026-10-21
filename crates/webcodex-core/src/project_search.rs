//! Shared native/fallback project text-search exclusions and output bounds.

pub const PROJECT_TEXT_SEARCH_EXCLUDE_GLOBS: &[&str] = &[
    "!.git/**",
    "!**/.git/**",
    "!.claude/**",
    "!**/.claude/**",
    "!target/**",
    "!**/target/**",
    "!node_modules/**",
    "!**/node_modules/**",
    "!**/.venv/**",
    "!**/venv/**",
    "!**/__pycache__/**",
    "!**/.pytest_cache/**",
    "!**/.mypy_cache/**",
    "!**/.ruff_cache/**",
    "!**/.tox/**",
    "!**/site-packages/**",
    "!**/.ipynb_checkpoints/**",
    "!**/dist/**",
    "!**/build/**",
    "!**/coverage/**",
    "!**/.next/**",
    "!secrets/**",
    "!**/secrets/**",
    "!tokens/**",
    "!**/tokens/**",
    "!.env",
    "!**/.env",
    "!.env.*",
    "!**/.env.*",
    "!runner.toml",
    "!**/runner.toml",
    "!agent.toml",
    "!**/agent.toml",
    "!webcodex.env",
    "!**/webcodex.env",
    "!*.pem",
    "!**/*.pem",
    "!*.key",
    "!**/*.key",
];

pub const PROJECT_TEXT_SEARCH_OUTPUT_BYTE_BUDGET: usize = 32 * 1024;

/// Backend options shared by generated POSIX commands and native Runner search.
/// User globs precede mandatory exclusions because the last matching rg glob wins.
/// The caller renders glob values as literal argv or quoted shell arguments.
pub fn project_text_search_rg_options(
    result_mode: &str,
    before: usize,
    after: usize,
    include: &[impl AsRef<str>],
    exclude: &[impl AsRef<str>],
    render_glob: impl Fn(&str) -> String,
) -> Option<Vec<String>> {
    let mode_args: &[&str] = match result_mode {
        "matches" => &["--with-filename", "--null", "--line-number", "--no-heading"],
        "files_with_matches" => &["--files-with-matches"],
        "count" => &["--with-filename", "--count", "--null"],
        _ => return None,
    };
    let mut args = mode_args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    if result_mode == "matches" {
        args.extend([
            "-B".to_string(),
            before.to_string(),
            "-A".to_string(),
            after.to_string(),
        ]);
    }
    args.extend(["--color", "never", "--hidden"].map(str::to_string));
    for glob in include {
        args.extend(["--glob".to_string(), render_glob(glob.as_ref())]);
    }
    for glob in exclude {
        args.extend([
            "--glob".to_string(),
            render_glob(&format!("!{}", glob.as_ref())),
        ]);
    }
    for glob in PROJECT_TEXT_SEARCH_EXCLUDE_GLOBS {
        args.extend(["--glob".to_string(), render_glob(glob)]);
    }
    Some(args)
}

pub fn project_text_search_line_budget(
    limit: usize,
    matches: bool,
    before: usize,
    after: usize,
) -> usize {
    let results = limit.saturating_add(1);
    if !matches || before == 0 && after == 0 {
        return results;
    }
    results
        .saturating_mul(before.saturating_add(after).saturating_add(2))
        .saturating_add(1)
}
