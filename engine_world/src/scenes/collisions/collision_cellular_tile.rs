// Copyright Rob Gage 2026

use crate::materials::{
    MaterialForm,
    MaterialIdentifier,
};

/// Solid static or settled dynamic cells of one cellular tile, as bit `y * 8 + x` per cell
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct CollisionCellularTile(u64);

impl CollisionCellularTile {

    /// Creates static occupancy of one cellular tile from its row-major material identifiers
    pub fn new(material_identifiers: &[MaterialIdentifier; 64]) -> Self {
        let mut occupancy: u64 = 0;
        for (cell_index, material_identifier) in material_identifiers.iter().enumerate() {
            if *material_identifier != MaterialIdentifier::EMPTY
                && material_identifier.form() == MaterialForm::CellularStatic
            {
                occupancy |= 1 << cell_index;
            }
        }
        Self(occupancy)
    }

    /// Returns this occupancy as a `u64`
    pub const fn as_u64(self) -> u64 { self.0 }

    /// Creates an occupancy from a `u64`
    pub const fn from_u64(value: u64) -> Self { Self(value) }

}
