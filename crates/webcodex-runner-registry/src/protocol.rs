use webcodex_core::runner_protocol::{
    RunnerProtocolGenerationNumber, RUNNER_PROTOCOL_GENERATION_V2,
};

/// Canonical protocol generation captured once at registration ingress.
/// Transport is an independent ingress fact and project inventory is always paged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AcceptedRunnerProtocol {
    generation: RunnerProtocolGenerationNumber,
}

impl AcceptedRunnerProtocol {
    pub(crate) fn try_from_registration(
        generation_number: RunnerProtocolGenerationNumber,
    ) -> Result<Self, String> {
        if generation_number == RUNNER_PROTOCOL_GENERATION_V2 {
            Ok(Self {
                generation: generation_number,
            })
        } else {
            Err("agent_protocol_generation is unsupported".to_string())
        }
    }

    pub(crate) const fn generation(self) -> RunnerProtocolGenerationNumber {
        self.generation
    }
}
