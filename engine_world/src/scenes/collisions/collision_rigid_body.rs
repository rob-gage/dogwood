// Copyright Rob Gage 2026

use crate::{
    materials::{
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::cellular_tiles::CELLULAR_TILE_SIZE,
};
use std::collections::{
    BTreeMap,
    HashSet,
};

/// A rigid body of static cells simulated by Rapier and rasterized into the cellular tiles
pub struct CollisionRigidBody {
    /// Index of this rigid body's GPU record
    pub index: u32,
    /// Cells of every rigid body tile by its coordinates in this body: cells as bit `y * 8 + x`,
    /// their material identifiers, colors, pressure and integrity, its GPU tile, and the cells
    /// last rasterized
    pub cellular_tiles: BTreeMap<
        (i32, i32),
        (u64, [MaterialIdentifier; 64], [u32; 64], [[u32; 2]; 64], u32, u64),
    >,
    /// Rapier body of this rigid body
    pub rigid_body_handle: rapier2d::dynamics::RigidBodyHandle,
    /// Voxel collider of this rigid body's cells
    pub collider_handle: rapier2d::geometry::ColliderHandle,
    /// Position of the body's first cell corner and rotation cosine and sine in cells from the
    /// collision origin, velocity in cells per tick, and angular velocity in radians per tick
    /// where this body was last rasterized, absent before its first rasterization
    pub rasterization: Option<([f32; 2], [f32; 2], [f32; 2], f32)>,
    /// Normal and arm from the center of mass in tiles of the dynamic cells holding this body
    /// back on each side of its center of mass, kept until newer contacts arrive
    pub holding: [Option<([f32; 2], [f32; 2])>; 2],
}

impl CollisionRigidBody {

    /// Returns the mass, center of mass in tiles, angular inertia around it, and mass-weighted
    /// friction and restitution of this rigid body's cells
    pub fn mass_properties(
        &self,
        material_table: &MaterialTable,
    ) -> (f32, [f32; 2], f32, f32, f32) {
        let cell_size: f64 = 1.0 / CELLULAR_TILE_SIZE as f64;
        let mut mass: f64 = 0.0;
        let mut moment: [f64; 2] = [0.0; 2];
        let mut second_moment: f64 = 0.0;
        let mut friction: f64 = 0.0;
        let mut restitution: f64 = 0.0;
        for ((tile_x, tile_y), (cells, material_identifiers, ..)) in &self.cellular_tiles {
            let mut remaining_cells: u64 = *cells;
            while remaining_cells != 0 {
                let cell: usize = remaining_cells.trailing_zeros() as usize;
                remaining_cells &= remaining_cells - 1;
                let Some(properties) =
                    material_table.cellular_static_properties(material_identifiers[cell])
                else { continue; };
                let cell_mass: f64 = f64::from(properties.mass);
                let x: f64 = (f64::from(tile_x * 8) + (cell % 8) as f64 + 0.5) * cell_size;
                let y: f64 = (f64::from(tile_y * 8) + (cell / 8) as f64 + 0.5) * cell_size;
                mass += cell_mass;
                moment[0] += cell_mass * x;
                moment[1] += cell_mass * y;
                // each square cell spins around its own center as well as the body's
                second_moment += cell_mass * (x * x + y * y + cell_size * cell_size / 6.0);
                friction += cell_mass * f64::from(properties.friction);
                restitution += cell_mass * f64::from(properties.restitution);
            }
        }
        if mass <= 0.0 { return (0.0, [0.0; 2], 0.0, 0.0, 0.0); }
        let center: [f64; 2] = [moment[0] / mass, moment[1] / mass];
        let inertia: f64 = second_moment - mass * (center[0] * center[0] + center[1] * center[1]);
        (
            mass as f32,
            [center[0] as f32, center[1] as f32],
            inertia.max(f64::MIN_POSITIVE) as f32,
            (friction / mass) as f32,
            (restitution / mass) as f32,
        )
    }

    /// Returns the cells of every group of side-connected cells in this rigid body, as cells of
    /// every tile by its coordinates, largest group first
    pub fn connected_cells(&self) -> Vec<BTreeMap<(i32, i32), u64>> {
        let is_filled = |x: i32, y: i32| -> bool {
            self.cellular_tiles.get(&(x.div_euclid(8), y.div_euclid(8))).is_some_and(
                |(cells, ..)| cells >> (y.rem_euclid(8) * 8 + x.rem_euclid(8)) & 1 == 1,
            )
        };
        let mut visited_cells: HashSet<(i32, i32)> = HashSet::new();
        let mut groups: Vec<(usize, BTreeMap<(i32, i32), u64>)> = Vec::new();
        for ((tile_x, tile_y), (cells, ..)) in &self.cellular_tiles {
            let mut remaining_cells: u64 = *cells;
            while remaining_cells != 0 {
                let cell: i32 = remaining_cells.trailing_zeros() as i32;
                remaining_cells &= remaining_cells - 1;
                let start: (i32, i32) = (tile_x * 8 + cell % 8, tile_y * 8 + cell / 8);
                if !visited_cells.insert(start) { continue; }
                // flood every cell reachable through shared sides
                let mut group: BTreeMap<(i32, i32), u64> = BTreeMap::new();
                let mut group_cell_count: usize = 0;
                let mut pending_cells: Vec<(i32, i32)> = vec![start];
                while let Some((x, y)) = pending_cells.pop() {
                    *group.entry((x.div_euclid(8), y.div_euclid(8))).or_insert(0) |=
                        1 << (y.rem_euclid(8) * 8 + x.rem_euclid(8));
                    group_cell_count += 1;
                    for (neighbor_x, neighbor_y) in
                        [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
                    {
                        if is_filled(neighbor_x, neighbor_y)
                            && visited_cells.insert((neighbor_x, neighbor_y))
                        {
                            pending_cells.push((neighbor_x, neighbor_y));
                        }
                    }
                }
                groups.push((group_cell_count, group));
            }
        }
        groups.sort_by(|(count, _), (other_count, _)| other_count.cmp(count));
        groups.into_iter().map(|(_, group)| group).collect()
    }

}
