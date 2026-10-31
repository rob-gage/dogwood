// Copyright Rob Gage 2026

use super::{MaterialReactionProduct, MaterialReactant};

/// A reaction between one or two materials when its conditions hold
pub struct MaterialReaction {
    /// One or two materials required for the reaction
    pub reactants: Vec<MaterialReactant>,
    /// Zero to three products made by each reaction event
    pub products: Vec<MaterialReactionProduct>,
    /// Reaction events per second for each contact while requirements hold
    pub rate: f32,
    /// Inclusive minimum mean reactant temperature in kelvin
    pub temperature_minimum: f32,
    /// Inclusive maximum mean reactant temperature in kelvin
    pub temperature_maximum: f32,
    /// Inclusive minimum reactant-cell pressure in atmospheres
    pub pressure_minimum: f32,
    /// Inclusive maximum reactant-cell pressure in atmospheres
    pub pressure_maximum: f32,
    /// Heat given off per event; negative values take heat in
    pub heat: f32,
    /// Atmospheres added to the reaction cell per event
    pub pressure: f32,
}

impl MaterialReaction {

    /// Creates a reaction with default rate and unbounded temperature and pressure ranges
    pub fn new(
        reactants: Vec<MaterialReactant>,
        products: Vec<MaterialReactionProduct>,
    ) -> Self {
        Self {
            reactants,
            products,
            rate: 1.0,
            temperature_minimum: 0.0,
            temperature_maximum: f32::INFINITY,
            pressure_minimum: 0.0,
            pressure_maximum: f32::INFINITY,
            heat: 0.0,
            pressure: 0.0,
        }
    }

    /// Sets the reaction event rate
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate;
        self
    }

    /// Sets the inclusive reactant temperature range in kelvin
    pub fn with_temperature_range(mut self, minimum: f32, maximum: f32) -> Self {
        self.temperature_minimum = minimum;
        self.temperature_maximum = maximum;
        self
    }

    /// Sets the inclusive reactant pressure range in atmospheres
    pub fn with_pressure_range(mut self, minimum: f32, maximum: f32) -> Self {
        self.pressure_minimum = minimum;
        self.pressure_maximum = maximum;
        self
    }

    /// Sets the heat given off per event
    pub fn with_heat(mut self, heat: f32) -> Self {
        self.heat = heat;
        self
    }

    /// Sets the atmospheres added to the reaction cell per event
    pub fn with_pressure(mut self, pressure: f32) -> Self {
        self.pressure = pressure;
        self
    }

}
