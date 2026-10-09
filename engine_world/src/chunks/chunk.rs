// Copyright Rob Gage 2026

use super::{
    ChunkError,
    ChunkCoordinates,
    CHUNK_SIZE,
};
use crate::materials::MaterialIdentifier;
use std::io;

/// Magic bytes identifying persisted world chunks
const CHUNK_FILE_HEADER: [u8; 8] = *b"DOGWOOD\0";

/// Current sparse chunk file format version
const CHUNK_FILE_VERSION: u16 = 3;

/// Number of cells addressable by one chunk
const CHUNK_CELL_COUNT: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Sparse CPU material storage for one chunk
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    /// World coordinates identifying this chunk
    pub coordinates: ChunkCoordinates,
    /// Material identifiers in row-major cell order
    materials: Vec<MaterialIdentifier>,
}

impl Chunk {

    /// Creates an empty chunk at the given coordinates
    pub fn new(coordinates: ChunkCoordinates) -> Self {
        Self {
            coordinates,
            materials: vec![MaterialIdentifier::EMPTY; CHUNK_CELL_COUNT],
        }
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
        let mut encoded_materials: Vec<u8> = vec![0; stored_cell_count * 8];
        r.read_exact(&mut encoded_materials).map_err(|error| {
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
        // expand sparse entries into the chunk's row-major material array
        for encoded_material in encoded_materials.chunks_exact(8) {
            let cell_index: u32 = u32::from_le_bytes([
                encoded_material[0],
                encoded_material[1],
                encoded_material[2],
                encoded_material[3],
            ]);
            let identifier_value: u32 = u32::from_le_bytes([
                encoded_material[4],
                encoded_material[5],
                encoded_material[6],
                encoded_material[7],
            ]);
            if cell_index as usize >= CHUNK_CELL_COUNT {
                return Err(ChunkError::CellIndexOutOfRange);
            }
            let identifier: MaterialIdentifier = MaterialIdentifier::from_u32(identifier_value);
            if identifier == MaterialIdentifier::EMPTY {
                return Err(ChunkError::CellIsEmpty);
            }
            let material: &mut MaterialIdentifier = &mut chunk.materials[cell_index as usize];
            if *material != MaterialIdentifier::EMPTY {
                return Err(ChunkError::CellIndexIsDuplicate);
            }
            *material = identifier;
        }
        Ok(chunk)
    }

    /// Writes this chunk in the current versioned sparse format
    pub fn write<W: io::Write>(&self, w: &mut W) -> Result<(), ChunkError> {
        let stored_cell_count: u32 = self.materials.iter()
            .filter(|material| **material != MaterialIdentifier::EMPTY)
            .count() as u32;
        // write the format header before the sparse cell entries
        let mut header: Vec<u8> = Vec::with_capacity(22);
        header.extend_from_slice(&CHUNK_FILE_HEADER);
        header.extend_from_slice(&CHUNK_FILE_VERSION.to_le_bytes());
        header.extend_from_slice(&self.coordinates.x.to_le_bytes());
        header.extend_from_slice(&self.coordinates.y.to_le_bytes());
        header.extend_from_slice(&stored_cell_count.to_le_bytes());
        w.write_all(&header).map_err(ChunkError::Io)?;
        let mut encoded_materials: Vec<u8> = Vec::with_capacity(stored_cell_count as usize * 8);
        for (cell_index, material) in self.materials.iter().enumerate() {
            if *material == MaterialIdentifier::EMPTY { continue; }
            let cell_index: u32 = cell_index as u32;
            encoded_materials.extend_from_slice(&cell_index.to_le_bytes());
            encoded_materials.extend_from_slice(&material.as_u32().to_le_bytes());
        }
        w.write_all(&encoded_materials).map_err(ChunkError::Io)
    }

}
