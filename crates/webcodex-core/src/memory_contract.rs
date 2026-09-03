//! Stable Memory schema and domain bounds shared by persistence and tool contracts.

pub const MAX_MEMORY_KEY_CHARS: usize = 96;
pub const MAX_MEMORY_SUMMARY_CHARS: usize = 512;
pub const MAX_MEMORY_BODY_BYTES: usize = 8 * 1024;
pub const MAX_MEMORY_TAGS: usize = 8;
pub const MAX_MEMORY_TAG_CHARS: usize = 64;
pub const MAX_MEMORY_QUERY_CHARS: usize = 200;
pub const MAX_MEMORY_SEARCH_LIMIT: usize = 50;
pub const MAX_MEMORY_BOOTSTRAP_BYTES: usize = 8 * 1024;
pub const MAX_MEMORY_SEARCH_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_MEMORY_SCOPE_LIST_LIMIT: usize = 100;
