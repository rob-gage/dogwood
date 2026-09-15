// Copyright Rob Gage 2026

use crate::{
    materials::{Material, MaterialIdentifier, MaterialRegistry},
    tiles::CellularAppearance,
};
use rapier2d::prelude::{
    MassProperties,
    Pose,
    RigidBodyHandle,
    SharedShape,
    Vector,
};
use std::collections::HashSet;

/// Authoritative body-local static cellular matter owned by one Rapier body
pub(crate) struct RigidCellularBody {
    pub(crate) handle: RigidBodyHandle,
    /// Local integer cells relative to the body's local origin
    pub(crate) cells: Vec<([i32; 2], MaterialIdentifier, CellularAppearance)>,
}

impl RigidCellularBody {

    /// Calculates aggregate local mass properties from constituent cellular material
    pub(crate) fn mass_properties(
        cells: &[([i32; 2], MaterialIdentifier, CellularAppearance)],
        materials: &MaterialRegistry,
    ) -> MassProperties {
        let (total_mass, weighted_center) = cells.iter().fold(
            (0.0, [0.0; 2]),
            |(total, weighted), (cell, identifier, _)| {
                let Some(Material::CellularStatic { mass, .. }) = materials.get(*identifier)
                    else { panic!("Rigid cellular body contains a non-static material"); };
                let center = [(cell[0] as f32 + 0.5) / 8.0,
                    (cell[1] as f32 + 0.5) / 8.0];
                (total + mass, [weighted[0] + mass * center[0],
                    weighted[1] + mass * center[1]])
            },
        );
        assert!(total_mass.is_finite() && total_mass > 0.0);
        let center = [weighted_center[0] / total_mass, weighted_center[1] / total_mass];
        let inertia = cells.iter().map(|(cell, identifier, _)| {
            let Some(Material::CellularStatic { mass, .. }) = materials.get(*identifier)
                else { unreachable!() };
            let offset = [(cell[0] as f32 + 0.5) / 8.0 - center[0],
                (cell[1] as f32 + 0.5) / 8.0 - center[1]];
            mass / 384.0 + mass * (offset[0] * offset[0] + offset[1] * offset[1])
        }).sum();
        MassProperties::new(Vector::new(center[0], center[1]), total_mass, inertia)
    }

    /// Builds a greedy rectangle compound in body-local tile units
    pub(crate) fn collision_shape(
        cells: &[([i32; 2], MaterialIdentifier, CellularAppearance)],
    ) -> SharedShape {
        let occupied: HashSet<[i32; 2]> = cells.iter().map(|cell| cell.0).collect();
        let mut consumed: HashSet<[i32; 2]> = HashSet::new();
        let mut ordered: Vec<[i32; 2]> = occupied.iter().copied().collect();
        ordered.sort_unstable_by_key(|cell| (cell[1], cell[0]));
        let mut parts: Vec<(Pose, SharedShape)> = Vec::new();
        for [x, y] in ordered {
            if consumed.contains(&[x, y]) { continue; }
            let mut width: i32 = 1;
            while occupied.contains(&[x + width, y]) &&
                    !consumed.contains(&[x + width, y]) { width += 1; }
            let mut height: i32 = 1;
            while (x..x + width).all(|column| {
                occupied.contains(&[column, y + height]) &&
                    !consumed.contains(&[column, y + height])
            }) { height += 1; }
            for row in y..y + height {
                for column in x..x + width { consumed.insert([column, row]); }
            }
            parts.push((
                Pose::translation(
                    x as f32 / 8.0 + width as f32 / 16.0,
                    y as f32 / 8.0 + height as f32 / 16.0,
                ),
                SharedShape::cuboid(width as f32 / 16.0, height as f32 / 16.0),
            ));
        }
        SharedShape::compound(parts)
    }

    /// Splits occupied local cells into deterministic 4-connected components
    pub(crate) fn connected_components(
        cells: Vec<([i32; 2], MaterialIdentifier, CellularAppearance)>,
    ) -> Vec<Vec<([i32; 2], MaterialIdentifier, CellularAppearance)>> {
        let mut remaining: std::collections::HashMap<
            [i32; 2], (MaterialIdentifier, CellularAppearance),
        > = cells.into_iter().map(|(cell, material, appearance)| {
            (cell, (material, appearance))
        }).collect();
        let mut components = Vec::new();
        while let Some(start) = remaining.keys().min_by_key(|cell| (cell[1], cell[0])).copied() {
            let mut pending = vec![start];
            let mut component = Vec::new();
            while let Some(cell) = pending.pop() {
                let Some((material, appearance)) = remaining.remove(&cell) else { continue; };
                component.push((cell, material, appearance));
                pending.extend([[cell[0] - 1, cell[1]], [cell[0] + 1, cell[1]],
                    [cell[0], cell[1] - 1], [cell[0], cell[1] + 1]]);
            }
            components.push(component);
        }
        components
    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::materials::MaterialForm;

    #[test]
    fn removed_bridge_splits_body_local_cells() {
        let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
        let cells = [[0, 0], [1, 0], [2, 0]].into_iter().map(|cell| {
            (cell, material, CellularAppearance::NEUTRAL)
        }).filter(|cell| cell.0 != [1, 0]).collect();
        let components = RigidCellularBody::connected_components(cells);
        assert!(components.len() == 2);
        assert!(components.iter().all(|component| component.len() == 1));
    }

}
