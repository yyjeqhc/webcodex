use crate::shell_protocol::{AgentProtocolGenerationNumber, AGENT_PROTOCOL_GENERATION_V2};

/// Canonical protocol generation captured once at registration ingress.
/// Transport is an independent ingress fact and project inventory is always paged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AcceptedRunnerProtocol {
    generation: AgentProtocolGenerationNumber,
}

impl AcceptedRunnerProtocol {
    pub(crate) fn try_from_registration(
        generation_number: Option<AgentProtocolGenerationNumber>,
    ) -> Result<Self, String> {
        match generation_number {
            Some(value) if value == AGENT_PROTOCOL_GENERATION_V2 => Ok(Self { generation: value }),
            None => Err("agent_protocol_generation is required".to_string()),
            Some(_) => Err("agent_protocol_generation is unsupported".to_string()),
        }
    }

    pub(crate) const fn generation(self) -> AgentProtocolGenerationNumber {
        self.generation
    }
}
