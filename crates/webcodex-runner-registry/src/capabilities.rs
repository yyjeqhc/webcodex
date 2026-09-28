use webcodex_core::runner_protocol::RunnerCapabilities;

/// Server spelling of the core protocol identity; no separate wire mapping.
pub use webcodex_core::runner_protocol::RunnerCapabilityId as RunnerFeature;

pub(crate) fn is_computer(feature: RunnerFeature) -> bool {
    matches!(
        feature,
        RunnerFeature::ComputerObserve
            | RunnerFeature::ComputerApplicationDiscovery
            | RunnerFeature::ComputerApplicationLaunch
            | RunnerFeature::ComputerDisplayObserve
            | RunnerFeature::ComputerPointerControl
            | RunnerFeature::ComputerClipboardRead
            | RunnerFeature::ComputerClipboardWrite
            | RunnerFeature::ComputerSnapshotRegion
            | RunnerFeature::ComputerAccessibilityObserve
            | RunnerFeature::ComputerElementState
            | RunnerFeature::ComputerControl
            | RunnerFeature::ComputerScrollToElement
            | RunnerFeature::ComputerKeyInput
            | RunnerFeature::ComputerWindowActivate
            | RunnerFeature::ComputerTextInput
    )
}

/// Canonical Server-side capability truth for one accepted Runner registration.
///
/// There is intentionally no public mutation API. Every set is rebuilt from the
/// corresponding immutable wire snapshot at registration ingress, so canonical
/// semantics cannot acquire features independently from accepted registration
/// semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerFeatureSet {
    capabilities: RunnerCapabilities,
}

impl RunnerFeatureSet {
    /// Normalize one accepted generation-2 registration into canonical feature truth.
    ///
    /// Every frozen generation-2 baseline capability must remain true in the
    /// explicit bool projection; contradictions reject registration instead of
    /// being silently inferred. RegistrationRequired features are never inferred
    /// from generation.
    pub(crate) fn try_from_registration(capabilities: &RunnerCapabilities) -> Result<Self, String> {
        for feature in RunnerFeature::all().iter().copied() {
            if feature.is_v2_baseline() && !capabilities.supports(feature) {
                return Err(format!(
                    "runner generation baseline capability mismatch: {}",
                    feature.as_wire_name()
                ));
            }
        }
        Ok(Self {
            capabilities: capabilities.clone(),
        })
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub(crate) fn from_wire_for_test(capabilities: &RunnerCapabilities) -> Self {
        Self {
            capabilities: capabilities.clone(),
        }
    }

    pub fn supports(&self, feature: RunnerFeature) -> bool {
        self.capabilities.supports(feature)
    }

    pub fn supports_wire_name(&self, capability: &str) -> bool {
        RunnerFeature::from_wire_name(capability).is_some_and(|feature| self.supports(feature))
    }

    pub fn wire_capabilities(&self) -> &RunnerCapabilities {
        &self.capabilities
    }
}
