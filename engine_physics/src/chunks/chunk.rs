// Copyright Rob Gage 2026

use super::{ChunkFluidParticle, ChunkGasCell};
use crate::{
    materials::MaterialIdentifier,
    tiles::{CellularAppearance, TileArea, TileCoordinates, TileData},
};
use std::io;

/// An inactive 64 tile by 64 tile chunk of a `Scene`
pub struct Chunk {
    /// The `TilePosition` of the bottom left tile in this `Chunk`
    pub tile_coordinates: TileCoordinates,
    /// The tiles in this `Chunk`,
    tiles: Box<[TileData]>,
    /// Sparse authoritative fluid particles outside GPU residency
    dormant_fluid_particles: Vec<ChunkFluidParticle>,
    /// Sparse authoritative gas cells outside GPU residency
    dormant_gas_cells: Vec<ChunkGasCell>,
}

impl Chunk {
    /// The width and height of a `Chunk` in tiles
    pub const WIDTH: u16 = 64;
    const LEGACY_MAGIC: [u8; 8] = *b"dogwood_";
    const CURRENT_MAGIC: [u8; 8] = *b"dogwd003";
    const V2_MAGIC: [u8; 8] = *b"dogwd002";

    /// Creates a new empty `Chunk`
    pub fn new_empty(tile_coordinates: TileCoordinates) -> Self {
        Self {
            tile_coordinates,
            tiles: (0..usize::from(Self::WIDTH) * usize::from(Self::WIDTH))
                .map(|_| TileData::EMPTY)
                .collect(),
            dormant_fluid_particles: Vec::new(),
            dormant_gas_cells: Vec::new(),
        }
    }

