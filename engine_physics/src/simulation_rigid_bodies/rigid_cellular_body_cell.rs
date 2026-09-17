// Copyright Rob Gage 2026

use crate::{materials::MaterialIdentifier, tiles::CellularAppearance};

/// One physical rigid cell with body-independent state identity
#[derive(Clone, Copy)]
pub(crate) struct RigidCellularBodyCell {
    pub(crate) local: [i32; 2],
    pub(crate) material: MaterialIdentifier,
    pub(crate) appearance: CellularAppearance,
    pub(crate) state_slot: u32,
    pub(crate) state_generation: u32,
}

impl RigidCellularBodyCell {
    #[cfg(test)]
    pub(crate) const fn test_cell(
        local: [i32; 2],
        material: MaterialIdentifier,
        appearance: CellularAppearance,
    ) -> Self {
        Self {
            local,
            material,
            appearance,
            state_slot: 0,
            state_generation: 0,
        }
    }
}
