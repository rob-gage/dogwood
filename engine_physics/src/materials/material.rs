// Copyright Rob Gage 2026

use super::{
    MaterialAppearance,
    MaterialForm,
};

/// A material
pub struct Material {
    /// The appearance of this `Material`
    appearance: MaterialAppearance,
    /// The form of this `Material`
    form: MaterialForm,
}