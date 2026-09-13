// Copyright Rob Gage 2026

use crate::tiles::{
    TileCoordinates,
    TileData,
};
use std::{
    io,
    task::Waker,
};

/// Tracks a nonblocking native-to-`Accelerator` tile upload
pub struct TileUpload {
    /// The world tile being uploaded
    pub coordinates: TileCoordinates,
    /// Material identifiers in GPU cell order
    pub material_identifiers: Vec<u8>,
    /// Persistent appearances in matching GPU cell order
    pub appearances: Vec<u8>,
    /// Persistent integrities in matching GPU cell order
    pub integrities: Vec<u8>,
    /// Whether the upload has completed
    pub is_complete: bool,
    /// The completed upload result
    pub result: Option<Result<(), io::Error>>,
    /// The task woken when the upload completes
    pub waker: Option<Waker>,
}

impl TileUpload {

    /// Creates a pending tile upload with serialized `TileData`
    pub fn new(coordinates: TileCoordinates, tile_data: &TileData) -> Self {
        let mut material_identifiers: Vec<u8> =
            Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data.serialize_material_identifiers(&mut material_identifiers)
            .expect("Writing to a Vec cannot fail");
        let mut appearances: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data.serialize_appearances(&mut appearances)
            .expect("Writing to a Vec cannot fail");
        let mut integrities: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data.serialize_integrities(&mut integrities)
            .expect("Writing to a Vec cannot fail");
        Self {
            coordinates,
            material_identifiers,
            appearances,
            integrities,
            result: None,
            is_complete: false,
            waker: None,
        }
    }

}
