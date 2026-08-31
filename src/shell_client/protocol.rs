use crate::shell_protocol::{
    normalize_agent_protocol_semantics, AgentProjectInventoryStrategy, AgentProtocolCompatibility,
    AgentProtocolGenerationNumber, AgentProtocolSemantics, AGENT_PROTOCOL_GENERATION_V2,
};

/// Supported protocol semantics captured once at registration ingress.
///
/// Protocol generation 2 is the only accepted generation. Historical
/// `polling-v1/v2`, `websocket-v1/v2`, and `quic-v1/v2` labels remain a separate
/// project-inventory dimension and do not select an older protocol generation.
/// Transport remains an independent ingress fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AcceptedRunnerProtocol {
    project_inventory: AgentProjectInventoryStrategy,
}

impl AcceptedRunnerProtocol {
    pub(crate) fn try_from_registration(
        agent_protocol_version: &str,
        generation_number: Option<AgentProtocolGenerationNumber>,
    ) -> Result<Self, String> {
        let label_semantics = normalize_agent_protocol_semantics(agent_protocol_version);
        if !label_semantics.compatibility.is_supported() {
            return Err("agent_protocol_version is unsupported".to_string());
        }
        match generation_number {
            Some(value) if value == AGENT_PROTOCOL_GENERATION_V2 => {}
            None => return Err("agent_protocol_generation is required".to_string()),
            Some(_) => return Err("agent_protocol_generation is unsupported".to_string()),
        }
        Ok(Self {
            project_inventory: label_semantics.project_inventory,
        })
    }

    pub(crate) const fn project_inventory(self) -> AgentProjectInventoryStrategy {
        self.project_inventory
    }

    /// Compatibility projection for existing internal/public diagnostic views.
    /// Every successful accepted protocol currently uses the same V1 wire grammar;
    /// generation and inventory remain separate semantic dimensions.
    pub(crate) const fn compatibility_semantics(self) -> AgentProtocolSemantics {
        AgentProtocolSemantics {
            compatibility: AgentProtocolCompatibility::V1,
            project_inventory: self.project_inventory,
        }
    }
}
