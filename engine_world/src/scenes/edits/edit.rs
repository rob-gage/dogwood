// Copyright Rob Gage 2026

use crate::{
    materials::{MaterialForm, MaterialIdentifier},
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileCoordinates,
            CELLULAR_TILE_SIZE,
        },
        ScenePosition,
    },
};
use super::{
    EditError,
    EditMode,
    EDIT_CELLULAR_TILE_COUNT_MAXIMUM,
};
use std::collections::BTreeMap;

/// Cells of a scene to fill with materials, grouped by cellular tile
#[derive(Clone, Debug)]
pub struct Edit {
    /// Whether filled cells keep or replace existing matter
    pub mode: EditMode,
    /// Filled cells as bit `y * 8 + x`, material identifiers, packed RGBA colors, radiances, and
    /// temperatures in kelvin of each tile, a NaN temperature taking the material's edit temperature
    pub cellular_tiles: BTreeMap<
        CellularTileCoordinates,
        (u64, [MaterialIdentifier; 64], [u32; 64], [u32; 64], [f32; 64]),
    >,
    /// Nonzero initial fluid velocities in tiles per second, keyed by world cell
    pub fluid_velocities: BTreeMap<CellCoordinates, [f32; 2]>,
    /// Pressure in atmospheres added to each row-major cell of each tile, negative where removed
    pub pressures: BTreeMap<CellularTileCoordinates, [f32; 64]>,
    /// Temperature in kelvin added to everything in each row-major cell of each tile, negative
    /// where removed
    pub temperatures: BTreeMap<CellularTileCoordinates, [f32; 64]>,
    /// Scripted damage load in atmospheres, the greatest supplied, and its total duration in
    /// seconds for each row-major cell of each tile
    pub damages: BTreeMap<CellularTileCoordinates, ([f32; 64], [f32; 64])>,
}

impl Edit {

    /// Creates an edit without cells
    pub fn new(mode: EditMode) -> Self {
        Self {
            mode,
            cellular_tiles: BTreeMap::new(),
            fluid_velocities: BTreeMap::new(),
            pressures: BTreeMap::new(),
            temperatures: BTreeMap::new(),
            damages: BTreeMap::new(),
        }
    }

    /// Fills one cell at rest with a material, packed RGBA color and radiance (`u32::MAX` selects
    /// the material's radiance), and temperature in kelvin, or the material's edit temperature
    /// when `None`, replacing an earlier fill of the cell
    pub fn fill_cell(
        &mut self,
        cell: CellCoordinates,
        material_identifier: MaterialIdentifier,
        color: u32,
        radiance: u32,
        temperature: Option<f32>,
    ) {
        self.fill_cell_with_velocity(
            cell,
            material_identifier,
            color,
            radiance,
            temperature,
            [0.0; 2],
        );
    }

    /// Fills one cell with a material and optional initial fluid velocity in tiles per second,
    /// with positive y pointing up; non-fluid materials ignore velocity and non-finite components
    /// become zero
    pub fn fill_cell_with_velocity(
        &mut self,
        cell: CellCoordinates,
        material_identifier: MaterialIdentifier,
        color: u32,
        radiance: u32,
        temperature: Option<f32>,
        velocity: [f32; 2],
    ) {
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        let (cells, material_identifiers, colors, radiances, temperatures):
            &mut (u64, [MaterialIdentifier; 64], [u32; 64], [u32; 64], [f32; 64]) = self.cellular_tiles
                .entry(cell.cellular_tile_coordinates())
                .or_insert((0, [MaterialIdentifier::EMPTY; 64], [0; 64], [0; 64], [f32::NAN; 64]));
        *cells |= 1 << cell_index;
        material_identifiers[cell_index] = material_identifier;
        colors[cell_index] = color;
        radiances[cell_index] = radiance;
        temperatures[cell_index] = temperature.unwrap_or(f32::NAN);
        let velocity: [f32; 2] = velocity.map(|component| {
            if component.is_finite() { component } else { 0.0 }
        });
        if material_identifier.form() != MaterialForm::Fluid || velocity == [0.0; 2] {
            self.fluid_velocities.remove(&cell);
        } else {
            self.fluid_velocities.insert(cell, velocity);
        }
    }

    /// Adds pressure in atmospheres to one cell, accumulating with earlier calls and ignoring a
    /// negative or nonfinite pressure
    pub fn add_pressure_cell(&mut self, cell: CellCoordinates, pressure: f32) {
        if !pressure.is_finite() || pressure < 0.0 { return; }
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        self.pressures.entry(cell.cellular_tile_coordinates())
            .or_insert([0.0; 64])[cell_index] += pressure;
    }

