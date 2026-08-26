use crate::shell_protocol::{
    normalize_agent_protocol_semantics, AgentProjectInventoryStrategy, AgentProtocolCompatibility,
    AgentProtocolGenerationNumber, AgentProtocolSemantics, AGENT_PROTOCOL_GENERATION_LEGACY_V1,
    AGENT_PROTOCOL_GENERATION_V2,
};

/// Closed Server-internal generation identity for an accepted Runner.
/// Unsupported wire numbers never enter this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerProtocolGeneration {
    LegacyV1,
    V2,
}

/// Supported protocol semantics captured once at registration ingress.
///
/// The legacy label remains separately available for diagnostics, while this
/// value is the only accepted generation/inventory semantic state stored in a
/// successful Runner record. Transport remains an independent ingress fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AcceptedRunnerProtocol {
    generation: RunnerProtocolGeneration,
    project_inventory: AgentProjectInventoryStrategy,
}

impl AcceptedRunnerProtocol {
    pub(crate) fn try_from_registration(
        agent_protocol_version: &str,
        generation_number: Option<AgentProtocolGenerationNumber>,
    ) -> Result<Self, String> {
        let legacy = normalize_agent_protocol_semantics(agent_protocol_version);
        if !legacy.compatibility.is_supported() {
            return Err("agent_protocol_version is unsupported".to_string());
        }
        let generation = match generation_number {
            None => RunnerProtocolGeneration::LegacyV1,
            Some(value) if value == AGENT_PROTOCOL_GENERATION_LEGACY_V1 => {
                RunnerProtocolGeneration::LegacyV1
            }
            Some(value) if value == AGENT_PROTOCOL_GENERATION_V2 => RunnerProtocolGeneration::V2,
            Some(_) => return Err("agent_protocol_generation is unsupported".to_string()),
        };
        Ok(Self {
            generation,
            project_inventory: legacy.project_inventory,
        })
    }

    pub(crate) const fn generation(self) -> RunnerProtocolGeneration {
        self.generation
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
