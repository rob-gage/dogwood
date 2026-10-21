// Copyright Rob Gage 2026

use super::{
    ChunkError,
    ChunkCoordinates,
    CHUNK_SIZE,
};
use crate::{
    scenes::cellular_tiles::{
        CellularTileCoordinates,
        CELLULAR_TILE_SIZE,
    },
    materials::{
        MaterialForm,
        MaterialIdentifier,
    },
};
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    io,
};

/// Magic bytes identifying persisted world chunks
const CHUNK_FILE_HEADER: [u8; 8] = *b"DOGWOOD\0";

/// Current sparse chunk file format version
const CHUNK_FILE_VERSION: u16 = 6;

/// Bytes in one sparse cell entry holding its index, material identifier, color, and kinematics
const CHUNK_FILE_CELL_BYTE_COUNT: usize = 20;

/// Bytes in one persisted fluid particle record
const CHUNK_FILE_FLUID_PARTICLE_BYTE_COUNT: usize = 20;

/// Number of cells addressable by one chunk
const CHUNK_CELL_COUNT: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Sparse CPU cell storage for one chunk
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    /// World coordinates identifying this chunk
    pub coordinates: ChunkCoordinates,
    /// Material identifiers in row-major cell order
    material_identifiers: Vec<MaterialIdentifier>,
    /// Packed RGBA colors in row-major cell order
    colors: Vec<u32>,
    /// Packed velocities and timing of dynamic cells in row-major cell order
    kinematics: Vec<u64>,
    /// Sparse fluid particles stored relative to this chunk's origin
    fluid_particles: Vec<ChunkFluidParticle>,
}

/// Exact bit representation of one persisted fluid particle
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChunkFluidParticle {
    /// IEEE-754 position bits relative to the chunk origin, in cells
    position_bits: [u32; 2],
    /// IEEE-754 velocity bits, in cells per tick
    velocity_bits: [u32; 2],
    /// Fluid material identifier
    material_identifier: MaterialIdentifier,
}

impl ChunkFluidParticle {

    /// Creates a particle record while preserving the exact floating-point bits
    pub fn new(
        position: [f32; 2],
        velocity: [f32; 2],
        material_identifier: MaterialIdentifier,
    ) -> Result<Self, ChunkError> {
        let particle: Self = Self {
            position_bits: [position[0].to_bits(), position[1].to_bits()],
            velocity_bits: [velocity[0].to_bits(), velocity[1].to_bits()],
            material_identifier,
        };
        particle.validate()?;
        Ok(particle)
    }

    /// Returns the particle position relative to the chunk origin, in cells
    pub fn position(self) -> [f32; 2] {
        [f32::from_bits(self.position_bits[0]), f32::from_bits(self.position_bits[1])]
    }

    /// Returns the particle velocity, in cells per tick
    pub fn velocity(self) -> [f32; 2] {
        [f32::from_bits(self.velocity_bits[0]), f32::from_bits(self.velocity_bits[1])]
    }

    /// Returns the fluid material identifier
    pub fn material_identifier(self) -> MaterialIdentifier { self.material_identifier }

    /// Checks that a persisted particle fits its chunk and uses a fluid material
    fn validate(self) -> Result<(), ChunkError> {
        let position: [f32; 2] = self.position();
        let velocity: [f32; 2] = self.velocity();
        if position.iter().any(|value| {
            !value.is_finite() || *value < 0.0 || *value >= CHUNK_SIZE as f32
        }) { return Err(ChunkError::FluidParticlePositionInvalid); }
        if velocity.iter().any(|value| !value.is_finite()) {
            return Err(ChunkError::FluidParticleVelocityInvalid);
        }
        if self.material_identifier == MaterialIdentifier::EMPTY
            || self.material_identifier.form() != MaterialForm::Fluid
        { return Err(ChunkError::FluidParticleMaterialInvalid); }
        Ok(())
    }

}

