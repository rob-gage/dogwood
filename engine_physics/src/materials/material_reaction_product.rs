// Copyright Rob Gage 2026

use super::MaterialIdentifier;

/// One material produced by a reaction at a given extent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialReactionProduct {
    /// Material identifier produced by the reaction.
    pub material: MaterialIdentifier,
    /// Amount produced per unit of reaction extent.
    pub amount: f32,
}
