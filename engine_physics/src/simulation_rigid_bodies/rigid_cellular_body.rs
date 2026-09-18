// Copyright Rob Gage 2026

use std::collections::HashSet;

use rapier2d::prelude::MassProperties;
use rapier2d::prelude::Pose;
use rapier2d::prelude::RigidBodyHandle;
use rapier2d::prelude::SharedShape;
use rapier2d::prelude::Vector;

use super::RigidCellularBodyCell;
use crate::materials::Material;
use crate::materials::MaterialRegistry;

/// Authoritative body-local static cellular matter owned by one Rapier body
pub(crate) struct RigidCellularBody {
    /// Scene-stable identity; runtime handles and state slots are never persisted.
    pub(crate) identifier: u64,
    pub(crate) handle: RigidBodyHandle,
    /// Local integer cells relative to the body's local origin
    pub(crate) cells: Vec<RigidCellularBodyCell>,
}

impl RigidCellularBody {
    /// Calculates aggregate local mass properties from constituent cellular material
    pub(crate) fn mass_properties(
        cells: &[RigidCellularBodyCell],
        materials: &MaterialRegistry,
    ) -> MassProperties {
        let (total_mass, weighted_center): (f32, [f32; 2]) =
            cells
                .iter()
                .fold((0.0, [0.0; 2]), |(total, weighted), cell| {
                    let Some(Material::CellularStatic { mass, .. }) = materials.get(cell.material)
                    else {
                        panic!("Rigid cellular body contains a non-static material");
                    };
                    let center: [f32; 2] = [
                        (cell.local[0] as f32 + 0.5) / 8.0,
                        (cell.local[1] as f32 + 0.5) / 8.0,
                    ];
                    (
                        total + mass,
                        [
                            weighted[0] + mass * center[0],
                            weighted[1] + mass * center[1],
                        ],
                    )
                });
        assert!(total_mass.is_finite() && total_mass > 0.0);
        let center: [f32; 2] = [
            weighted_center[0] / total_mass,
            weighted_center[1] / total_mass,
        ];
        let inertia: f32 = cells
            .iter()
            .map(|cell| {
                let Some(Material::CellularStatic { mass, .. }) = materials.get(cell.material)
                else {
                    unreachable!()
                };
                let rigid_cell_center_offset: [f32; 2] = [
                    (cell.local[0] as f32 + 0.5) / 8.0 - center[0],
                    (cell.local[1] as f32 + 0.5) / 8.0 - center[1],
                ];
                mass / 384.0
                    + mass
                        * (rigid_cell_center_offset[0] * rigid_cell_center_offset[0]
                            + rigid_cell_center_offset[1] * rigid_cell_center_offset[1])
            })
            .sum();
        MassProperties::new(Vector::new(center[0], center[1]), total_mass, inertia)
    }

    /// Builds a greedy rectangle compound in body-local tile units
    pub(crate) fn collision_shape(
        rigid_cellular_body_cells: &[RigidCellularBodyCell],
    ) -> SharedShape {
        let occupied: HashSet<[i32; 2]> = rigid_cellular_body_cells
            .iter()
            .map(|cell| cell.local)
            .collect();
        let mut consumed: HashSet<[i32; 2]> = HashSet::new();
        let mut ordered: Vec<[i32; 2]> = occupied.iter().copied().collect();
        ordered.sort_unstable_by_key(|cell| (cell[1], cell[0]));
        let mut parts: Vec<(Pose, SharedShape)> = Vec::new();
        for [x, y] in ordered {
            if consumed.contains(&[x, y]) {
                continue;
            }
            let mut width: i32 = 1;
            while occupied.contains(&[x + width, y]) && !consumed.contains(&[x + width, y]) {
                width += 1;
            }
            let mut height: i32 = 1;
            while (x..x + width).all(|column| {
                occupied.contains(&[column, y + height])
                    && !consumed.contains(&[column, y + height])
            }) {
                height += 1;
            }
            for row in y..y + height {
                for column in x..x + width {
                    consumed.insert([column, row]);
                }
            }
            parts.push((
                Pose::translation(
                    x as f32 / 8.0 + width as f32 / 16.0,
                    y as f32 / 8.0 + height as f32 / 16.0,
                ),
                SharedShape::cuboid(width as f32 / 16.0, height as f32 / 16.0),
            ));
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            cells = rigid_cellular_body_cells.len(),
            compound_children = parts.len(),
            "rigid collider complexity"
        );
        SharedShape::compound(parts)
    }

    /// Splits occupied local cells into deterministic 4-connected components
    pub(crate) fn connected_components(
        cells: Vec<RigidCellularBodyCell>,
    ) -> Vec<Vec<RigidCellularBodyCell>> {
        let mut remaining: std::collections::HashMap<[i32; 2], RigidCellularBodyCell> =
            cells.into_iter().map(|cell| (cell.local, cell)).collect();
        let mut components: Vec<Vec<RigidCellularBodyCell>> = Vec::new();
        while let Some(start) = remaining
            .keys()
            .min_by_key(|cell| (cell[1], cell[0]))
            .copied()
        {
            let mut pending: Vec<[i32; 2]> = vec![start];
            let mut component: Vec<RigidCellularBodyCell> = Vec::new();
            while let Some(cell) = pending.pop() {
                let Some(cell) = remaining.remove(&cell) else {
                    continue;
                };
                component.push(cell);
                pending.extend([
                    [cell.local[0] - 1, cell.local[1]],
                    [cell.local[0] + 1, cell.local[1]],
                    [cell.local[0], cell.local[1] - 1],
                    [cell.local[0], cell.local[1] + 1],
                ]);
            }
            components.push(component);
        }
        components
    }
}