impl Chunk {

    /// Creates an empty chunk at the given coordinates
    pub fn new(coordinates: ChunkCoordinates) -> Self {
        Self {
            coordinates,
            material_identifiers: vec![MaterialIdentifier::EMPTY; CHUNK_CELL_COUNT],
            colors: vec![0; CHUNK_CELL_COUNT],
            kinematics: vec![0; CHUNK_CELL_COUNT],
            fluid_particles: Vec::new(),
        }
    }

    /// Fills every cell in this chunk with one material and packed color, at rest
    pub fn fill(&mut self, material_identifier: MaterialIdentifier, color: u32) {
        self.material_identifiers.fill(material_identifier);
        self.colors.fill(color);
        self.kinematics.fill(0);
        self.fluid_particles.clear();
    }

    /// Reads a versioned sparse chunk from a reader
    pub fn read<R: io::Read>(r: &mut R) -> Result<Self, ChunkError> {
        let mut header: [u8; 22] = [0; 22];
        r.read_exact(&mut header).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        // validate the fixed header before trusting its entry count
        if header[0..8] != CHUNK_FILE_HEADER { return Err(ChunkError::InvalidHeader); }
        let version: u16 = u16::from_le_bytes([header[8], header[9]]);
        if version != CHUNK_FILE_VERSION { return Err(ChunkError::InvalidVersion); }
        let x: i32 = i32::from_le_bytes([header[10], header[11], header[12], header[13]]);
        let y: i32 = i32::from_le_bytes([header[14], header[15], header[16], header[17]]);
        let coordinates: ChunkCoordinates = ChunkCoordinates { x, y };
        let stored_cell_count: usize = u32::from_le_bytes([
            header[18],
            header[19],
            header[20],
            header[21],
        ]) as usize;
        if stored_cell_count > CHUNK_CELL_COUNT { return Err(ChunkError::CellCapacityExceeded); }
        let mut encoded_cells: Vec<u8> = vec![0; stored_cell_count * CHUNK_FILE_CELL_BYTE_COUNT];
        r.read_exact(&mut encoded_cells).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let mut particle_count_bytes: [u8; 4] = [0; 4];
        r.read_exact(&mut particle_count_bytes).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let particle_count: usize = u32::from_le_bytes(particle_count_bytes) as usize;
        if particle_count > CHUNK_CELL_COUNT {
            return Err(ChunkError::FluidParticleCapacityExceeded);
        }
        let mut encoded_particles: Vec<u8> = vec![
            0;
            particle_count * CHUNK_FILE_FLUID_PARTICLE_BYTE_COUNT
        ];
        r.read_exact(&mut encoded_particles).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let mut trailing_byte: [u8; 1] = [0];
        if r.read(&mut trailing_byte).map_err(ChunkError::Io)? != 0 {
            return Err(ChunkError::InvalidLength);
        }
        let mut chunk: Self = Self::new(coordinates);
        // expand sparse entries into the chunk's row-major cell arrays
        let mut is_cell_stored: Vec<bool> = vec![false; CHUNK_CELL_COUNT];
        for encoded_cell in encoded_cells.chunks_exact(CHUNK_FILE_CELL_BYTE_COUNT) {
            let cell_index: u32 = u32::from_le_bytes([
                encoded_cell[0],
                encoded_cell[1],
                encoded_cell[2],
                encoded_cell[3],
            ]);
            let identifier_value: u32 = u32::from_le_bytes([
                encoded_cell[4],
                encoded_cell[5],
                encoded_cell[6],
                encoded_cell[7],
            ]);
            let color: u32 = u32::from_le_bytes([
                encoded_cell[8],
                encoded_cell[9],
                encoded_cell[10],
                encoded_cell[11],
            ]);
            let kinematics: u64 = u64::from_le_bytes([
                encoded_cell[12],
                encoded_cell[13],
                encoded_cell[14],
                encoded_cell[15],
                encoded_cell[16],
                encoded_cell[17],
                encoded_cell[18],
                encoded_cell[19],
            ]);
            if cell_index as usize >= CHUNK_CELL_COUNT {
                return Err(ChunkError::CellIndexOutOfRange);
            }
            let identifier: MaterialIdentifier = MaterialIdentifier::from_u32(identifier_value);
            if identifier == MaterialIdentifier::EMPTY && color == 0 {
                return Err(ChunkError::CellIsEmpty);
            }
            if is_cell_stored[cell_index as usize] {
                return Err(ChunkError::CellIndexIsDuplicate);
            }
            is_cell_stored[cell_index as usize] = true;
            chunk.material_identifiers[cell_index as usize] = identifier;
            chunk.colors[cell_index as usize] = color;
            chunk.kinematics[cell_index as usize] = kinematics;
        }
        for encoded_particle in encoded_particles.chunks_exact(
            CHUNK_FILE_FLUID_PARTICLE_BYTE_COUNT,
        ) {
            let read_u32 = |offset: usize| -> u32 {
                u32::from_le_bytes([
                    encoded_particle[offset],
                    encoded_particle[offset + 1],
                    encoded_particle[offset + 2],
                    encoded_particle[offset + 3],
                ])
            };
            let particle: ChunkFluidParticle = ChunkFluidParticle {
                position_bits: [read_u32(0), read_u32(4)],
                velocity_bits: [read_u32(8), read_u32(12)],
                material_identifier: MaterialIdentifier::from_u32(read_u32(16)),
            };
            particle.validate()?;
            chunk.fluid_particles.push(particle);
        }
        Ok(chunk)
    }

