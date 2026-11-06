// Copyright Rob Gage 2026

use super::{
    SoftBodyAnchor,
    SoftBodyDescriptor,
    SoftBodyEnvironment,
    SOFT_BODY_ENVIRONMENT_ACCELERATION_MAXIMUM,
    SOFT_BODY_GAS_SPEED_MINIMUM,
    SOFT_BODY_PRESSURE_GRADIENT_MINIMUM,
};
use crate::materials::MaterialIdentifier;
use rapier2d::dynamics::SoftBodyHandle;
use std::collections::{BTreeMap, HashSet};

/// One tile of a segment's cells, in the layout of the rigid body rasterization
#[derive(Clone, Debug)]
pub(crate) struct SoftBodySegmentTile {
    /// Cells held, as bit `y * 8 + x`
    pub(crate) cells: u64,
    /// Material of each cell
    pub(crate) materials: [MaterialIdentifier; 64],
    /// Color of each cell
    pub(crate) colors: [u32; 64],
    /// Radiance of each cell
    pub(crate) radiances: [u32; 64],
    /// Stored pressure, kinematics, temperature bits, and amount of each cell
    pub(crate) states: [[u32; 4]; 64],
    /// GPU tile
    pub(crate) index: u32,
    /// Cells last rasterized
    pub(crate) rasterized: u64,
}

/// The cells between two particles, a tiny rigid body of cells to the rasterization
#[derive(Clone, Debug)]
pub(crate) struct SoftBodySegment {
    /// Particle at its origin
    pub(crate) first: u32,
    /// Particle its rows run toward
    pub(crate) second: u32,
    /// Rows between the particles
    pub(crate) length: i32,
    /// GPU record
    pub(crate) index: u32,
    /// Tiles by their coordinates in the segment
    pub(crate) tiles: BTreeMap<(i32, i32), SoftBodySegmentTile>,
    /// Position and rotation cosine and sine in cells where last rasterized
    pub(crate) rasterization: Option<([f32; 2], [f32; 2])>,
    /// Whether its tiles changed since they were last written to the GPU
    pub(crate) is_dirty: bool,
    /// Whether the GPU holds its cells' states, which the simulation owns from then on
    pub(crate) has_written_states: bool,
}

impl SoftBodySegment {

    /// Returns how many cells it holds
    pub(crate) fn cell_count(&self) -> usize {
        self.tiles.values().map(|tile| tile.cells.count_ones() as usize).sum()
    }

    /// Returns the cells not connected to its first row, and whether connected cells reach its
    /// last row
    pub(crate) fn connectivity(&self) -> (Vec<(i32, i32)>, bool) {
        let mut remaining: HashSet<(i32, i32)> = HashSet::new();
        for ((tile_x, tile_y), tile) in &self.tiles {
            let mut bits: u64 = tile.cells;
            while bits != 0 {
                let bit: i32 = bits.trailing_zeros() as i32;
                bits &= bits - 1;
                remaining.insert((tile_x * 8 + bit % 8, tile_y * 8 + bit / 8));
            }
        }
        let mut frontier: Vec<(i32, i32)> =
            remaining.iter().copied().filter(|cell| cell.1 <= 0).collect();
        for cell in &frontier { remaining.remove(cell); }
        let mut reaches_end: bool = false;
        while let Some((x, y)) = frontier.pop() {
            if y >= self.length - 1 { reaches_end = true; }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if remaining.remove(&(x + dx, y + dy)) { frontier.push((x + dx, y + dy)); }
                }
            }
        }
        (remaining.into_iter().collect(), reaches_end)
    }

    /// Takes a cell out of the segment, returning its material
    pub(crate) fn remove_cell(&mut self, cell: (i32, i32)) -> Option<&mut MaterialIdentifier> {
        let tile: &mut SoftBodySegmentTile =
            self.tiles.get_mut(&(cell.0.div_euclid(8), cell.1.div_euclid(8)))?;
        let bit: usize = (cell.1.rem_euclid(8) * 8 + cell.0.rem_euclid(8)) as usize;
        tile.cells &= !(1 << bit);
        self.is_dirty = true;
        Some(&mut tile.materials[bit])
    }

}

/// What Dogwood keeps beside one Rapier soft body: its place in the cell grid, its art, and its
/// anchors, particle arrays parallel to Rapier's
#[derive(Clone, Debug)]
pub(crate) struct SoftBody {
    /// The Rapier soft body
    pub(crate) handle: SoftBodyHandle,
    /// Particle positions after the last terrain contacts, in tiles relative to the Rapier origin
    pub(crate) settled: Vec<[f32; 2]>,
    /// Accelerations from the cell grid at the newest sample, in tiles per second squared
    pub(crate) accelerations: Vec<[f32; 2]>,
    /// Newest sample of the cell grid around each particle
    pub(crate) environments: Vec<SoftBodyEnvironment>,
    /// Whether each particle is held at a target by the body's owner
    pub(crate) held: Vec<bool>,
    /// Anchors still holding
    pub(crate) anchors: Vec<SoftBodyAnchor>,
    /// Static cells carried between particles
    pub(crate) segments: Vec<SoftBodySegment>,
    /// Fewest cells of a piece, below which its cells decompose
    pub(crate) minimum_cell_count: usize,
    /// Whether its cells changed, so that it should be checked for being too small
    pub(crate) is_cell_changed: bool,
    /// Fraction of motion along a solid cell lost at a contact
    pub(crate) friction: f32,
    /// Particle thickness against rigid bodies in tiles
    pub(crate) radius: f32,
    /// Acceleration per atmosphere per cell of pressure gradient
    pub(crate) pressure_response: f32,
    /// Acceleration per tile per second of gas flow
    pub(crate) gas_response: f32,
    /// Rate per second of taking on the velocity of surrounding fluid
    pub(crate) fluid_drag: f32,
    /// Fraction of gravity cancelled when submerged
    pub(crate) buoyancy: f32,
    /// Share of gravity applied
    pub(crate) gravity_scale: f32,
    /// Whether Rapier skips this body until something disturbs it
    pub(crate) is_sleeping: bool,
    /// Consecutive steps at rest
    pub(crate) rest_steps: u32,
    /// Smallest corner of the particles' bounding box in tiles
    pub(crate) bounds_minimum: [f32; 2],
    /// Largest corner of the particles' bounding box in tiles
    pub(crate) bounds_maximum: [f32; 2],
}

