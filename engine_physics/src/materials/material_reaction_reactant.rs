// Copyright Rob Gage 2026

use super::MaterialReference;

/// One material or tag consumed by a reaction at a given extent.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialReactionReactant {
    /// Material identifier or tag that selects the consumed material.
    pub selector: MaterialReference,
    /// Amount required per unit of reaction extent.
    pub amount: f32,
}
