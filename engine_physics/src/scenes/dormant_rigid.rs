// Copyright Rob Gage 2026

use crate::{
    materials::{Material, MaterialForm, MaterialIdentifier, MaterialRegistry},
    tiles::CellularAppearance,
};
use std::{collections::HashSet, io};

/// Authoritative, handle-free state for a rigid body outside simulation residency.
///
/// SceneData stores each record once, in the file keyed by the chunk containing
/// `position`.  Cells may span arbitrary chunks; they never own copies.
#[derive(Clone)]
pub(crate) struct DormantRigidBody {
    pub(crate) id: u64,
    pub(crate) position: [f32; 2],
    pub(crate) rotation: f32,
    pub(crate) linear_velocity: [f32; 2],
    pub(crate) angular_velocity: f32,
    pub(crate) cells: Vec<DormantRigidCell>,
}

#[derive(Clone)]
pub(crate) struct DormantRigidCell {
    pub(crate) local: [i32; 2],
    pub(crate) material: MaterialIdentifier,
    pub(crate) appearance: CellularAppearance,
    pub(crate) integrity: f32,
    pub(crate) amount: f32,
    pub(crate) temperature: f32,
}

impl DormantRigidBody {
    const MAX_CELLS: usize = 1 << 20;

    pub(crate) fn validate(&self, materials: &MaterialRegistry) -> Result<(), io::Error> {
        if self.id == 0
            || self.cells.is_empty()
            || self.cells.len() > Self::MAX_CELLS
            || !self
                .position
                .into_iter()
                .chain(self.linear_velocity)
                .all(f32::is_finite)
            || !self.rotation.is_finite()
            || !self.angular_velocity.is_finite()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid dormant rigid body",
            ));
        }
        let mut locals = HashSet::with_capacity(self.cells.len());
        for cell in &self.cells {
            if !locals.insert(cell.local)
                || !matches!(
                    materials.get(cell.material),
                    Some(Material::CellularStatic { .. })
                )
                || cell.material.form_checked() != Some(MaterialForm::CellularStatic)
                || !cell.integrity.is_finite()
                || !cell.amount.is_finite()
                || cell.amount <= 0.0
                || !cell.temperature.is_finite()
                || cell.temperature < 0.0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid dormant rigid cell",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn serialize<W: io::Write>(
        &self,
        writer: &mut W,
        materials: &MaterialRegistry,
    ) -> Result<(), io::Error> {
        self.validate(materials)?;
        writer.write_all(&self.id.to_le_bytes())?;
        for value in self
            .position
            .into_iter()
            .chain([self.rotation])
            .chain(self.linear_velocity)
            .chain([self.angular_velocity])
        {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(
            &(u32::try_from(self.cells.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "too many dormant rigid cells")
            })?)
            .to_le_bytes(),
        )?;
        for cell in &self.cells {
            writer.write_all(&cell.local[0].to_le_bytes())?;
            writer.write_all(&cell.local[1].to_le_bytes())?;
            writer.write_all(&cell.material.as_u32().to_le_bytes())?;
            writer.write_all(&cell.appearance.0.to_le_bytes())?;
            for value in [cell.integrity, cell.amount, cell.temperature] {
                writer.write_all(&value.to_bits().to_le_bytes())?;
            }
        }
        Ok(())
    }

    pub(crate) fn deserialize<R: io::Read>(
        reader: &mut R,
        materials: &MaterialRegistry,
    ) -> Result<Self, io::Error> {
        let u32 = |reader: &mut R| -> Result<u32, io::Error> {
            let mut b = [0; 4];
            reader.read_exact(&mut b)?;
            Ok(u32::from_le_bytes(b))
        };
        let mut id = [0; 8];
        reader.read_exact(&mut id)?;
        let position = [f32::from_bits(u32(reader)?), f32::from_bits(u32(reader)?)];
        let rotation = f32::from_bits(u32(reader)?);
        let linear_velocity = [f32::from_bits(u32(reader)?), f32::from_bits(u32(reader)?)];
        let angular_velocity = f32::from_bits(u32(reader)?);
        let count = usize::try_from(u32(reader)?).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dormant rigid cell count overflow",
            )
        })?;
        if count > Self::MAX_CELLS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "too many dormant rigid cells",
            ));
        }
        let mut cells = Vec::new();
        cells.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dormant rigid allocation failed",
            )
        })?;
        for _ in 0..count {
            cells.push(DormantRigidCell {
                local: [
                    i32::from_le_bytes(u32(reader)?.to_le_bytes()),
                    i32::from_le_bytes(u32(reader)?.to_le_bytes()),
                ],
                material: MaterialIdentifier::from_u32(u32(reader)?),
                appearance: CellularAppearance(u32(reader)?),
                integrity: f32::from_bits(u32(reader)?),
                amount: f32::from_bits(u32(reader)?),
                temperature: f32::from_bits(u32(reader)?),
            });
        }
        let body = Self {
            id: u64::from_le_bytes(id),
            position,
            rotation,
            linear_velocity,
            angular_velocity,
            cells,
        };
        body.validate(materials)?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_graphics::{Color, MaterialAppearance};

    fn materials() -> (MaterialRegistry, MaterialIdentifier) {
        let mut registry = MaterialRegistry::new();
        let id = registry.register(Material::CellularStatic {
            name: "test".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 2, 3)),
            mass: 1.0,
            pressure_ignore_threshold: 0.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 0.0,
            friction: 0.0,
            restitution: 0.0,
        });
        (registry, id)
    }

    #[test]
    fn dormant_rigid_serialization_preserves_authoritative_state() {
        let (materials, material) = materials();
        let body = DormantRigidBody {
            id: 9,
            position: [1.25, -2.5],
            rotation: 0.75,
            linear_velocity: [3.0, -4.0],
            angular_velocity: 5.0,
            cells: vec![
                DormantRigidCell {
                    local: [-2, 3],
                    material,
                    appearance: CellularAppearance(0x1234_5678),
                    integrity: 0.25,
                    amount: 0.5,
                    temperature: 456.0,
                },
                DormantRigidCell {
                    local: [4, 5],
                    material,
                    appearance: CellularAppearance(7),
                    integrity: 0.75,
                    amount: 1.0,
                    temperature: 789.0,
                },
            ],
        };
        let mut bytes = Vec::new();
        body.serialize(&mut bytes, &materials).unwrap();
        let loaded = DormantRigidBody::deserialize(&mut bytes.as_slice(), &materials).unwrap();
        assert_eq!(loaded.id, body.id);
        assert_eq!(
            loaded.position.map(f32::to_bits),
            body.position.map(f32::to_bits)
        );
        assert_eq!(loaded.rotation.to_bits(), body.rotation.to_bits());
        assert_eq!(
            loaded.linear_velocity.map(f32::to_bits),
            body.linear_velocity.map(f32::to_bits)
        );
        assert_eq!(
            loaded.angular_velocity.to_bits(),
            body.angular_velocity.to_bits()
        );
        assert_eq!(loaded.cells[0].local, body.cells[0].local);
        assert_eq!(loaded.cells[0].appearance.0, body.cells[0].appearance.0);
        assert_eq!(loaded.cells[0].integrity, body.cells[0].integrity);
        assert_eq!(loaded.cells[1].amount, body.cells[1].amount);
        assert_eq!(loaded.cells[1].temperature, body.cells[1].temperature);
    }

    #[test]
    fn malformed_dormant_rigid_is_rejected() {
        let (materials, material) = materials();
        let body = DormantRigidBody {
            id: 1,
            position: [0.0, 0.0],
            rotation: 0.0,
            linear_velocity: [0.0; 2],
            angular_velocity: 0.0,
            cells: vec![DormantRigidCell {
                local: [0, 0],
                material,
                appearance: CellularAppearance::NEUTRAL,
                integrity: 1.0,
                amount: 0.0,
                temperature: 1.0,
            }],
        };
        assert_eq!(
            body.validate(&materials).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
