//! Closed vocabulary for existing lossless input normalizations.
//!
//! These labels describe observed normalization, never admission or retry authority.
//! Keep the wire spellings and model hints stable; callers own normalization rules.
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolInputNormalizationCode {
    ArgvToArgs,
    RunProcessShCToRunShell,
    RunProcessBashCToRunShell,
    RunProcessBashLcToLoginRunShell,
}

impl ToolInputNormalizationCode {
    pub const fn all() -> &'static [Self] {
        &[
            Self::ArgvToArgs,
            Self::RunProcessShCToRunShell,
            Self::RunProcessBashCToRunShell,
            Self::RunProcessBashLcToLoginRunShell,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArgvToArgs => "argv_to_args",
            Self::RunProcessShCToRunShell => "run_process_sh_c_to_run_shell",
            Self::RunProcessBashCToRunShell => "run_process_bash_c_to_run_shell",
            Self::RunProcessBashLcToLoginRunShell => "run_process_bash_lc_to_login_run_shell",
        }
    }

    /// Exact closed lookup. No trimming, aliases, or arbitrary diagnostic strings.
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|code| code.as_str() == value)
    }

    pub const fn model_hint(self) -> &'static str {
        match self {
            Self::ArgvToArgs => "normalized argv→args",
            Self::RunProcessShCToRunShell => "normalized run_process sh -c → run_shell",
            Self::RunProcessBashCToRunShell => "normalized run_process bash -c → run_shell",
            Self::RunProcessBashLcToLoginRunShell => {
                "normalized run_process bash -lc → run_shell(login=true)"
            }
        }
    }
}

impl Serialize for ToolInputNormalizationCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