impl SoftBody {

    /// Creates the record of a Rapier body built from a descriptor
    pub(crate) fn new(
        handle: SoftBodyHandle,
        descriptor: &SoftBodyDescriptor,
        positions: Vec<[f32; 2]>,
    ) -> Self {
        let count: usize = positions.len();
        let mut body: Self = Self {
            handle,
            settled: positions,
            accelerations: vec![[0.0; 2]; count],
            environments: vec![SoftBodyEnvironment::default(); count],
            held: vec![false; count],
            anchors: descriptor.anchors.clone(),
            segments: Vec::new(),
            minimum_cell_count: descriptor.minimum_cell_count,
            is_cell_changed: false,
            friction: descriptor.friction.clamp(0.0, 1.0),
            radius: descriptor.radius.max(0.0),
            pressure_response: descriptor.pressure_response,
            gas_response: descriptor.gas_response,
            fluid_drag: descriptor.fluid_drag.max(0.0),
            buoyancy: descriptor.buoyancy.clamp(0.0, 1.0),
            gravity_scale: descriptor.gravity_scale,
            is_sleeping: false,
            rest_steps: 0,
            bounds_minimum: [0.0; 2],
            bounds_maximum: [0.0; 2],
        };
        body.update_bounds();
        body
    }

    /// Recomputes the bounding box of the settled particles
    pub(crate) fn update_bounds(&mut self) {
        let mut minimum: [f32; 2] = [f32::INFINITY; 2];
        let mut maximum: [f32; 2] = [f32::NEG_INFINITY; 2];
        for position in &self.settled {
            for axis in 0..2 {
                minimum[axis] = minimum[axis].min(position[axis]);
                maximum[axis] = maximum[axis].max(position[axis]);
            }
        }
        self.bounds_minimum = minimum;
        self.bounds_maximum = maximum;
    }

    /// Returns whether a box, grown by a margin, touches this body's bounding box
    pub(crate) fn bounds_near(&self, minimum: [f32; 2], maximum: [f32; 2], margin: f32) -> bool {
        self.bounds_minimum[0] - margin <= maximum[0] && self.bounds_maximum[0] + margin >= minimum[0]
            && self.bounds_minimum[1] - margin <= maximum[1]
            && self.bounds_maximum[1] + margin >= minimum[1]
    }

    /// Records a sample of a particle's surroundings, and the acceleration they cause, for the
    /// particles it stands for
    pub(crate) fn apply_environment(
        &mut self,
        particle: usize,
        environment: SoftBodyEnvironment,
        spread: usize,
        gravity: f32,
        velocity: [f32; 2],
    ) {
        // ordinary air stirs nothing: only real pressure waves push, so bodies can come to rest
        let push = |gradient: f32| -> f32 {
            if gradient.abs() < SOFT_BODY_PRESSURE_GRADIENT_MINIMUM { 0.0 } else { -gradient * self.pressure_response }
        };
        let gas_speed: f32 = environment.gas_velocity[0].hypot(environment.gas_velocity[1]);
        let gas_share: f32 = if gas_speed < SOFT_BODY_GAS_SPEED_MINIMUM { 0.0 } else { self.gas_response };
        let mut acceleration: [f32; 2] = [
            push(environment.pressure_gradient[0]) + environment.gas_velocity[0] * gas_share,
            push(environment.pressure_gradient[1]) + environment.gas_velocity[1] * gas_share,
        ];
        let coverage: f32 = environment.fluid_coverage.clamp(0.0, 1.0);
        if coverage > 0.0 {
            acceleration[0] += (environment.fluid_velocity[0] - velocity[0])
                * self.fluid_drag * coverage;
            acceleration[1] += (environment.fluid_velocity[1] - velocity[1])
                * self.fluid_drag * coverage
                + gravity * self.gravity_scale * self.buoyancy * coverage;
        }
        let magnitude: f32 = (acceleration[0] * acceleration[0] + acceleration[1] * acceleration[1])
            .sqrt();
        if magnitude > SOFT_BODY_ENVIRONMENT_ACCELERATION_MAXIMUM {
            let scale: f32 = SOFT_BODY_ENVIRONMENT_ACCELERATION_MAXIMUM / magnitude;
            acceleration = [acceleration[0] * scale, acceleration[1] * scale];
        }
        // a sparse sample stands for the particles around it
        let first: usize = particle.saturating_sub(spread / 2);
        let last: usize = (particle + spread.div_ceil(2)).min(self.settled.len());
        for index in first..last {
            self.accelerations[index] = acceleration;
            self.environments[index] = environment;
        }
    }

}
