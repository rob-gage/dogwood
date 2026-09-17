// Copyright Rob Gage 2026

use super::MaterialReactionProduct;
use super::MaterialReactionReactant;

/// Declarative, form-independent reaction rule. A reaction is deliberately
/// limited to two inputs and two outputs so its compiled GPU representation is
/// fixed stride.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReaction {
    pub reactants: [Option<MaterialReactionReactant>; 2],
    pub products: [Option<MaterialReactionProduct>; 2],
    pub minimum_temperature: Option<f32>,
    pub maximum_temperature: Option<f32>,
    pub minimum_pressure: Option<f32>,
    pub maximum_pressure: Option<f32>,
    pub minimum_air: Option<f32>,
    pub maximum_air: Option<f32>,
    pub maximum_extent_per_tick: f32,
    pub thermal_energy: f32,
    pub pressure_output: f32,
    pub priority: i32,
}

impl Default for MaterialReaction {
    fn default() -> Self {
        Self {
            reactants: [None, None],
            products: [None, None],
            minimum_temperature: None,
            maximum_temperature: None,
            minimum_pressure: None,
            maximum_pressure: None,
            minimum_air: None,
            maximum_air: None,
            maximum_extent_per_tick: 1.0,
            thermal_energy: 0.0,
            pressure_output: 0.0,
            priority: 0,
        }
    }
}