    /// Removes pressure in atmospheres from one cell, accumulating with earlier calls and ignoring
    /// a negative or nonfinite pressure
    pub fn remove_pressure_cell(&mut self, cell: CellCoordinates, pressure: f32) {
        if !pressure.is_finite() || pressure < 0.0 { return; }
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        self.pressures.entry(cell.cellular_tile_coordinates())
            .or_insert([0.0; 64])[cell_index] -= pressure;
    }

    /// Adds temperature in kelvin to everything in one cell, accumulating with earlier calls and
    /// ignoring a negative or nonfinite temperature
    pub fn add_temperature_cell(&mut self, cell: CellCoordinates, temperature: f32) {
        if !temperature.is_finite() || temperature < 0.0 { return; }
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        self.temperatures.entry(cell.cellular_tile_coordinates())
            .or_insert([0.0; 64])[cell_index] += temperature;
    }

    /// Removes temperature in kelvin from everything in one cell, never below zero, accumulating
    /// with earlier calls and ignoring a negative or nonfinite temperature
    pub fn remove_temperature_cell(&mut self, cell: CellCoordinates, temperature: f32) {
        if !temperature.is_finite() || temperature < 0.0 { return; }
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        self.temperatures.entry(cell.cellular_tile_coordinates())
            .or_insert([0.0; 64])[cell_index] -= temperature;
    }

    /// Damages the cellular matter of one cell as a load in atmospheres borne for a duration in
    /// seconds, the way pressure damages static cells: the cell loses its material's damage
    /// coefficient times the load beyond its damage threshold times the duration in amount. When
    /// none is left a static cell breaks, becoming its debris by its debris chance or emptying, and
    /// a dynamic cell vanishes. Calls on one cell keep the greatest load and add their durations; a
    /// negative or nonfinite load or duration is ignored
    pub fn damage_cell(&mut self, cell: CellCoordinates, load: f32, duration: f32) {
        if !load.is_finite() || load < 0.0 || !duration.is_finite() || duration < 0.0 { return; }
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        let (loads, durations): &mut ([f32; 64], [f32; 64]) = self.damages
            .entry(cell.cellular_tile_coordinates())
            .or_insert(([0.0; 64], [0.0; 64]));
        loads[cell_index] = loads[cell_index].max(load);
        durations[cell_index] += duration;
    }

