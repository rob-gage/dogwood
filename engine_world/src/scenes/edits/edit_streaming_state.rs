// Copyright Rob Gage 2026

use super::{
    edit_cellular_tile_pass::EditCellularTilePass,
    Edit,
    EditError,
    EditMode,
    EDIT_CELLULAR_TILE_COUNT_MAXIMUM,
};
use crate::{
    actors::ActorRegistry,
    materials::{
        MaterialForm,
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileBuffers,
            CellularTileCoordinates,
            CellularTileIndex,
            CellularTileRegion,
            CellularTileStreamingState,
            CELLULAR_TILE_SIZE,
        },
        collisions::Collision,
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::{
    BTreeMap,
    HashMap,
    HashSet,
};

/// Requested edits awaiting their write into the GPU-resident cellular tiles
pub struct EditStreamingState {
    /// Edits requested since the last stream in request order
    edits: Vec<Edit>,
    /// Cellular tiles held by the requested edits
    cellular_tile_count: usize,
    /// GPU writing of edited cells, absent until edits first stream
    edit_cellular_tile_pass: Option<EditCellularTilePass>,
}

impl EditStreamingState {

    /// Creates edit streaming without requested edits
    pub fn new() -> Self {
        Self { edits: Vec::new(), cellular_tile_count: 0, edit_cellular_tile_pass: None }
    }

    /// Requests an edit of resident cells, written before the next stream moves resident tiles,
    /// giving each filled cell its material's color tinted by the edit's color
    pub fn request_edit(
        &mut self,
        mut edit: Edit,
        material_table: &MaterialTable,
        active_region: Option<CellularTileRegion>,
    ) -> Result<(), EditError> {
        // the whole edit is rejected unless every tile and material can be written
        if self.cellular_tile_count + edit.cellular_tiles.len() > EDIT_CELLULAR_TILE_COUNT_MAXIMUM {
            return Err(EditError::CapacityReached);
        }
        for (coordinates, (cells, material_identifiers, _)) in &edit.cellular_tiles {
            if !active_region.is_some_and(|region| region.contains(*coordinates)) {
                return Err(EditError::CellularTileInactive { coordinates: *coordinates });
            }
            for (cell_index, material_identifier) in material_identifiers.iter().enumerate() {
                if cells & 1 << cell_index == 0 || *material_identifier == MaterialIdentifier::EMPTY
                { continue; }
                match material_table.get(*material_identifier).map(|material| material.form()) {
                    None => return Err(EditError::MaterialUnregistered {
                        material_identifier: *material_identifier,
                    }),
                    Some(MaterialForm::CellularStatic | MaterialForm::CellularDynamic) => {}
                    Some(MaterialForm::Fluid | MaterialForm::Gas) => {
                        return Err(EditError::MaterialFormUnsupported {
                            material_identifier: *material_identifier,
                        });
                    }
                }
            }
        }
        for (coordinates, (cells, material_identifiers, colors)) in &mut edit.cellular_tiles {
            for (cell_index, (material_identifier, color)) in
                material_identifiers.iter().zip(colors.iter_mut()).enumerate()
            {
                if *cells & 1 << cell_index == 0
                    || *material_identifier == MaterialIdentifier::EMPTY
                { continue; }
                let cell: CellCoordinates = CellCoordinates {
                    x: coordinates.x * CELLULAR_TILE_SIZE + cell_index as i64 % CELLULAR_TILE_SIZE,
                    y: coordinates.y * CELLULAR_TILE_SIZE + cell_index as i64 / CELLULAR_TILE_SIZE,
                };
                *color = material_table.cellular_color(*material_identifier, cell, *color);
            }
        }
        self.cellular_tile_count += edit.cellular_tiles.len();
        self.edits.push(edit);
        Ok(())
    }

    /// Writes requested edits into the resident tiles in request order before the ring moves
    pub fn stream_edits(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        cellular_tile_streaming_state: &CellularTileStreamingState,
        actor_registry: &ActorRegistry,
        collision: Option<&mut Collision>,
    ) -> Result<(), SceneError> {
        if self.edits.is_empty() { return Ok(()); }
        // insertion skips cells overlapped by colliders in the edit collision group
        let inserted_coordinates: HashSet<CellularTileCoordinates> = self.edits.iter()
            .filter(|edit| edit.mode == EditMode::Insert)
            .flat_map(|edit| edit.cellular_tiles.keys().copied())
            .collect();
        let intersections: HashMap<CellularTileCoordinates, u64> =
            Collision::intersect_collisions(actor_registry, &inserted_coordinates)?;
        // merge edits per cell so one dispatch matches applying them one after another: a
        // replacement always writes, and an insertion writes only if no earlier edit filled the
        // cell, becoming a replacement when an earlier replacement emptied it
        let mut edited_cellular_tiles: BTreeMap<
            CellularTileCoordinates,
            (u64, u64, [MaterialIdentifier; 64], [u32; 64]),
        > = BTreeMap::new();
        for edit in self.edits.drain(..) {
            for (coordinates, (cells, material_identifiers, colors)) in edit.cellular_tiles {
                let (inserted_cells, replaced_cells, edited_materials, edited_colors):
                    &mut (u64, u64, [MaterialIdentifier; 64], [u32; 64]) = edited_cellular_tiles
                        .entry(coordinates)
                        .or_insert((0, 0, [MaterialIdentifier::EMPTY; 64], [0; 64]));
                let blocked_cells: u64 = intersections.get(&coordinates).copied().unwrap_or(0);
                for cell_index in 0..64 {
                    let cell: u64 = 1 << cell_index;
                    if cells & cell == 0 { continue; }
                    let is_written: bool = match edit.mode {
                        EditMode::Replace => {
                            *inserted_cells &= !cell;
                            *replaced_cells |= cell;
                            true
                        }
                        EditMode::Insert => {
                            let is_writable: bool = material_identifiers[cell_index]
                                != MaterialIdentifier::EMPTY
                                && blocked_cells & cell == 0
                                && (*replaced_cells & cell == 0
                                    || edited_materials[cell_index] == MaterialIdentifier::EMPTY)
                                && *inserted_cells & cell == 0;
                            if is_writable && *replaced_cells & cell == 0 {
                                *inserted_cells |= cell;
                            }
                            is_writable
                        }
                    };
                    if !is_written { continue; }
                    edited_materials[cell_index] = material_identifiers[cell_index];
                    edited_colors[cell_index] = colors[cell_index];
                }
            }
        }
        self.cellular_tile_count = 0;
        // validated tiles stay resident because only streaming after this write moves the ring
        let edit_cellular_tiles: Vec<(
            CellularTileIndex,
            u64,
            u64,
            [MaterialIdentifier; 64],
            [u32; 64],
        )> = edited_cellular_tiles.into_iter()
            .filter(|(_, (inserted_cells, replaced_cells, _, _))| {
                *inserted_cells | *replaced_cells != 0
            })
            .filter_map(|(coordinates, (inserted_cells, replaced_cells, materials, colors))| {
                Some((
                    cellular_tile_streaming_state.cellular_tile_index(coordinates)?,
                    inserted_cells,
                    replaced_cells,
                    materials,
                    colors,
                ))
            })
            .collect();
        if edit_cellular_tiles.is_empty() { return Ok(()); }
        self.edit_cellular_tile_pass
            .get_or_insert_with(|| EditCellularTilePass::new(accelerator, cellular_tile_buffers))
            .write_cellular_tiles(accelerator, &edit_cellular_tiles);
        // collisions read the edited cells in the next simulation even when the ring is unmoved
        if let Some(collision) = collision { collision.request_cellular_tile_read(); }
        Ok(())
    }

}
