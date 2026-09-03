use serde_json::{json, Value};

use webcodex_core::memory_contract::{
    MAX_MEMORY_BODY_BYTES, MAX_MEMORY_KEY_CHARS, MAX_MEMORY_QUERY_CHARS,
    MAX_MEMORY_SCOPE_LIST_LIMIT, MAX_MEMORY_SEARCH_LIMIT, MAX_MEMORY_SUMMARY_CHARS,
    MAX_MEMORY_TAGS, MAX_MEMORY_TAG_CHARS,
};

fn memory_key_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": MAX_MEMORY_KEY_CHARS,
        "pattern": "^[A-Za-z0-9._-]+$",
        "description": "Stable project-scoped semantic Memory key. It is not a filesystem path; '.' and '..' are rejected at runtime."
    })
}

fn expected_revision_schema() -> Value {
    json!({
        "type": "string",
        "pattern": "^wc_memrev_[0-9a-f]{64}$",
        "description": "Explicit Memory state revision / ETag CAS guard. It identifies the current incarnation generation, not only the current content definition."
    })
}

fn tags_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": MAX_MEMORY_TAGS,
        "items": {"type": "string", "minLength": 1, "maxLength": MAX_MEMORY_TAG_CHARS},
        "description": "Optional bounded deterministic tags. Tags are metadata, not authority."
    })
}

fn memory_set_tags_schema() -> Value {
    let mut schema = tags_schema();
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "description".to_string(),
            json!("Optional bounded deterministic tags. On create omission means no tags; on CAS update omission preserves current tags. Tags are metadata, not authority."),
        );
    }
    schema
}

pub fn memory_search_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {"type":"string","minLength":1,"description":"Required authorized runtime Project id."},
            "query": {"type":"string","maxLength":MAX_MEMORY_QUERY_CHARS,"description":"Optional case-insensitive literal substring over memory_key, summary, body, and tags. No regex, SQL, FTS, or embedding query language."},
            "tags": tags_schema(),
            "offset": {"type":"integer","minimum":0},
            "limit": {"type":"integer","minimum":1,"maximum":MAX_MEMORY_SEARCH_LIMIT},
            "expected_catalog_revision": {"type":"string","pattern":"^wc_memcat_[0-9a-f]{64}$","description":"Optional catalog guard. A stale value fails with memory_catalog_changed rather than continuing an old offset."},
            "session_id": {"type":"string","description":"Optional explicit Workflow Session for metadata-only consequence recording. It grants no Memory authority."}
        },
        "required": ["project"],
        "additionalProperties": false
    })
}

pub fn memory_read_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {"type":"string","minLength":1},
            "memory_key": memory_key_schema(),
            "expected_revision": expected_revision_schema(),
            "session_id": {"type":"string"}
        },
        "required": ["project", "memory_key"],
        "additionalProperties": false
    })
}

pub fn memory_set_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {"type":"string","minLength":1,"description":"Authorized project scope. Project re-registration to another registered root resolves to a distinct internal Memory namespace."},
            "memory_key": memory_key_schema(),
            "summary": {"type":"string","minLength":1,"maxLength":MAX_MEMORY_SUMMARY_CHARS,"description":"Required lightweight project guidance summary. Do not persist credentials, passwords, access tokens, private keys, or other secrets in Memory."},
            "body": {"type":"string","maxLength":MAX_MEMORY_BODY_BYTES,"description":"Optional detailed UTF-8 guidance, runtime-bounded to 8 KiB by bytes. On create omission means empty body; on CAS update omission preserves the current body. Returned only by memory_read. Memory is not a secret vault."},
            "priority": {"type":"string","enum":["high","normal","low"],"description":"On create omission means normal; on CAS update omission preserves the current priority. Priority only orders Memory entries within memory.bootstrap and never increases trust, authority, or instruction precedence."},
            "bootstrap": {"type":"boolean","description":"Only bootstrap=true Memories are eligible for caller-explicit memory.bootstrap sidecar projection. On create omission means false; on CAS update omission preserves the current value."},
            "tags": memory_set_tags_schema(),
            "expected_revision": expected_revision_schema(),
            "session_id": {"type":"string"}
        },
        "required": ["project", "memory_key", "summary"],
        "additionalProperties": false
    })
}

pub fn memory_delete_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {"type":"string","minLength":1},
            "memory_key": memory_key_schema(),
            "expected_revision": expected_revision_schema(),
            "session_id": {"type":"string"}
        },
        "required": ["project", "memory_key", "expected_revision"],
        "additionalProperties": false
    })
}

pub fn memory_scope_list_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "offset": {"type":"integer","minimum":0},
            "limit": {"type":"integer","minimum":1,"maximum":MAX_MEMORY_SCOPE_LIST_LIMIT}
        },
        "additionalProperties": false
    })
}

pub fn memory_scope_purge_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "memory_scope_id": {
                "type":"string",
                "pattern":"^wc_memscope_[0-9a-f]{64}$",
                "description":"Opaque Control-owned project Memory scope identity from memory_scope_list. It contains no native root path."
            },
            "expected_catalog_revision": {
                "type":"string",
                "pattern":"^wc_memcat_[0-9a-f]{64}$",
                "description":"Required scope-content CAS fence from memory_scope_list. Any Memory add/update/delete/recreate makes a prior value stale."
            },
            "confirm": {
                "type":"boolean",
                "description":"Must be true. Confirmation never overrides current or unknown scope status."
            }
        },
        "required": ["memory_scope_id", "expected_catalog_revision", "confirm"],
        "additionalProperties": false
    })
}