    /// Fills every cell whose center lies within a radius in tiles of a segment at rest with a
    /// material, packed RGBA color and radiance (`u32::MAX` selects the material's radiance), and
    /// temperature in kelvin, or the material's edit temperature when `None`
    pub fn fill_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        material_identifier: MaterialIdentifier,
        color: u32,
        radiance: u32,
        temperature: Option<f32>,
    ) -> Result<(), EditError> {
        self.fill_segment_with_velocity(
            start,
            end,
            radius,
            material_identifier,
            color,
            radiance,
            temperature,
            [0.0; 2],
        )
    }

    /// Fills cells within a segment radius with a material and optional initial fluid velocity in
    /// tiles per second, with positive y pointing up; non-fluid materials ignore velocity and
    /// non-finite components become zero
    pub fn fill_segment_with_velocity(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        material_identifier: MaterialIdentifier,
        color: u32,
        radiance: u32,
        temperature: Option<f32>,
        velocity: [f32; 2],
    ) -> Result<(), EditError> {
        for cell in Self::segment_cells(start, end, radius)? {
            self.fill_cell_with_velocity(
                cell, material_identifier, color, radiance, temperature, velocity,
            );
        }
        Ok(())
    }

    /// Adds pressure in atmospheres to every cell within a segment radius
    pub fn add_pressure_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        pressure: f32,
    ) -> Result<(), EditError> {
        if !pressure.is_finite() || pressure < 0.0 { return Err(EditError::InvalidPressure); }
        for cell in Self::segment_cells(start, end, radius)? {
            self.add_pressure_cell(cell, pressure);
        }
        Ok(())
    }

    /// Removes pressure in atmospheres from every cell within a segment radius
    pub fn remove_pressure_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        pressure: f32,
    ) -> Result<(), EditError> {
        if !pressure.is_finite() || pressure < 0.0 { return Err(EditError::InvalidPressure); }
        for cell in Self::segment_cells(start, end, radius)? {
            self.remove_pressure_cell(cell, pressure);
        }
        Ok(())
    }

    /// Adds temperature in kelvin to everything in every cell within a segment radius
    pub fn add_temperature_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        temperature: f32,
    ) -> Result<(), EditError> {
        if !temperature.is_finite() || temperature < 0.0 {
            return Err(EditError::InvalidTemperature);
        }
        for cell in Self::segment_cells(start, end, radius)? {
            self.add_temperature_cell(cell, temperature);
        }
        Ok(())
    }

    /// Removes temperature in kelvin from everything in every cell within a segment radius, never
    /// below zero
    pub fn remove_temperature_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        temperature: f32,
    ) -> Result<(), EditError> {
        if !temperature.is_finite() || temperature < 0.0 {
            return Err(EditError::InvalidTemperature);
        }
        for cell in Self::segment_cells(start, end, radius)? {
            self.remove_temperature_cell(cell, temperature);
        }
        Ok(())
    }

    /// Damages every cell within a segment radius as `damage_cell` does
    pub fn damage_segment(
        &mut self,
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
        load: f32,
        duration: f32,
    ) -> Result<(), EditError> {
        if !load.is_finite() || load < 0.0 || !duration.is_finite() || duration < 0.0 {
            return Err(EditError::InvalidDamage);
        }
        for cell in Self::segment_cells(start, end, radius)? {
            self.damage_cell(cell, load, duration);
        }
        Ok(())
    }

    /// Returns cells whose centers lie within a radius in tiles of a segment
    fn segment_cells(
        start: ScenePosition,
        end: ScenePosition,
        radius: f64,
    ) -> Result<Vec<CellCoordinates>, EditError> {
        let (relative_x, relative_y): (f64, f64) = end.relative_position(start);
        if !radius.is_finite() || radius < 0.0 || !relative_x.is_finite() || !relative_y.is_finite()
        { return Ok(Vec::new()); }
        // rasterize in cells relative to the start tile so distant positions stay exact
        let cellular_tile_size: f64 = CELLULAR_TILE_SIZE as f64;
        let start_x: f64 =
            f64::from(start.offset_x) / ScenePosition::OFFSET_UNITS_PER_TILE * cellular_tile_size;
        let start_y: f64 =
            f64::from(start.offset_y) / ScenePosition::OFFSET_UNITS_PER_TILE * cellular_tile_size;
        let segment_x: f64 = relative_x * cellular_tile_size;
        let segment_y: f64 = relative_y * cellular_tile_size;
        let radius: f64 = radius * cellular_tile_size;
        let minimum_x: i64 = (start_x + segment_x.min(0.0) - radius).floor() as i64;
        let maximum_x: i64 = (start_x + segment_x.max(0.0) + radius).floor() as i64;
        let minimum_y: i64 = (start_y + segment_y.min(0.0) - radius).floor() as i64;
        let maximum_y: i64 = (start_y + segment_y.max(0.0) + radius).floor() as i64;
        // reject the segment before rasterizing when its bounds span too many tiles
        let cellular_tile_count: i128 = (i128::from(maximum_x.div_euclid(CELLULAR_TILE_SIZE))
            - i128::from(minimum_x.div_euclid(CELLULAR_TILE_SIZE)) + 1)
            * (i128::from(maximum_y.div_euclid(CELLULAR_TILE_SIZE))
                - i128::from(minimum_y.div_euclid(CELLULAR_TILE_SIZE)) + 1);
        if cellular_tile_count > EDIT_CELLULAR_TILE_COUNT_MAXIMUM as i128 {
            return Err(EditError::CapacityReached);
        }
        // select every cell whose center is within the radius of its closest segment point
        let segment_length_squared: f64 = segment_x * segment_x + segment_y * segment_y;
        let mut cells: Vec<CellCoordinates> = Vec::new();
        for cell_y in minimum_y..=maximum_y {
            for cell_x in minimum_x..=maximum_x {
                let center_x: f64 = cell_x as f64 + 0.5 - start_x;
                let center_y: f64 = cell_y as f64 + 0.5 - start_y;
                let projection: f64 = if segment_length_squared == 0.0 { 0.0 } else {
                    ((center_x * segment_x + center_y * segment_y) / segment_length_squared)
                        .clamp(0.0, 1.0)
                };
                let distance_x: f64 = center_x - projection * segment_x;
                let distance_y: f64 = center_y - projection * segment_y;
                if distance_x * distance_x + distance_y * distance_y > radius * radius { continue; }
                cells.push(CellCoordinates {
                    x: start.tile_x * CELLULAR_TILE_SIZE + cell_x,
                    y: start.tile_y * CELLULAR_TILE_SIZE + cell_y,
                });
            }
        }
        Ok(cells)
    }

}

