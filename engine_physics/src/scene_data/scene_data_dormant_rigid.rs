// Copyright Rob Gage 2026

use super::DormantRigidCell;
use crate::materials::{Material, MaterialForm, MaterialIdentifier, MaterialRegistry};
use crate::tiles::CellularAppearance;
use std::{collections::HashSet, io};

/// Authoritative, handle-free state for a rigid body outside simulation residency.
///
/// SceneData stores each record once, in the file keyed by the chunk containing
/// the center of its world-space geometry. Cells may span arbitrary chunks;
/// they never own copies.
#[derive(Clone)]
pub(crate) struct DormantRigidBody {
    pub(crate) identifier: u64,
    pub(crate) position: [f32; 2],
    pub(crate) rotation: f32,
    pub(crate) linear_velocity: [f32; 2],
    pub(crate) angular_velocity: f32,
    pub(crate) sleeping: bool,
    pub(crate) cells: Vec<DormantRigidCell>,
}

pub(crate) fn append_record(
    records: &mut Vec<DormantRigidBody>,
    record: DormantRigidBody,
) -> Result<(), io::Error> {
    let mut before: HashSet<u64> = records.iter().map(|record| record.identifier).collect();
    if !before.insert(record.identifier) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "duplicate dormant rigid identity",
        ));
    }
    records.push(record);
    debug_assert_eq!(
        records
            .iter()
            .map(|record| record.identifier)
            .collect::<HashSet<_>>(),
        before
    );
    Ok(())
}

pub(crate) fn remove_ids(records: &mut Vec<DormantRigidBody>, ids: &[u64]) {
    let before: HashSet<u64> = records.iter().map(|record| record.identifier).collect();
    let claimed: HashSet<u64> = ids.iter().copied().collect();
    records.retain(|record| !claimed.contains(&record.identifier));
    let after: HashSet<u64> = records.iter().map(|record| record.identifier).collect();
    debug_assert_eq!(
        after,
        before.difference(&claimed).copied().collect::<HashSet<_>>()
    );
}

/// Conservative world-space bounds of the occupied cell squares.
pub(crate) fn world_aabb<I>(
    position: [f32; 2],
    rotation: f32,
    locals: I,
) -> Option<([f32; 2], [f32; 2])>
where
    I: IntoIterator<Item = [i32; 2]>,
{
    let (sine, cosine): (f32, f32) = rotation.sin_cos();
    let mut minimum: [f32; 2] = [f32::INFINITY; 2];
    let mut maximum: [f32; 2] = [f32::NEG_INFINITY; 2];
    for [x, y] in locals {
        for corner in [
            [x as f32 / 8.0, y as f32 / 8.0],
            [(x + 1) as f32 / 8.0, y as f32 / 8.0],
            [x as f32 / 8.0, (y + 1) as f32 / 8.0],
            [(x + 1) as f32 / 8.0, (y + 1) as f32 / 8.0],
        ] {
            let world_position: [f32; 2] = [
                position[0] + cosine * corner[0] - sine * corner[1],
                position[1] + sine * corner[0] + cosine * corner[1],
            ];
            minimum[0] = minimum[0].min(world_position[0]);
            minimum[1] = minimum[1].min(world_position[1]);
            maximum[0] = maximum[0].max(world_position[0]);
            maximum[1] = maximum[1].max(world_position[1]);
        }
    }
    minimum[0].is_finite().then_some((minimum, maximum))
}

pub(crate) fn owner_chunk(
    position: [f32; 2],
    rotation: f32,
    locals: impl IntoIterator<Item = [i32; 2]>,
) -> Option<crate::tiles::TileCoordinates> {
    let (minimum, maximum): ([f32; 2], [f32; 2]) = world_aabb(position, rotation, locals)?;
    Some(
        crate::tiles::TileCoordinates {
            x: ((minimum[0] + maximum[0]) * 0.5).floor() as i32,
            y: ((minimum[1] + maximum[1]) * 0.5).floor() as i32,
        }
        .chunk_coordinates(),
    )
}

