// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// Declarative transition metadata; no phase behavior is evaluated here yet
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialThermalTransition {
    /// Temperature at which this transition becomes eligible.
    pub threshold_temperature: f32,
    /// Material produced by the transition.
    pub target: MaterialIdentifier,
    /// Fraction of the source material transferred to the target.
    pub yield_rate: f32,
    /// Energy consumed or released per unit of transitioned material.
    pub latent_energy: f32,
}
