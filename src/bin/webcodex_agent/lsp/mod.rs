#![cfg_attr(not(test), allow(dead_code))]

mod protocol;
mod supervisor;

pub(crate) use supervisor::LspSupervisor;