#[cfg(test)]
mod tests {

    use super::*;

    /// Keeps the greatest load and adds durations across damage on one cell
    #[test]
    fn damage_cell_keeps_maximum_load_and_adds_durations() {
        let cell: CellCoordinates = CellCoordinates { x: -1, y: 9 };
        let mut edit: Edit = Edit::new(EditMode::Replace);
        edit.damage_cell(cell, 20.0, 0.25);
        edit.damage_cell(cell, 30.0, 0.5);
        edit.damage_cell(cell, 0.0, 0.0);
        let (loads, durations): &([f32; 64], [f32; 64]) =
            edit.damages.get(&cell.cellular_tile_coordinates()).unwrap();
        let cell_index: usize = (cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE)) as usize;
        assert_eq!(loads[cell_index], 30.0);
        assert_eq!(durations[cell_index], 0.75);
    }

    /// Ignores damage with a negative or nonfinite load or duration
    #[test]
    fn damage_cell_ignores_invalid_load_and_duration() {
        let cell: CellCoordinates = CellCoordinates { x: 0, y: 0 };
        let mut edit: Edit = Edit::new(EditMode::Replace);
        edit.damage_cell(cell, f32::NAN, 1.0);
        edit.damage_cell(cell, f32::INFINITY, 1.0);
        edit.damage_cell(cell, -1.0, 1.0);
        edit.damage_cell(cell, 1.0, f32::NAN);
        edit.damage_cell(cell, 1.0, f32::INFINITY);
        edit.damage_cell(cell, 1.0, -1.0);
        assert!(edit.damages.is_empty());
    }

    /// Damages the cells under a segment and rejects invalid loads and durations
    #[test]
    fn damage_segment_rasterizes_cells_and_rejects_invalid_inputs() {
        let mut edit: Edit = Edit::new(EditMode::Replace);
        let start: ScenePosition = ScenePosition::default();
        edit.damage_segment(start, start, 0.2, 0.5, 2.0).unwrap();
        assert!(edit.damages.contains_key(
            &CellCoordinates { x: 0, y: 0 }.cellular_tile_coordinates(),
        ));
        assert!(matches!(
            edit.damage_segment(start, start, 0.2, f32::NAN, 1.0),
            Err(EditError::InvalidDamage),
        ));
        assert!(matches!(
            edit.damage_segment(start, start, 0.2, 1.0, f32::NAN),
            Err(EditError::InvalidDamage),
        ));
    }

    /// Zeroes nonfinite velocity components and clears a velocity refilled at rest
    #[test]
    fn fill_cell_with_velocity_sanitizes_and_replaces_velocity() {
        let cell: CellCoordinates = CellCoordinates { x: -1, y: 9 };
        let fluid: MaterialIdentifier = MaterialIdentifier::from_u32(0x80000001);
        let mut edit: Edit = Edit::new(EditMode::Replace);
        edit.fill_cell_with_velocity(cell, fluid, 0, 0, None, [12.0, f32::NAN]);
        assert_eq!(edit.fluid_velocities.get(&cell), Some(&[12.0, 0.0]));
        edit.fill_cell(cell, fluid, 0, 0, None);
        assert!(!edit.fluid_velocities.contains_key(&cell));
    }

    /// Records no velocity for a material that is not a fluid
    #[test]
    fn fill_cell_with_velocity_ignores_nonfluid_material_velocity() {
        let cell: CellCoordinates = CellCoordinates { x: 0, y: 0 };
        let mut edit: Edit = Edit::new(EditMode::Replace);
        edit.fill_cell_with_velocity(cell, MaterialIdentifier::EMPTY, 0, 0, None, [1.0, 2.0]);
        assert!(edit.fluid_velocities.is_empty());
    }

    /// Records the velocity of every cell a segment fills
    #[test]
    fn fill_segment_with_velocity_records_each_filled_cell() {
        let mut edit: Edit = Edit::new(EditMode::Replace);
        let start: ScenePosition = ScenePosition::default();
        edit.fill_segment_with_velocity(
            start,
            start,
            0.75,
            MaterialIdentifier::from_u32(0x80000001),
            0,
            0,
            None,
            [1.0, -2.0],
        ).unwrap();
        assert!(!edit.fluid_velocities.is_empty());
        assert!(edit.fluid_velocities.values().all(|velocity| *velocity == [1.0, -2.0]));
    }

}
