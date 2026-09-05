mod supervisor;

#[cfg(test)]
mod tests;

pub(crate) use supervisor::MachineEventReceiver;
pub use supervisor::{ProcessKind, ProcessPhase, ProcessSnapshot, ProcessSupervisor};
