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
    materials::MaterialIdentifier,
};
use std::io;

/// Magic bytes identifying persisted world chunks
const CHUNK_FILE_HEADER: [u8; 8] = *b"DOGWOOD\0";

/// Current sparse chunk file format version
const CHUNK_FILE_VERSION: u16 = 4;

/// Bytes in one sparse cell entry holding its index, material identifier, and color
const CHUNK_FILE_CELL_BYTE_COUNT: usize = 12;

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
}

impl Chunk {

    /// Creates an empty chunk at the given coordinates
    pub fn new(coordinates: ChunkCoordinates) -> Self {
        Self {
            coordinates,
            material_identifiers: vec![MaterialIdentifier::EMPTY; CHUNK_CELL_COUNT],
            colors: vec![0; CHUNK_CELL_COUNT],
        }
    }

    /// Fills every cell in this chunk with one material and packed color
    pub fn fill(&mut self, material_identifier: MaterialIdentifier, color: u32) {
        self.material_identifiers.fill(material_identifier);
        self.colors.fill(color);
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
        for (cell_index, (material, color)) in
            self.material_identifiers.iter().zip(self.colors.iter()).enumerate()
        {
            if *material == MaterialIdentifier::EMPTY && *color == 0 { continue; }
            let cell_index: u32 = cell_index as u32;
            encoded_cells.extend_from_slice(&cell_index.to_le_bytes());
            encoded_cells.extend_from_slice(&material.as_u32().to_le_bytes());
            encoded_cells.extend_from_slice(&color.to_le_bytes());
        }
        w.write_all(&encoded_cells).map_err(ChunkError::Io)
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
