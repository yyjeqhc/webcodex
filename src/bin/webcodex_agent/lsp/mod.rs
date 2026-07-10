mod navigation;
mod position;
mod protocol;
mod supervisor;

pub(crate) use navigation::{handle_lsp_request, is_lsp_request_kind};
pub(crate) use supervisor::LspSupervisor;

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod navigation_tests;
