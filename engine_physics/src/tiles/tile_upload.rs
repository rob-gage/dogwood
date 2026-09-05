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
    /// The serialized tile data to upload
    pub data: Vec<u8>,
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
        let mut data: Vec<u8> = Vec::with_capacity(TileData::SERIALIZED_SIZE);
        tile_data.serialize(&mut data).expect("Writing to a Vec cannot fail");
        Self {
            coordinates,
            data,
            result: None,
            is_complete: false,
            waker: None,
        }
    }

}
