// Copyright Rob Gage 2026

use super::MaterialReactionProduct;
use super::MaterialReactionReactant;

/// Declarative, form-independent reaction rule. A reaction is deliberately
/// limited to two inputs and two outputs so its compiled Accelerator representation is
/// fixed stride.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReaction {
    /// Up to two materials or tags consumed by this reaction.
    pub reactants: [Option<MaterialReactionReactant>; 2],
    /// Up to two materials produced by this reaction.
    pub products: [Option<MaterialReactionProduct>; 2],
    /// Optional lower temperature bound for reaction eligibility.
    pub minimum_temperature: Option<f32>,
    /// Optional upper temperature bound for reaction eligibility.
    pub maximum_temperature: Option<f32>,
    /// Optional lower pressure bound for reaction eligibility.
    pub minimum_pressure: Option<f32>,
    /// Optional upper pressure bound for reaction eligibility.
    pub maximum_pressure: Option<f32>,
    /// Optional lower implicit-air concentration bound for reaction eligibility.
    pub minimum_air: Option<f32>,
    /// Optional upper implicit-air concentration bound for reaction eligibility.
    pub maximum_air: Option<f32>,
    /// Maximum reaction extent that may be applied during one simulation tick.
    pub maximum_extent_per_tick: f32,
    /// Thermal energy released or consumed by one unit of reaction extent.
    pub thermal_energy: f32,
    /// Pressure contribution produced by one unit of reaction extent.
    pub pressure_output: f32,
    /// Arbitration priority used when eligible reactions contend for matter.
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
