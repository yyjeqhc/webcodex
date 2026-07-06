use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpsCommonOptions {
    pub(crate) server_url: String,
    pub(crate) env_file: Option<PathBuf>,
    pub(crate) token_file: Option<PathBuf>,
    pub(crate) token: Option<String>,
    pub(crate) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpsSmokePreflightOptions {
    pub(crate) common: OpsCommonOptions,
    pub(crate) project: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpsCommand {
    Status(OpsCommonOptions),
    Agents(OpsCommonOptions),
    Projects(OpsCommonOptions),
    SmokePreflight(OpsSmokePreflightOptions),
}

impl OpsCommand {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Status(_) => "status",
            Self::Agents(_) => "agents",
            Self::Projects(_) => "projects",
            Self::SmokePreflight(_) => "smoke-preflight",
        }
    }
}

pub(crate) async fn run_ops_command(command: OpsCommand) -> Result<String, String> {
    Err(format!(
        "ops {} is not implemented yet; run with --help for usage",
        command.name()
    ))
}
