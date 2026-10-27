// Copyright Rob Gage 2026

use super::{MaterialTemperatureColor, MaterialTransition};

/// Thermal properties shared by every material form
pub struct MaterialPropertiesThermal {
    /// Heat per unit of amount needed to warm the material by one kelvin; gas amount is in
    /// atmospheres of one cell
    pub heat_capacity: f32,
    /// Heat per second per kelvin of temperature difference passing through each face between
    /// two cells of the material; zero insulates
    pub conduction: f32,
    /// Temperature in kelvin of the material when edits create it without another temperature
    pub edit_temperature: f32,
    /// Transition of the material when it reaches its hot transition temperature, or `None` to stay
    pub hot_transition: Option<MaterialTransition>,
    /// Transition of the material at its cold transition temperature, or `None` to stay
    pub cold_transition: Option<MaterialTransition>,
    /// Visual-only color and radiance transformations selected by temperature
    pub temperature_colors: Vec<MaterialTemperatureColor>,
}

impl Default for MaterialPropertiesThermal {

    /// Returns default thermal properties
    fn default() -> Self {
        Self {
            heat_capacity: 1.0,
            conduction: 0.0,
            edit_temperature: 293.0,
            hot_transition: None,
            cold_transition: None,
            temperature_colors: Vec::new(),
        }
    }

}
