// Copyright Rob Gage 2026

use super::MaterialReference;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReactionReactant {
    pub selector: MaterialReference,
    pub amount: f32,
}
