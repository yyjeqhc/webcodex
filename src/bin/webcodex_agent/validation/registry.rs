//! Agent-side validation adapter registry (execution metadata only).
//!
//! Metadata here is intentionally declarative. Parser and command construction
//! live with each adapter implementation so the server never holds function
//! pointers. Adapter ids must stay stable and aligned with any future server
//! ValidationProfile metadata.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValidationAdapterMeta {
    pub(crate) adapter_id: &'static str,
    pub(crate) language: &'static str,
    pub(crate) validation_kind: &'static str,
    pub(crate) executable_name: &'static str,
    pub(crate) env_override: &'static str,
}

const PYRIGHT: ValidationAdapterMeta = ValidationAdapterMeta {
    adapter_id: "pyright",
    language: "python",
    validation_kind: "typecheck",
    executable_name: "pyright",
    env_override: "WEBCODEX_PYRIGHT",
};

const ADAPTERS: &[ValidationAdapterMeta] = &[PYRIGHT];

pub(crate) fn lookup_adapter(adapter_id: &str) -> Option<&'static ValidationAdapterMeta> {
    ADAPTERS.iter().find(|meta| meta.adapter_id == adapter_id)
}

#[cfg(test)]
pub(crate) fn registered_adapter_ids() -> Vec<&'static str> {
    ADAPTERS.iter().map(|meta| meta.adapter_id).collect()
}

#[cfg(test)]
pub(crate) fn adapter_metadata(adapter_id: &str) -> Option<&'static ValidationAdapterMeta> {
    lookup_adapter(adapter_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyright_is_registered() {
        let meta = lookup_adapter("pyright").expect("pyright");
        assert_eq!(meta.language, "python");
        assert_eq!(meta.validation_kind, "typecheck");
        assert_eq!(meta.executable_name, "pyright");
        assert!(registered_adapter_ids().contains(&"pyright"));
        assert!(lookup_adapter("does-not-exist").is_none());
    }
}
