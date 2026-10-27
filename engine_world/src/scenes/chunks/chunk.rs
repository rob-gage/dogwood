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
const CHUNK_FILE_VERSION: u16 = 9;

/// Bytes in one sparse cell entry holding its index, material identifier, color, radiance, and kinematics
const CHUNK_FILE_CELL_BYTE_COUNT: usize = 24;

/// Bytes in one persisted fluid particle record with color, radiance, temperature, and amount
const CHUNK_FILE_FLUID_PARTICLE_BYTE_COUNT: usize = 36;

/// Bytes in one persisted cellular tile gas record
const CHUNK_FILE_GAS_TILE_BYTE_COUNT: usize = 4 + 512 * 4;

/// Bytes in one persisted cellular tile matter record
const CHUNK_FILE_MATTER_TILE_BYTE_COUNT: usize = 4 + 128 * 4;

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
    /// Packed radiance values in row-major cell order
    radiances: Vec<u32>,
    /// Packed velocities and timing of dynamic cells in row-major cell order
    kinematics: Vec<u64>,
    /// Sparse fluid particles stored relative to this chunk's origin
    fluid_particles: Vec<ChunkFluidParticle>,
    /// Gas cells of each cellular tile whose gas differs from the default, keyed by row-major tile
    /// index within the chunk, with eight words per row-major cell
    gas_cellular_tiles: BTreeMap<u32, Box<[u32; 512]>>,
    /// Temperature bits and amount of each cell of each cellular tile whose matter differs from the
    /// default, keyed by row-major tile index within the chunk, with two words per row-major cell
    matter_cellular_tiles: BTreeMap<u32, Box<[u32; 128]>>,
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
    /// Packed sRGB RGBA color
    color: u32,
    /// Packed linear RGBA16F radiance
    radiance: u32,
    /// IEEE-754 temperature bits in kelvin
    temperature_bits: u32,
    /// Amount in units of `CELLULAR_TILE_AMOUNT_PER_UNIT`
    amount: u32,
}

impl ChunkFluidParticle {