    /// Deserializes binary data into a `Chunk`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<Chunk, io::Error> {
        let mut magic: [u8; 8] = [0; 8];
        reader.read_exact(&mut magic)?;
        let legacy = magic == Self::LEGACY_MAGIC;
        let v2 = magic == Self::V2_MAGIC;
        if !legacy && !v2 && magic != Self::CURRENT_MAGIC {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut coordinate_data: [u8; 8] = [0; 8];
        reader.read_exact(&mut coordinate_data)?;
        let tile_coordinates: TileCoordinates = TileCoordinates {
            x: i32::from_le_bytes(coordinate_data[0..4].try_into().unwrap()),
            y: i32::from_le_bytes(coordinate_data[4..8].try_into().unwrap()),
        };
        let mut tile_data: Vec<TileData> = Vec::with_capacity(4096);
        for _ in 0..4096 {
            tile_data.push(if legacy {
                TileData::deserialize_legacy(reader)?
            } else {
                TileData::deserialize(reader)?
            });
        }
        let mut fluid_magic: [u8; 8] = [0; 8];
        if reader.read(&mut fluid_magic[..1])? == 0 {
            return Ok(Self {
                tile_coordinates,
                tiles: tile_data.into_boxed_slice(),
                dormant_fluid_particles: Vec::new(),
                dormant_gas_cells: Vec::new(),
            });
        }
        reader.read_exact(&mut fluid_magic[1..])?;
        if &fluid_magic != b"fluid___" {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut count_data: [u8; 4] = [0; 4];
        reader.read_exact(&mut count_data)?;
        let count: usize = u32::from_le_bytes(count_data) as usize;
        let mut dormant_fluid_particles: Vec<ChunkFluidParticle> = Vec::new();
        dormant_fluid_particles
            .try_reserve_exact(count)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dormant fluid particle count is too large",
                )
            })?;
        for _ in 0..count {
            let particle: ChunkFluidParticle = if legacy || v2 {
                ChunkFluidParticle::deserialize_legacy(reader)?
            } else {
                ChunkFluidParticle::deserialize(reader)?
            };
            if particle.tile_coordinates().chunk_coordinates() != tile_coordinates {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dormant fluid particle is outside its chunk",
                ));
            }
            dormant_fluid_particles.push(particle);
        }
        let mut gas_magic: [u8; 8] = [0; 8];
        if reader.read(&mut gas_magic[..1])? == 0 {
            return Ok(Self {
                tile_coordinates,
                tiles: tile_data.into_boxed_slice(),
                dormant_fluid_particles,
                dormant_gas_cells: Vec::new(),
            });
        }
        reader.read_exact(&mut gas_magic[1..])?;
        if &gas_magic != b"gas_____" {
            return Err(io::ErrorKind::InvalidData.into());
        }
        reader.read_exact(&mut count_data)?;
        let count: usize = u32::from_le_bytes(count_data) as usize;
        let mut dormant_gas_cells: Vec<ChunkGasCell> = Vec::new();
        dormant_gas_cells.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant gas cell count is too large",
            )
        })?;
        for _ in 0..count {
            let cell: ChunkGasCell = ChunkGasCell::deserialize(reader)?;
            if cell.tile_coordinates().chunk_coordinates() != tile_coordinates {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Dormant gas cell is outside its chunk",
                ));
            }
            dormant_gas_cells.push(cell);
        }
        Ok(Self {
            tile_coordinates,
            tiles: tile_data.into_boxed_slice(),
            dormant_fluid_particles,
            dormant_gas_cells,
        })
    }

    /// Serializes a `Chunk` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&Self::CURRENT_MAGIC)?;
        writer.write_all(&self.tile_coordinates.x.to_le_bytes())?;
        writer.write_all(&self.tile_coordinates.y.to_le_bytes())?;
        for tile in &self.tiles {
            tile.serialize(writer)?;
        }
        writer.write_all(b"fluid___")?;
        let count: u32 = self.dormant_fluid_particles.len().try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Too many dormant fluid particles",
            )
        })?;
        writer.write_all(&count.to_le_bytes())?;
        for particle in &self.dormant_fluid_particles {
            particle.serialize(writer)?;
        }
        writer.write_all(b"gas_____")?;
        let count: u32 = self.dormant_gas_cells.len().try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Too many dormant gas cells")
        })?;
        writer.write_all(&count.to_le_bytes())?;
        for cell in &self.dormant_gas_cells {
            cell.serialize(writer)?;
        }
        Ok(())
    }

    /// Returns the `TileData` at a given position in this `Chunk` if it is not out of bounds
    pub fn get_tile(&self, position: TileCoordinates) -> Result<&TileData, ()> {
        let x: usize = usize::try_from(position.x.checked_sub(self.tile_coordinates.x).ok_or(())?)
            .map_err(|_| ())?;
        let y: usize = usize::try_from(position.y.checked_sub(self.tile_coordinates.y).ok_or(())?)
            .map_err(|_| ())?;
        if x >= usize::from(Self::WIDTH) || y >= usize::from(Self::WIDTH) {
            return Err(());
        }
        Ok(&self.tiles[y * usize::from(Self::WIDTH) + x])
    }

    /// Returns the `TileData` at a given position in this `Chunk`, panicking if it is out of bounds
    pub fn get_tile_unchecked(&self, position: TileCoordinates) -> &TileData {
        self.get_tile(position).unwrap()
    }

    /// Returns mutable tile data when the tile lies within this chunk.
    pub(crate) fn get_tile_mut(&mut self, position: TileCoordinates) -> Result<&mut TileData, ()> {
        let x = usize::try_from(position.x.checked_sub(self.tile_coordinates.x).ok_or(())?)
            .map_err(|_| ())?;
        let y = usize::try_from(position.y.checked_sub(self.tile_coordinates.y).ok_or(())?)
            .map_err(|_| ())?;
        if x >= usize::from(Self::WIDTH) || y >= usize::from(Self::WIDTH) {
            return Err(());
        }
        Ok(&mut self.tiles[y * usize::from(Self::WIDTH) + x])
    }

    /// Sets a provided `TileData` at a given position in this `Chunk` if it is not out of bounds
    pub fn set_tile(&mut self, position: TileCoordinates, tile: TileData) -> Result<(), ()> {
        let x: usize = usize::try_from(position.x.checked_sub(self.tile_coordinates.x).ok_or(())?)
            .map_err(|_| ())?;
        let y: usize = usize::try_from(position.y.checked_sub(self.tile_coordinates.y).ok_or(())?)
            .map_err(|_| ())?;
        if x >= usize::from(Self::WIDTH) || y >= usize::from(Self::WIDTH) {
            return Err(());
        }
        self.tiles[y * usize::from(Self::WIDTH) + x] = tile;
        Ok(())
    }

    /// Sets a provided `TileData` at a given position in this `Chunk`,
    /// panicking if it is out of bounds
    pub fn set_tile_unchecked(&mut self, position: TileCoordinates, tile: TileData) {
        self.set_tile(position, tile).unwrap()
    }

    /// Sets one cell in a tile in this `Chunk` if the tile is not out of bounds
    pub fn set_cell(
        &mut self,
        position: TileCoordinates,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) -> Result<(), ()> {
        self.set_cell_with_integrity(position, x, y, material_identifier, appearance, 0.0)
    }

    /// Sets one cell with an explicit persistent integrity value
    pub fn set_cell_with_integrity(
        &mut self,
        position: TileCoordinates,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        integrity: f32,
    ) -> Result<(), ()> {
        let tile_x: usize =
            usize::try_from(position.x.checked_sub(self.tile_coordinates.x).ok_or(())?)
                .map_err(|_| ())?;
        let tile_y: usize =
            usize::try_from(position.y.checked_sub(self.tile_coordinates.y).ok_or(())?)
                .map_err(|_| ())?;
        if tile_x >= usize::from(Self::WIDTH) || tile_y >= usize::from(Self::WIDTH) {
            return Err(());
        }
        self.tiles[tile_y * usize::from(Self::WIDTH) + tile_x].set_cell_with_integrity(
            x,
            y,
            material_identifier,
            appearance,
            integrity,
        );
        Ok(())
    }

    /// Removes and returns dormant fluid belonging to an area
    pub fn take_dormant_fluid_particles(&mut self, area: TileArea) -> Vec<ChunkFluidParticle> {
        // ponytail: linear sparse scan; index by local tile if dormant chunk density becomes costly
        let mut particles: Vec<ChunkFluidParticle> = Vec::new();
        let mut index: usize = 0;
        while index < self.dormant_fluid_particles.len() {
            if area.contains(self.dormant_fluid_particles[index].tile_coordinates()) {
                particles.push(self.dormant_fluid_particles.swap_remove(index));
            } else {
                index += 1;
            }
        }
        particles
    }

    /// Adds an authoritative dormant particle according to its current world position
    pub fn insert_dormant_fluid_particle(
        &mut self,
        particle: ChunkFluidParticle,
    ) -> Result<(), ()> {
        if particle.tile_coordinates().chunk_coordinates() != self.tile_coordinates {
            return Err(());
        }
        self.dormant_fluid_particles.push(particle);
        Ok(())
    }

    /// Removes and returns dormant gas cells belonging to an area
    pub fn take_dormant_gas_cells(&mut self, area: TileArea) -> Vec<ChunkGasCell> {
        // ponytail: linear sparse scan; index by local tile if dormant gas density becomes costly
        let mut cells: Vec<ChunkGasCell> = Vec::new();
        let mut index: usize = 0;
        while index < self.dormant_gas_cells.len() {
            if area.contains(self.dormant_gas_cells[index].tile_coordinates()) {
                cells.push(self.dormant_gas_cells.swap_remove(index));
            } else {
                index += 1;
            }
        }
        cells
    }

    /// Adds one authoritative dormant gas cell to this chunk
    pub fn insert_dormant_gas_cell(&mut self, cell: ChunkGasCell) -> Result<(), ()> {
        if cell.tile_coordinates().chunk_coordinates() != self.tile_coordinates {
            return Err(());
        }
        self.dormant_gas_cells.push(cell);
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::materials::MaterialForm;

    #[test]
    fn dormant_fluid_round_trips_and_old_chunks_remain_readable() {
        let coordinates: TileCoordinates = TileCoordinates { x: -64, y: -64 };
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        let tile_coordinates = coordinates;
        let cellular = MaterialIdentifier::new(MaterialForm::CellularStatic, 1);
        let mut tile = TileData::EMPTY;
        tile.set_cell_with_integrity(2, 3, cellular, CellularAppearance(12), 0.4);
        tile.set_cell_state(2, 3, 0.37, 777.0);
        chunk.set_tile_unchecked(tile_coordinates, tile);
        let particle: ChunkFluidParticle = ChunkFluidParticle {
            material_identifier: MaterialIdentifier::new(MaterialForm::Fluid, 7),
            position: [-0.25, -63.5],
            velocity: [1.25, -2.5],
            amount: 0.37,
            temperature: 777.0,
        };
        chunk.insert_dormant_fluid_particle(particle).unwrap();
        let gas_cell: ChunkGasCell = ChunkGasCell {
            coordinates: crate::tiles::CellCoordinates { x: -2, y: -510 },
            velocity: [0.5, 1.25],
            species: vec![(MaterialIdentifier::new(MaterialForm::Gas, 0), 0.75)],
        };
        chunk.insert_dormant_gas_cell(gas_cell.clone()).unwrap();
        let mut bytes: Vec<u8> = Vec::new();
        chunk.serialize(&mut bytes).unwrap();
        let mut reader: &[u8] = &bytes;
        let mut loaded: Chunk = Chunk::deserialize(&mut reader).unwrap();
        let loaded_tile = loaded.get_tile_unchecked(tile_coordinates);
        assert_eq!(loaded_tile.cell_amount(2, 3), 0.37);
        assert_eq!(loaded_tile.cell_temperature(2, 3), 777.0);
        let loaded_particles: Vec<ChunkFluidParticle> = loaded
            .take_dormant_fluid_particles(TileArea::new(TileCoordinates { x: -1, y: -64 }, 1, 1));
        assert!(loaded_particles.len() == 1);
        assert!(loaded_particles[0].material_identifier == particle.material_identifier);
        assert!(loaded_particles[0].position == particle.position);
        assert!(loaded_particles[0].velocity == particle.velocity);
        let loaded_gas: Vec<ChunkGasCell> =
            loaded.take_dormant_gas_cells(TileArea::new(TileCoordinates { x: -1, y: -64 }, 1, 1));
        assert!(loaded_gas.len() == 1);
        assert!(loaded_gas[0].coordinates == gas_cell.coordinates);
        assert!(loaded_gas[0].velocity == gas_cell.velocity);
        assert!(loaded_gas[0].species == gas_cell.species);

        let gas_section_offset: usize =
            16 + 4096 * TileData::SERIALIZED_SIZE + 8 + 4 + ChunkFluidParticle::SERIALIZED_SIZE;
        let mut fluid_only_bytes: Vec<u8> = bytes.clone();
        fluid_only_bytes.truncate(gas_section_offset);
        let mut fluid_only_reader: &[u8] = &fluid_only_bytes;
        let fluid_only_chunk: Chunk = Chunk::deserialize(&mut fluid_only_reader).unwrap();
        assert!(fluid_only_chunk.dormant_gas_cells.is_empty());

        bytes.truncate(16 + 4096 * TileData::SERIALIZED_SIZE);
        let mut old_reader: &[u8] = &bytes;
        let old_chunk: Chunk = Chunk::deserialize(&mut old_reader).unwrap();
        assert!(old_chunk.dormant_fluid_particles.is_empty());
        assert!(old_chunk.dormant_gas_cells.is_empty());
    }

    #[test]
    fn legacy_cellular_tiles_load_with_unresolved_temperature() {
        let coordinates = TileCoordinates { x: 0, y: 0 };
        let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
        let mut tile = TileData::EMPTY;
        tile.set_cell(0, 0, material, CellularAppearance(9));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"dogwood_");
        bytes.extend_from_slice(&coordinates.x.to_le_bytes());
        bytes.extend_from_slice(&coordinates.y.to_le_bytes());
        tile.serialize_material_identifiers(&mut bytes).unwrap();
        tile.serialize_appearances(&mut bytes).unwrap();
        tile.serialize_integrities(&mut bytes).unwrap();
        for _ in 1..4096 {
            TileData::EMPTY
                .serialize_material_identifiers(&mut bytes)
                .unwrap();
            TileData::EMPTY.serialize_appearances(&mut bytes).unwrap();
            TileData::EMPTY.serialize_integrities(&mut bytes).unwrap();
        }
        let chunk = Chunk::deserialize(&mut bytes.as_slice()).unwrap();
        let loaded = chunk.get_tile_unchecked(coordinates);
        assert_eq!(loaded.cell_amount(0, 0), 1.0);
        assert!(loaded.cell_temperature(0, 0).is_nan());
        assert_eq!(loaded.cell_amount(1, 0), 0.0);
        assert_eq!(loaded.cell_temperature(1, 0), 0.0);
    }
}