    /// Writes this chunk in the current versioned sparse format
    pub fn write<W: io::Write>(&self, w: &mut W) -> Result<(), ChunkError> {
        let stored_cell_count: u32 = self.material_identifiers.iter()
            .zip(self.colors.iter())
            .filter(|(material, color)| **material != MaterialIdentifier::EMPTY || **color != 0)
            .count() as u32;
        // write the format header before the sparse cell entries
        let mut header: Vec<u8> = Vec::with_capacity(22);
        header.extend_from_slice(&CHUNK_FILE_HEADER);
        header.extend_from_slice(&CHUNK_FILE_VERSION.to_le_bytes());
        header.extend_from_slice(&self.coordinates.x.to_le_bytes());
        header.extend_from_slice(&self.coordinates.y.to_le_bytes());
        header.extend_from_slice(&stored_cell_count.to_le_bytes());
        w.write_all(&header).map_err(ChunkError::Io)?;
        let mut encoded_cells: Vec<u8> =
            Vec::with_capacity(stored_cell_count as usize * CHUNK_FILE_CELL_BYTE_COUNT);
        for (cell_index, ((material, color), kinematics)) in self.material_identifiers.iter()
            .zip(self.colors.iter())
            .zip(self.kinematics.iter())
            .enumerate()
        {
            if *material == MaterialIdentifier::EMPTY && *color == 0 { continue; }
            let cell_index: u32 = cell_index as u32;
            encoded_cells.extend_from_slice(&cell_index.to_le_bytes());
            encoded_cells.extend_from_slice(&material.as_u32().to_le_bytes());
            encoded_cells.extend_from_slice(&color.to_le_bytes());
            encoded_cells.extend_from_slice(&kinematics.to_le_bytes());
        }
        w.write_all(&encoded_cells).map_err(ChunkError::Io)?;
        let particle_count: u32 = self.fluid_particles.len().try_into()
            .map_err(|_| ChunkError::FluidParticleCapacityExceeded)?;
        if self.fluid_particles.len() > CHUNK_CELL_COUNT {
            return Err(ChunkError::FluidParticleCapacityExceeded);
        }
        w.write_all(&particle_count.to_le_bytes()).map_err(ChunkError::Io)?;
        let mut encoded_particles: Vec<u8> = Vec::with_capacity(
            self.fluid_particles.len() * CHUNK_FILE_FLUID_PARTICLE_BYTE_COUNT,
        );
        for particle in &self.fluid_particles {
            particle.validate()?;
            for value in particle.position_bits
                .into_iter()
                .chain(particle.velocity_bits)
                .chain([particle.material_identifier.as_u32()])
            {
                encoded_particles.extend_from_slice(&value.to_le_bytes());
            }
        }
        w.write_all(&encoded_particles).map_err(ChunkError::Io)
    }

