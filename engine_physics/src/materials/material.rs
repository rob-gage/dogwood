// Copyright Rob Gage 2026

use super::{
    MaterialAppearance,
    MaterialForm,
};

/// A material
pub struct Material {
    /// The name of this `Material`
    pub name: &'static str,
    /// The appearance of this `Material`
    pub appearance: MaterialAppearance,
    /// The form of this `Material`
    pub form: MaterialForm,
}