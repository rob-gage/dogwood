// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// Declarative transition metadata; no phase behavior is evaluated here yet
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialThermalTransition {
    pub threshold_temperature: f32,
    pub target: MaterialIdentifier,
    pub yield_rate: f32,
    pub latent_energy: f32,
}