    /// Returns the sparse fluid particles stored in this chunk
    pub fn fluid_particles(&self) -> &[ChunkFluidParticle] { &self.fluid_particles }

    /// Replaces the sparse fluid particle list after validating its records
    pub fn set_fluid_particles(
        &mut self,
        fluid_particles: Vec<ChunkFluidParticle>,
    ) -> Result<(), ChunkError> {
        if fluid_particles.len() > CHUNK_CELL_COUNT {
            return Err(ChunkError::FluidParticleCapacityExceeded);
        }
        for particle in &fluid_particles { particle.validate()?; }
        self.fluid_particles = fluid_particles;
        Ok(())
    }

    /// Removes one matching copy of each supplied particle record
    pub(crate) fn remove_fluid_particle_records(
        &mut self,
        fluid_particles: &[ChunkFluidParticle],
    ) {
        for fluid_particle in fluid_particles {
            if let Some(index) = self.fluid_particles.iter().position(|stored| {
                stored == fluid_particle
            }) {
                self.fluid_particles.remove(index);
            }
        }
    }

    /// Removes particles whose cells lie in selected tiles, taking whole tiles up to a bounded
    /// count unless one tile alone exceeds it
    pub(crate) fn take_fluid_particles_in_tiles(
        &mut self,
        coordinates: &[CellularTileCoordinates],
        maximum: usize,
    ) -> Vec<ChunkFluidParticle> {
        let chunk_tile_origin_x: i64 = i64::from(self.coordinates.x)
            * (CHUNK_SIZE / CELLULAR_TILE_SIZE);
        let chunk_tile_origin_y: i64 = i64::from(self.coordinates.y)
            * (CHUNK_SIZE / CELLULAR_TILE_SIZE);
        let tile = |particle: &ChunkFluidParticle| -> (i64, i64) {
            let position: [f32; 2] = particle.position();
            (
                chunk_tile_origin_x + (position[0].floor() as i64).div_euclid(CELLULAR_TILE_SIZE),
                chunk_tile_origin_y + (position[1].floor() as i64).div_euclid(CELLULAR_TILE_SIZE),
            )
        };
        let mut tile_counts: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        for particle in &self.fluid_particles {
            let (tile_x, tile_y): (i64, i64) = tile(particle);
            if coordinates.iter().any(|tile| tile.x == tile_x && tile.y == tile_y) {
                *tile_counts.entry((tile_x, tile_y)).or_default() += 1;
            }
        }
        // take whole tiles, so records sharing a cell enter in one batch and never reject each
        // other
        let mut selected_tiles: BTreeSet<(i64, i64)> = BTreeSet::new();
        let mut selected_count: usize = 0;
        for (tile_coordinates, count) in tile_counts {
            if selected_count > 0 && selected_count + count > maximum { break; }
            selected_tiles.insert(tile_coordinates);
            selected_count += count;
        }
        let mut selected: Vec<ChunkFluidParticle> = Vec::with_capacity(selected_count);
        let mut retained: Vec<ChunkFluidParticle> = Vec::with_capacity(self.fluid_particles.len());
        for particle in self.fluid_particles.drain(..) {
            if selected_tiles.contains(&tile(&particle)) {
                selected.push(particle);
            } else {
                retained.push(particle);
            }
        }
        self.fluid_particles = retained;
        selected
    }

