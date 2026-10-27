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

/// Inserted and replaced cells, materials, colors, added pressures, temperatures, and added
/// temperatures of one tile's merged edits
pub type EditedCellularTile =
    (u64, u64, [MaterialIdentifier; 64], [u32; 64], [f32; 64], [f32; 64], [f32; 64]);

/// Requested edits awaiting their write into the GPU-resident cellular tiles
pub struct EditStreamingState {
    /// Edits requested since the last stream in request order
    edits: Vec<Edit>,
    /// GPU writing of edited cells, absent until edits first stream
    edit_cellular_tile_pass: Option<EditCellularTilePass>,
}

impl EditStreamingState {

    /// Creates edit streaming without requested edits
    pub fn new() -> Self {
        Self { edits: Vec::new(), edit_cellular_tile_pass: None }
    }

    /// Requests an edit of resident cells, written before the next stream moves resident tiles,
    /// giving each filled cell its material's color tinted by the edit's color and, without a
    /// temperature, its material's edit temperature or, when emptied, the ambient gas temperature,
    /// and returns the queued edit
    pub fn request_edit(
        &mut self,
        mut edit: Edit,
        material_table: &MaterialTable,
        active_region: Option<CellularTileRegion>,
        ambient_gas_temperature: f32,
    ) -> Result<&Edit, EditError> {
        // the whole edit is rejected unless every tile and material can be written
        let mut cellular_tiles: HashSet<CellularTileCoordinates> = self.edits.iter()
            .flat_map(|queued_edit| queued_edit.cellular_tiles.keys()
                .chain(queued_edit.pressures.keys())
                .chain(queued_edit.temperatures.keys()).copied())
            .collect();
        cellular_tiles.extend(edit.cellular_tiles.keys().copied());
        cellular_tiles.extend(edit.pressures.keys().copied());
        cellular_tiles.extend(edit.temperatures.keys().copied());
        if cellular_tiles.len() > EDIT_CELLULAR_TILE_COUNT_MAXIMUM {
            return Err(EditError::CapacityReached);
        }
        for (coordinates, (cells, material_identifiers, _, temperatures)) in &edit.cellular_tiles {
            if !active_region.is_some_and(|region| region.contains(*coordinates)) {
                return Err(EditError::CellularTileInactive { coordinates: *coordinates });
            }
            for (cell_index, material_identifier) in material_identifiers.iter().enumerate() {
                if cells & 1 << cell_index == 0 { continue; }
                let temperature: f32 = temperatures[cell_index];
                if !temperature.is_nan() && (!temperature.is_finite() || temperature < 0.0) {
                    return Err(EditError::InvalidTemperature);
                }
                if *material_identifier == MaterialIdentifier::EMPTY { continue; }
                match material_table.get(*material_identifier).map(|material| material.form()) {
                    None => return Err(EditError::MaterialUnregistered {
                        material_identifier: *material_identifier,
                    }),
                    Some(_) => {}
                }
            }
        }
        // like filled tiles, a pressure or temperature tile outside the resident region rejects the
        // whole edit
        for coordinates in edit.pressures.keys().chain(edit.temperatures.keys()) {
            if !active_region.is_some_and(|region| region.contains(*coordinates)) {
                return Err(EditError::CellularTileInactive { coordinates: *coordinates });
            }
        }
        for (coordinates, (cells, material_identifiers, colors, temperatures)) in
            &mut edit.cellular_tiles
        {
            for (cell_index, (material_identifier, (color, temperature))) in material_identifiers
                .iter()
                .zip(colors.iter_mut().zip(temperatures.iter_mut()))
                .enumerate()
            {
                if *cells & 1 << cell_index == 0 { continue; }
                if temperature.is_nan() {
                    *temperature = material_table.edit_temperature(*material_identifier)
                        .unwrap_or(ambient_gas_temperature);
                }
                if *material_identifier == MaterialIdentifier::EMPTY { continue; }
                let cell: CellCoordinates = CellCoordinates {
                    x: coordinates.x * CELLULAR_TILE_SIZE + cell_index as i64 % CELLULAR_TILE_SIZE,
                    y: coordinates.y * CELLULAR_TILE_SIZE + cell_index as i64 / CELLULAR_TILE_SIZE,
                };
                *color = material_table.cellular_color(*material_identifier, cell, *color);
            }
        }
        self.edits.push(edit);
        Ok(self.edits.last().expect("the edit was just queued"))
    }

    /// Writes requested edits into the resident tiles in request order before the ring moves,
    /// emptied cells holding phantom air of an ambient amount
    pub fn stream_edits(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        cellular_tile_streaming_state: &CellularTileStreamingState,
        actor_registry: &ActorRegistry,
        collision: Option<&mut Collision>,
        ambient_gas_amount: u32,
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
        let mut edited_cellular_tiles: BTreeMap<CellularTileCoordinates, EditedCellularTile> =
            BTreeMap::new();
        let empty_cellular_tile: EditedCellularTile =
            (0, 0, [MaterialIdentifier::EMPTY; 64], [0; 64], [0.0; 64], [0.0; 64], [0.0; 64]);
        for edit in self.edits.drain(..) {
            for (coordinates, (cells, material_identifiers, colors, temperatures)) in
                edit.cellular_tiles
            {
                let (inserted_cells, replaced_cells, edited_materials, edited_colors, _,
                    edited_temperatures, _): &mut EditedCellularTile =
                    edited_cellular_tiles.entry(coordinates).or_insert(empty_cellular_tile);
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
                    // a fluid fill empties the cell for its particle and keeps its temperature
                    if material_identifiers[cell_index].form() == MaterialForm::Fluid {
                        edited_materials[cell_index] = MaterialIdentifier::EMPTY;
                        edited_colors[cell_index] = 0;
                        edited_temperatures[cell_index] = -1.0;
                    } else {
                        edited_materials[cell_index] = material_identifiers[cell_index];
                        edited_colors[cell_index] = colors[cell_index];
                        edited_temperatures[cell_index] = temperatures[cell_index];
                    }
                }
            }
            for (coordinates, pressures) in edit.pressures {
                let (_, _, _, _, edited_pressures, _, _): &mut EditedCellularTile =
                    edited_cellular_tiles.entry(coordinates).or_insert(empty_cellular_tile);
                for cell_index in 0..64 { edited_pressures[cell_index] += pressures[cell_index]; }
            }
            for (coordinates, temperatures) in edit.temperatures {
                let (_, _, _, _, _, _, temperature_changes): &mut EditedCellularTile =
                    edited_cellular_tiles.entry(coordinates).or_insert(empty_cellular_tile);
                for cell_index in 0..64 {
                    temperature_changes[cell_index] += temperatures[cell_index];
                }
            }
        }
        // validated tiles stay resident because only streaming after this write moves the ring
        let edit_cellular_tiles: Vec<(CellularTileIndex, EditedCellularTile)> =
            edited_cellular_tiles.into_iter()
                .filter_map(|(coordinates, edited_cellular_tile)| Some((
                    cellular_tile_streaming_state.cellular_tile_index(coordinates)?,
                    edited_cellular_tile,
                )))
                .collect();
        if edit_cellular_tiles.is_empty() { return Ok(()); }
        self.edit_cellular_tile_pass
            .get_or_insert_with(|| EditCellularTilePass::new(accelerator, cellular_tile_buffers))
            .write_cellular_tiles(accelerator, &edit_cellular_tiles, ambient_gas_amount);
        // collisions read the edited cells in the next simulation even when the ring is unmoved
        if let Some(collision) = collision { collision.request_cellular_tile_read(); }
        Ok(())
    }

}
