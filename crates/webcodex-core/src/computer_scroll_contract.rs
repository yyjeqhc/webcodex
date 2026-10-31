//! Closed, single-increment Computer container scrolling contract.

use crate::runner_protocol::RunnerCapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const COMPUTER_SCROLL_CAPABILITIES: &[RunnerCapabilityId] = &[
    RunnerCapabilityId::ComputerControl,
    RunnerCapabilityId::ComputerScroll,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ComputerScrollDirection {
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    pub fn increasing(self) -> bool {
        matches!(self, Self::Down | Self::Right)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerScrollAmount {
    #[default]
    Small,
    Large,
}

/// Keep null as an explicit schema branch through Host/OpenAPI normalization.
pub fn computer_scroll_amount_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let amount = schemars::schema_for!(ComputerScrollAmount);
    schemars::json_schema!({"anyOf": [amount, {"type": "null"}]})
}

/// Canonical Runner payload always names the effective amount.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputerScrollPayload {
    pub surface_id: String,
    pub element_id: String,
    pub direction: ComputerScrollDirection,
    pub amount: ComputerScrollAmount,
}

impl ComputerScrollPayload {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.surface_id.is_empty()
            || self.surface_id.len() > 128
            || !self.element_id.starts_with("element_")
            || self.element_id.len() <= 8
            || self.element_id.len() > 128
        {
            return Err("invalid computer scroll target identity");
        }
        Ok(())
    }
}

/// Native positions are finite percentages in 0..100. A null axis is
/// confirmed non-scrollable by the provider.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComputerScrollState {
    pub horizontal_percent: Option<f64>,
    pub vertical_percent: Option<f64>,
}

impl ComputerScrollState {
    pub fn valid(&self) -> bool {
        [self.horizontal_percent, self.vertical_percent]
            .into_iter()
            .all(|position| position.is_none_or(valid_scroll_percent))
    }
}

pub fn valid_scroll_percent(position: f64) -> bool {
    position.is_finite() && (0.0..=100.0).contains(&position)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computer_scroll_contract_is_closed_and_positions_are_bounded() {
        let valid = serde_json::json!({"surface_id":"s", "element_id":"e", "direction":"down", "amount":"small"});
        assert!(serde_json::from_value::<ComputerScrollPayload>(valid.clone()).is_ok());
        for (field, value) in [
            ("direction", "diagonal"),
            ("amount", "page"),
            ("steps", "2"),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value.into();
            assert!(serde_json::from_value::<ComputerScrollPayload>(invalid).is_err());
        }
        for position in [0.0, 1.25, 100.0] {
            assert!(valid_scroll_percent(position));
        }
        for position in [-1.0, 100.1, f64::NAN, f64::INFINITY] {
            assert!(!valid_scroll_percent(position));
        }
    }
}