    /// Returns the material identifiers of one cellular tile within this chunk
    pub fn cellular_tile_material_identifiers(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<[MaterialIdentifier; 64], ChunkError> {
        let mut material_identifiers: [MaterialIdentifier; 64] = [MaterialIdentifier::EMPTY; 64];
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            material_identifiers[tile_row * 8..tile_row * 8 + 8]
                .copy_from_slice(&self.material_identifiers[chunk_row_start..chunk_row_start + 8]);
        }
        Ok(material_identifiers)
    }

    /// Returns the packed RGBA colors of one cellular tile within this chunk
    pub fn cellular_tile_colors(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<[u32; 64], ChunkError> {
        let mut colors: [u32; 64] = [0; 64];
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            colors[tile_row * 8..tile_row * 8 + 8]
                .copy_from_slice(&self.colors[chunk_row_start..chunk_row_start + 8]);
        }
        Ok(colors)
    }

    /// Returns the packed kinematics of one cellular tile within this chunk
    pub fn cellular_tile_kinematics(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<[u64; 64], ChunkError> {
        let mut kinematics: [u64; 64] = [0; 64];
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            kinematics[tile_row * 8..tile_row * 8 + 8]
                .copy_from_slice(&self.kinematics[chunk_row_start..chunk_row_start + 8]);
        }
        Ok(kinematics)
    }

    /// Replaces the material identifiers of one cellular tile within this chunk
    pub fn set_cellular_tile_material_identifiers(
        &mut self,
        coordinates: CellularTileCoordinates,
        material_identifiers: &[MaterialIdentifier; 64],
    ) -> Result<(), ChunkError> {
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            self.material_identifiers[chunk_row_start..chunk_row_start + 8]
                .copy_from_slice(&material_identifiers[tile_row * 8..tile_row * 8 + 8]);
        }
        Ok(())
    }

    /// Replaces the packed RGBA colors of one cellular tile within this chunk
    pub fn set_cellular_tile_colors(
        &mut self,
        coordinates: CellularTileCoordinates,
        colors: &[u32; 64],
    ) -> Result<(), ChunkError> {
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            self.colors[chunk_row_start..chunk_row_start + 8]
                .copy_from_slice(&colors[tile_row * 8..tile_row * 8 + 8]);
        }
        Ok(())
    }

    /// Replaces the packed kinematics of one cellular tile within this chunk
    pub fn set_cellular_tile_kinematics(
        &mut self,
        coordinates: CellularTileCoordinates,
        kinematics: &[u64; 64],
    ) -> Result<(), ChunkError> {
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            self.kinematics[chunk_row_start..chunk_row_start + 8]
                .copy_from_slice(&kinematics[tile_row * 8..tile_row * 8 + 8]);
        }
        Ok(())
    }

    /// Returns the chunk cell index starting each row of one cellular tile within this chunk
    fn cellular_tile_row_starts(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<[usize; 8], ChunkError> {
        if coordinates.chunk_coordinates() != self.coordinates {
            return Err(ChunkError::CoordinateMismatch);
        }
        let cellular_tile_size: usize = CELLULAR_TILE_SIZE as usize;
        let tile_x: usize = coordinates.x.rem_euclid(CHUNK_SIZE / CELLULAR_TILE_SIZE) as usize;
        let tile_y: usize = coordinates.y.rem_euclid(CHUNK_SIZE / CELLULAR_TILE_SIZE) as usize;
        let mut row_starts: [usize; 8] = [0; 8];
        for (tile_row, row_start) in row_starts.iter_mut().enumerate() {
            let chunk_cell_y: usize = tile_y * cellular_tile_size + tile_row;
            *row_start = chunk_cell_y * CHUNK_SIZE as usize + tile_x * cellular_tile_size;
        }
        Ok(row_starts)
    }

}