pub(crate) fn intersects_area(bounds: ([f32; 2], [f32; 2]), area: crate::tiles::TileArea) -> bool {
    let (minimum, maximum): ([f32; 2], [f32; 2]) = bounds;
    let origin: crate::tiles::TileCoordinates = area.origin();
    let dimensions: [u16; 2] = area.dimensions();
    maximum[0] >= origin.x as f32
        && minimum[0] < (origin.x + i32::from(dimensions[0])) as f32
        && maximum[1] >= origin.y as f32
        && minimum[1] < (origin.y + i32::from(dimensions[1])) as f32
}

impl DormantRigidBody {
    const MAX_CELLS: usize = 1 << 20;

    pub(crate) fn validate(&self, materials: &MaterialRegistry) -> Result<(), io::Error> {
        if self.identifier == 0
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
        let mut locals: HashSet<[i32; 2]> = HashSet::with_capacity(self.cells.len());
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
        writer.write_all(&self.identifier.to_le_bytes())?;
        for value in self
            .position
            .into_iter()
            .chain([self.rotation])
            .chain(self.linear_velocity)
            .chain([self.angular_velocity])
        {
            writer.write_all(&value.to_bits().to_le_bytes())?;
        }
        writer.write_all(&(self.sleeping as u32).to_le_bytes())?;
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
        Self::deserialize_versioned(reader, materials, true)
    }

    pub(crate) fn deserialize_legacy<R: io::Read>(
        reader: &mut R,
        materials: &MaterialRegistry,
    ) -> Result<Self, io::Error> {
        Self::deserialize_versioned(reader, materials, false)
    }

    fn deserialize_versioned<R: io::Read>(
        reader: &mut R,
        materials: &MaterialRegistry,
        has_sleeping: bool,
    ) -> Result<Self, io::Error> {
        let read_u32: fn(&mut R) -> Result<u32, io::Error> = |reader: &mut R| {
            let mut bytes: [u8; 4] = [0; 4];
            reader.read_exact(&mut bytes)?;
            Ok(u32::from_le_bytes(bytes))
        };
        let mut identifier_bytes: [u8; 8] = [0; 8];
        reader.read_exact(&mut identifier_bytes)?;
        let position: [f32; 2] = [
            f32::from_bits(read_u32(reader)?),
            f32::from_bits(read_u32(reader)?),
        ];
        let rotation: f32 = f32::from_bits(read_u32(reader)?);
        let linear_velocity: [f32; 2] = [
            f32::from_bits(read_u32(reader)?),
            f32::from_bits(read_u32(reader)?),
        ];
        let angular_velocity: f32 = f32::from_bits(read_u32(reader)?);
        let sleeping: bool = if has_sleeping {
            match read_u32(reader)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid sleeping state",
                    ));
                }
            }
        } else {
            false
        };
        let count: usize = usize::try_from(read_u32(reader)?).map_err(|_| {
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
        let mut cells: Vec<DormantRigidCell> = Vec::new();
        cells.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dormant rigid allocation failed",
            )
        })?;
        for _ in 0..count {
            cells.push(DormantRigidCell {
                local: [
                    i32::from_le_bytes(read_u32(reader)?.to_le_bytes()),
                    i32::from_le_bytes(read_u32(reader)?.to_le_bytes()),
                ],
                material: MaterialIdentifier::from_u32(read_u32(reader)?),
                appearance: CellularAppearance(read_u32(reader)?),
                integrity: f32::from_bits(read_u32(reader)?),
                amount: f32::from_bits(read_u32(reader)?),
                temperature: f32::from_bits(read_u32(reader)?),
            });
        }
        let body: Self = Self {
            identifier: u64::from_le_bytes(identifier_bytes),
            position,
            rotation,
            linear_velocity,
            angular_velocity,
            sleeping,
            cells,
        };
        body.validate(materials)?;
        Ok(body)
    }
}
