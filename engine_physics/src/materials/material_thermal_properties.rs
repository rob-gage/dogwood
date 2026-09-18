// Copyright Rob Gage 2026

use super::MaterialThermalTransition;

/// Common immutable thermal metadata, independent of material form.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialThermalProperties {
    /// Thermal conductivity used when transferring heat between neighboring cells.
    pub conductivity: f32,
    /// Energy required to change the temperature of one unit of material.
    pub specific_heat_capacity: f32,
    /// Temperature assigned when a material is first initialized, if authored.
    pub default_temperature: Option<f32>,
    /// Transition applied when the material becomes colder than its threshold, if any.
    pub cold_transition: Option<MaterialThermalTransition>,
    /// Transition applied when the material becomes hotter than its threshold, if any.
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
