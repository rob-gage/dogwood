// Copyright Rob Gage 2026

use crate::tiles::{
    TileCoordinates,
    TileData,
};
use engine_compute::Accelerator;
use std::{
    io,
    task::Waker,
};

/// Tracks a nonblocking `Accelerator`-to-native tile download
pub struct TileDownload {
    /// The world tile being downloaded
    pub coordinates: TileCoordinates,
    /// The staging buffer receiving the tile data
    pub buffer: wgpu::Buffer,
    /// Whether the GPU copy has been submitted
    pub is_started: bool,
    /// Whether the download callback has completed
    pub is_complete: bool,
    /// The completed download result
    pub result: Option<Result<TileData, io::Error>>,
    /// The task woken when the download completes
    pub waker: Option<Waker>,
}

impl TileDownload {

    /// Creates a pending tile download with a staging buffer sized for one `TileData`
    pub fn new(accelerator: &Accelerator, coordinates: TileCoordinates) -> Self {
        Self {
            coordinates,
            buffer: accelerator.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("Tile download buffer"),
                size: TileData::SERIALIZED_SIZE as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            is_started: false,
            is_complete: false,
            result: None,
            waker: None,
        }
    }

}
