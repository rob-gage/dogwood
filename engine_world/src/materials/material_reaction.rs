// Copyright Rob Gage 2026

use super::{
    MaterialReactionProduct,
    MaterialReactant,
    MaterialValue,
};

/// A reaction between one or two materials when its conditions hold
pub struct MaterialReaction {
    /// One or two materials required for the reaction
    pub reactants: Vec<MaterialReactant>,
    /// Zero to three products made by each reaction event
    pub products: Vec<MaterialReactionProduct>,
    /// Reaction events per second for each contact while requirements hold
    pub rate: MaterialValue,
    /// Inclusive minimum mean reactant temperature in kelvin
    pub temperature_minimum: MaterialValue,
    /// Inclusive maximum mean reactant temperature in kelvin
    pub temperature_maximum: MaterialValue,
    /// Inclusive minimum reactant-cell pressure in atmospheres
    pub pressure_minimum: MaterialValue,
    /// Inclusive maximum reactant-cell pressure in atmospheres
    pub pressure_maximum: MaterialValue,
    /// Heat given off per event; negative values take heat in
    pub heat: MaterialValue,
    /// Atmospheres added to the reaction cell per event
    pub pressure: MaterialValue,
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
            rate: MaterialValue::Constant(1.0),
            temperature_minimum: MaterialValue::Constant(0.0),
            temperature_maximum: MaterialValue::Constant(f32::INFINITY),
            pressure_minimum: MaterialValue::Constant(0.0),
            pressure_maximum: MaterialValue::Constant(f32::INFINITY),
            heat: MaterialValue::Constant(0.0),
            pressure: MaterialValue::Constant(0.0),
        }
    }

    /// Sets the reaction event rate
    pub fn with_rate(mut self, rate: impl Into<MaterialValue>) -> Self {
        self.rate = rate.into();
        self
    }

    /// Sets the inclusive reactant temperature range in kelvin
    pub fn with_temperature_range(
        mut self,
        minimum: impl Into<MaterialValue>,
        maximum: impl Into<MaterialValue>,
    ) -> Self {
        self.temperature_minimum = minimum.into();
        self.temperature_maximum = maximum.into();
        self
    }

    /// Sets the inclusive reactant pressure range in atmospheres
    pub fn with_pressure_range(
        mut self,
        minimum: impl Into<MaterialValue>,
        maximum: impl Into<MaterialValue>,
    ) -> Self {
        self.pressure_minimum = minimum.into();
        self.pressure_maximum = maximum.into();
        self
    }

    /// Sets the heat given off per event
    pub fn with_heat(mut self, heat: impl Into<MaterialValue>) -> Self {
        self.heat = heat.into();
        self
    }

    /// Sets the atmospheres added to the reaction cell per event
    pub fn with_pressure(mut self, pressure: impl Into<MaterialValue>) -> Self {
        self.pressure = pressure.into();
        self
    }

}
