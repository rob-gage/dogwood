// Copyright Rob Gage 2026

use super::MaterialIdentifier;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialReactionProduct {
    pub material: MaterialIdentifier,
    pub amount: f32,
}
