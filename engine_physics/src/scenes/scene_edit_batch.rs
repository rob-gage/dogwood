// Copyright Rob Gage 2026

use super::{SceneEdit, SceneEditCellPlacement};
use crate::{
    materials::MaterialIdentifier,
    tiles::{CellCoordinates, CellularAppearance},
};

/// Pending requested material mutations of a `Scene`
pub struct SceneEditBatch {
    edits: Vec<SceneEdit>,
}

impl SceneEditBatch {
    /// Creates an empty `SceneEditBatch`
    pub const fn new() -> Self {
        Self { edits: Vec::new() }
    }

    /// Adds a request to place a material in cells
    pub fn place_material(
        &mut self,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        cells: Vec<CellCoordinates>,
    ) {
        self.place_cells(
            cells
                .into_iter()
                .map(|coordinates| SceneEditCellPlacement {
                    coordinates,
                    material_identifier,
                    appearance,
                })
                .collect(),
        );
    }

    /// Adds a request to place explicit state in cells
    pub fn place_cells(&mut self, cells: Vec<SceneEditCellPlacement>) {
        self.edits.push(SceneEdit::PlaceCells { cells });
    }

    /// Adds one atomic authored rigid cellular body.
    pub fn place_rigid_body(&mut self, cells: Vec<SceneEditCellPlacement>) {
        self.edits.push(SceneEdit::PlaceRigidBody { cells });
    }

    /// Adds a request to erase material from cells
    pub fn erase(&mut self, cells: Vec<CellCoordinates>) {
        self.edits.push(SceneEdit::Erase { cells });
    }

    /// Drains the pending edits
    pub fn drain(&mut self) -> impl Iterator<Item = SceneEdit> + '_ {
        self.edits.drain(..)
    }

    pub const fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Appends requests without changing their producer ordering.
    pub fn append(&mut self, mut other: Self) {
        self.edits.append(&mut other.edits);
    }
}
