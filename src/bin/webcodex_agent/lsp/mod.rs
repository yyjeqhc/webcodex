mod navigation;
mod position;
mod protocol;
mod supervisor;

pub(crate) use navigation::{handle_lsp_request, is_lsp_request_kind};
pub(crate) use supervisor::LspSupervisor;

#[cfg(test)]
pub(crate) use supervisor::{
    classify_uri_against_project_root, LspCommand, LspError, LspServerKind, LspServerStatus,
    LspSupervisorConfig, PositionEncoding, ProjectUriClassification,
};

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod navigation_tests;
