// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// Declarative transition metadata; no phase behavior is evaluated here yet.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialThermalTransition {
    pub threshold_temperature: f32,
    pub target: MaterialIdentifier,
    pub yield_rate: f32,
    pub latent_energy: f32,
}

/// Common immutable thermal metadata, independent of material form.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialThermalProperties {
    pub conductivity: f32,
    pub specific_heat_capacity: f32,
    pub default_temperature: Option<f32>,
    pub cold_transition: Option<MaterialThermalTransition>,
    pub hot_transition: Option<MaterialThermalTransition>,
}

impl Default for MaterialThermalProperties {
    fn default() -> Self {
        Self {
            conductivity: 0.0,
            specific_heat_capacity: 1.0,
            default_temperature: None,
            cold_transition: None,
            hot_transition: None,
        }
    }
}
