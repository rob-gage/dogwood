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
    /// Sparse authoritative fluid particles outside Accelerator residency
    dormant_fluid_particles: Vec<ChunkFluidParticle>,
    /// Sparse authoritative gas cells outside Accelerator residency
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

    /// Resolves authored and legacy occupied cells before this chunk becomes active.
    pub(crate) fn resolve_uninitialized_temperatures(
        &mut self,
        initial_temperature: impl Fn(MaterialIdentifier) -> f32,
    ) {
        for tile in &mut self.tiles {
            tile.resolve_uninitialized_temperatures(&initial_temperature);
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
            let cell: ChunkGasCell = if legacy || v2 {
                ChunkGasCell::deserialize_legacy(reader)?
            } else {
                ChunkGasCell::deserialize(reader)?
            };
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