    /// Creates a particle record while preserving the exact floating-point bits
    pub fn new(
        position: [f32; 2],
        velocity: [f32; 2],
        material_identifier: MaterialIdentifier,
        color: u32,
        radiance: u32,
        temperature: f32,
        amount: u32,
    ) -> Result<Self, ChunkError> {
        let particle: Self = Self {
            position_bits: [position[0].to_bits(), position[1].to_bits()],
            velocity_bits: [velocity[0].to_bits(), velocity[1].to_bits()],
            material_identifier,
            color,
            radiance,
            temperature_bits: temperature.to_bits(),
            amount,
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

    /// Returns the packed sRGB RGBA color
    pub fn color(self) -> u32 { self.color }

    /// Returns the packed linear RGBA16F radiance
    pub fn radiance(self) -> u32 { self.radiance }

    /// Returns the particle temperature in kelvin
    pub fn temperature(self) -> f32 { f32::from_bits(self.temperature_bits) }

    /// Returns the amount in units of `CELLULAR_TILE_AMOUNT_PER_UNIT`
    pub fn amount(self) -> u32 { self.amount }

    /// Checks that a persisted particle fits its chunk, uses a fluid material, and has a valid
    /// temperature
    fn validate(self) -> Result<(), ChunkError> {
        let position: [f32; 2] = self.position();
        let velocity: [f32; 2] = self.velocity();
        let temperature: f32 = self.temperature();
        if position.iter().any(|value| {
            !value.is_finite() || *value < 0.0 || *value >= CHUNK_SIZE as f32
        }) { return Err(ChunkError::FluidParticlePositionInvalid); }
        if velocity.iter().any(|value| !value.is_finite()) {
            return Err(ChunkError::FluidParticleVelocityInvalid);
        }
        if !temperature.is_finite() || temperature < 0.0 {
            return Err(ChunkError::FluidParticleTemperatureInvalid);
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
            radiances: vec![0; CHUNK_CELL_COUNT],
            kinematics: vec![0; CHUNK_CELL_COUNT],
            fluid_particles: Vec::new(),
            gas_cellular_tiles: BTreeMap::new(),
            matter_cellular_tiles: BTreeMap::new(),
        }
    }

    /// Fills every cell in this chunk with one material, packed color and radiance, at rest
    pub fn fill(&mut self, material_identifier: MaterialIdentifier, color: u32, radiance: u32) {
        self.material_identifiers.fill(material_identifier);
        self.colors.fill(color);
        self.radiances.fill(radiance);
        self.kinematics.fill(0);
        self.fluid_particles.clear();
        self.gas_cellular_tiles.clear();
        self.matter_cellular_tiles.clear();
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
        let mut gas_tile_count_bytes: [u8; 4] = [0; 4];
        r.read_exact(&mut gas_tile_count_bytes).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let gas_tile_count: usize = u32::from_le_bytes(gas_tile_count_bytes) as usize;
        if gas_tile_count > 4096 { return Err(ChunkError::GasTileCapacityExceeded); }
        let mut encoded_gas_tiles: Vec<u8> = vec![
            0;
            gas_tile_count * CHUNK_FILE_GAS_TILE_BYTE_COUNT
        ];
        r.read_exact(&mut encoded_gas_tiles).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let mut matter_tile_count_bytes: [u8; 4] = [0; 4];
        r.read_exact(&mut matter_tile_count_bytes).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ChunkError::InvalidLength
            } else {
                ChunkError::Io(error)
            }
        })?;
        let matter_tile_count: usize = u32::from_le_bytes(matter_tile_count_bytes) as usize;
        if matter_tile_count > 4096 { return Err(ChunkError::MatterTileCapacityExceeded); }
        let mut encoded_matter_tiles: Vec<u8> = vec![
            0;
            matter_tile_count * CHUNK_FILE_MATTER_TILE_BYTE_COUNT
        ];
        r.read_exact(&mut encoded_matter_tiles).map_err(|error| {
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
            let radiance: u32 = u32::from_le_bytes([
                encoded_cell[12],
                encoded_cell[13],
                encoded_cell[14],
                encoded_cell[15],
            ]);
            let kinematics: u64 = u64::from_le_bytes([
                encoded_cell[16],
                encoded_cell[17],
                encoded_cell[18],
                encoded_cell[19],
                encoded_cell[20],
                encoded_cell[21],
                encoded_cell[22],
                encoded_cell[23],
            ]);
            if cell_index as usize >= CHUNK_CELL_COUNT {
                return Err(ChunkError::CellIndexOutOfRange);
            }
            let identifier: MaterialIdentifier = MaterialIdentifier::from_u32(identifier_value);
            if identifier == MaterialIdentifier::EMPTY && color == 0 && radiance == 0 {
                return Err(ChunkError::CellIsEmpty);
            }
            if is_cell_stored[cell_index as usize] {
                return Err(ChunkError::CellIndexIsDuplicate);
            }
            is_cell_stored[cell_index as usize] = true;
            chunk.material_identifiers[cell_index as usize] = identifier;
            chunk.colors[cell_index as usize] = color;
            chunk.radiances[cell_index as usize] = radiance;
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
                color: read_u32(20),
                radiance: read_u32(24),
                temperature_bits: read_u32(28),
                amount: read_u32(32),
            };
            particle.validate()?;
            chunk.fluid_particles.push(particle);
        }
        for encoded_gas_tile in encoded_gas_tiles.chunks_exact(
            CHUNK_FILE_GAS_TILE_BYTE_COUNT,
        ) {
            let tile_index: u32 = u32::from_le_bytes([
                encoded_gas_tile[0],
                encoded_gas_tile[1],
                encoded_gas_tile[2],
                encoded_gas_tile[3],
            ]);
            if tile_index >= 4096 { return Err(ChunkError::GasTileIndexOutOfRange); }
            if chunk.gas_cellular_tiles.contains_key(&tile_index) {
                return Err(ChunkError::GasTileIndexIsDuplicate);
            }
            let mut gas_cells: [u32; 512] = [0; 512];
            for (cell_index, gas_cell) in gas_cells.iter_mut().enumerate() {
                let offset: usize = 4 + cell_index * 4;
                *gas_cell = u32::from_le_bytes([
                    encoded_gas_tile[offset],
                    encoded_gas_tile[offset + 1],
                    encoded_gas_tile[offset + 2],
                    encoded_gas_tile[offset + 3],
                ]);
            }
            chunk.gas_cellular_tiles.insert(tile_index, Box::new(gas_cells));
        }
        for encoded_matter_tile in encoded_matter_tiles.chunks_exact(
            CHUNK_FILE_MATTER_TILE_BYTE_COUNT,
        ) {
            let tile_index: u32 = u32::from_le_bytes([
                encoded_matter_tile[0],
                encoded_matter_tile[1],
                encoded_matter_tile[2],
                encoded_matter_tile[3],
            ]);
            if tile_index >= 4096 { return Err(ChunkError::MatterTileIndexOutOfRange); }
            if chunk.matter_cellular_tiles.contains_key(&tile_index) {
                return Err(ChunkError::MatterTileIndexIsDuplicate);
            }
            let mut matter_cells: [u32; 128] = [0; 128];
            for (cell_index, matter_cell) in matter_cells.iter_mut().enumerate() {
                let offset: usize = 4 + cell_index * 4;
                *matter_cell = u32::from_le_bytes([
                    encoded_matter_tile[offset],
                    encoded_matter_tile[offset + 1],
                    encoded_matter_tile[offset + 2],
                    encoded_matter_tile[offset + 3],
                ]);
            }
            chunk.matter_cellular_tiles.insert(tile_index, Box::new(matter_cells));
        }
        Ok(chunk)
    }

    /// Writes this chunk in the current versioned sparse format
    pub fn write<W: io::Write>(&self, w: &mut W) -> Result<(), ChunkError> {
        let stored_cell_count: u32 = self.material_identifiers.iter()
            .zip(self.colors.iter())
            .zip(self.radiances.iter())
            .filter(|((material, color), radiance)| **material != MaterialIdentifier::EMPTY || **color != 0 || **radiance != 0)
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
        for (cell_index, (((material, color), radiance), kinematics)) in self.material_identifiers.iter()
            .zip(self.colors.iter())
            .zip(self.radiances.iter())
            .zip(self.kinematics.iter())
            .enumerate()
        {
            if *material == MaterialIdentifier::EMPTY && *color == 0 && *radiance == 0 { continue; }
            let cell_index: u32 = cell_index as u32;
            encoded_cells.extend_from_slice(&cell_index.to_le_bytes());
            encoded_cells.extend_from_slice(&material.as_u32().to_le_bytes());
            encoded_cells.extend_from_slice(&color.to_le_bytes());
            encoded_cells.extend_from_slice(&radiance.to_le_bytes());
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
                .chain([
                    particle.material_identifier.as_u32(),
                    particle.color,
                    particle.radiance,
                    particle.temperature_bits,
                    particle.amount,
                ])
            {
                encoded_particles.extend_from_slice(&value.to_le_bytes());
            }
        }
        w.write_all(&encoded_particles).map_err(ChunkError::Io)?;
        let gas_tile_count: u32 = self.gas_cellular_tiles.len().try_into()
            .map_err(|_| ChunkError::GasTileCapacityExceeded)?;
        if self.gas_cellular_tiles.len() > 4096 {
            return Err(ChunkError::GasTileCapacityExceeded);
        }
        w.write_all(&gas_tile_count.to_le_bytes()).map_err(ChunkError::Io)?;
        let mut encoded_gas_tiles: Vec<u8> = Vec::with_capacity(
            self.gas_cellular_tiles.len() * CHUNK_FILE_GAS_TILE_BYTE_COUNT,
        );
        for (tile_index, gas_cells) in &self.gas_cellular_tiles {
            encoded_gas_tiles.extend_from_slice(&tile_index.to_le_bytes());
            for gas_cell in gas_cells.iter() {
                encoded_gas_tiles.extend_from_slice(&gas_cell.to_le_bytes());
            }
        }
        w.write_all(&encoded_gas_tiles).map_err(ChunkError::Io)?;
        let matter_tile_count: u32 = self.matter_cellular_tiles.len().try_into()
            .map_err(|_| ChunkError::MatterTileCapacityExceeded)?;
        if self.matter_cellular_tiles.len() > 4096 {
            return Err(ChunkError::MatterTileCapacityExceeded);
        }
        w.write_all(&matter_tile_count.to_le_bytes()).map_err(ChunkError::Io)?;
        let mut encoded_matter_tiles: Vec<u8> = Vec::with_capacity(
            self.matter_cellular_tiles.len() * CHUNK_FILE_MATTER_TILE_BYTE_COUNT,
        );
        for (tile_index, matter_cells) in &self.matter_cellular_tiles {
            encoded_matter_tiles.extend_from_slice(&tile_index.to_le_bytes());
            for matter_cell in matter_cells.iter() {
                encoded_matter_tiles.extend_from_slice(&matter_cell.to_le_bytes());
            }
        }
        w.write_all(&encoded_matter_tiles).map_err(ChunkError::Io)
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

    /// Returns the packed radiances of one cellular tile within this chunk
    pub fn cellular_tile_radiances(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<[u32; 64], ChunkError> {
        let mut radiances: [u32; 64] = [0; 64];
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            radiances[tile_row * 8..tile_row * 8 + 8]
                .copy_from_slice(&self.radiances[chunk_row_start..chunk_row_start + 8]);
        }
        Ok(radiances)
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

    /// Returns one cellular tile's gas cells, or `None` when its gas is the default
    pub fn cellular_tile_gas(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<Option<[u32; 512]>, ChunkError> {
        self.cellular_tile_row_starts(coordinates)?;
        let tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let tile_x: usize = coordinates.x.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_y: usize = coordinates.y.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_index: u32 = (tile_y * tiles_per_chunk_side as usize + tile_x) as u32;
        Ok(self.gas_cellular_tiles.get(&tile_index).map(|gas_cells| **gas_cells))
    }

    /// Returns one cellular tile's temperature bits and amount of each cell, or `None` when its
    /// matter is the default
    pub fn cellular_tile_matter(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Result<Option<[u32; 128]>, ChunkError> {
        self.cellular_tile_row_starts(coordinates)?;
        let tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let tile_x: usize = coordinates.x.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_y: usize = coordinates.y.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_index: u32 = (tile_y * tiles_per_chunk_side as usize + tile_x) as u32;
        Ok(self.matter_cellular_tiles.get(&tile_index).map(|matter_cells| **matter_cells))
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

    /// Replaces the packed radiances of one cellular tile within this chunk
    pub fn set_cellular_tile_radiances(
        &mut self,
        coordinates: CellularTileCoordinates,
        radiances: &[u32; 64],
    ) -> Result<(), ChunkError> {
        for (tile_row, chunk_row_start) in self.cellular_tile_row_starts(coordinates)?
            .into_iter()
            .enumerate()
        {
            self.radiances[chunk_row_start..chunk_row_start + 8]
                .copy_from_slice(&radiances[tile_row * 8..tile_row * 8 + 8]);
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

    /// Replaces one cellular tile's gas cells, or removes its stored gas when `None`
    pub fn set_cellular_tile_gas(
        &mut self,
        coordinates: CellularTileCoordinates,
        gas: Option<&[u32; 512]>,
    ) -> Result<(), ChunkError> {
        self.cellular_tile_row_starts(coordinates)?;
        let tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let tile_x: usize = coordinates.x.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_y: usize = coordinates.y.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_index: u32 = (tile_y * tiles_per_chunk_side as usize + tile_x) as u32;
        if let Some(gas_cells) = gas {
            self.gas_cellular_tiles.insert(tile_index, Box::new(*gas_cells));
        } else {
            self.gas_cellular_tiles.remove(&tile_index);
        }
        Ok(())
    }

    /// Replaces one cellular tile's temperature bits and amount of each cell, or removes its stored
    /// matter when `None`
    pub fn set_cellular_tile_matter(
        &mut self,
        coordinates: CellularTileCoordinates,
        matter: Option<&[u32; 128]>,
    ) -> Result<(), ChunkError> {
        self.cellular_tile_row_starts(coordinates)?;
        let tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let tile_x: usize = coordinates.x.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_y: usize = coordinates.y.rem_euclid(tiles_per_chunk_side) as usize;
        let tile_index: u32 = (tile_y * tiles_per_chunk_side as usize + tile_x) as u32;
        if let Some(matter_cells) = matter {
            self.matter_cellular_tiles.insert(tile_index, Box::new(*matter_cells));
        } else {
            self.matter_cellular_tiles.remove(&tile_index);
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
