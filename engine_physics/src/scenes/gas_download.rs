// Copyright Rob Gage 2026

use crate::{
    chunks::ChunkGasCell,
    materials::MaterialIdentifier,
    tiles::{
        CellCoordinates,
        TileArea,
    },
};
use engine_compute::Accelerator;
use std::io;

const STREAMING_CONCENTRATION_EPSILON: f32 = 0.0001;

/// Tracks one nonblocking export of gas leaving GPU residency
pub struct GasDownload {
    /// The outgoing world-tile strip owned by this transfer
    pub area: TileArea,
    /// Dense staging records for every outgoing world cell
    pub buffer: wgpu::Buffer,
    /// Whether GPU extraction and readback have been submitted
    pub is_started: bool,
    /// Completed sparse persistent cells or a readback failure
    pub result: Option<Result<Vec<ChunkGasCell>, io::Error>>,
}

impl GasDownload {

    /// Creates reusable staging storage for the largest streamed strip
    pub fn new(
        accelerator: &Accelerator,
        area: TileArea,
        maximum_cell_count: u32,
        gas_count: u32,
    ) -> Self {
        Self {
            area,
            buffer: accelerator.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("Gas download buffer"),
                size: u64::from(maximum_cell_count) * u64::from(4 + gas_count) * 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            is_started: false,
            result: None,
        }
    }

    /// Reuses completed staging storage for another outgoing area
    pub fn reset(&mut self, area: TileArea) {
        self.area = area;
        self.is_started = false;
        self.result = None;
    }

    /// Decodes fixed GPU records and drops negligible concentration tails
    pub fn deserialize(
        bytes: &[u8],
        area: TileArea,
        gas_identifiers: &[MaterialIdentifier],
    ) -> Result<Vec<ChunkGasCell>, io::Error> {
        let dimensions: [u16; 2] = area.dimensions();
        let cell_count: usize = usize::from(dimensions[0]) * usize::from(dimensions[1]) * 64;
        let stride: usize = 4 + gas_identifiers.len();
        if bytes.len() != cell_count * stride * 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid gas download size"));
        }
        let mut cells: Vec<ChunkGasCell> = Vec::new();
        for cell_index in 0..cell_count {
            let offset: usize = cell_index * stride * 4;
            let coordinates: CellCoordinates = CellCoordinates {
                x: Self::u32_at(bytes, offset) as i32,
                y: Self::u32_at(bytes, offset + 4) as i32,
            };
            if !area.contains(coordinates.tile_coordinates()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Downloaded gas cell is outside its transfer area",
                ));
            }
            let velocity: [f32; 2] = [
                f32::from_bits(Self::u32_at(bytes, offset + 8)),
                f32::from_bits(Self::u32_at(bytes, offset + 12)),
            ];
            if !velocity.into_iter().all(f32::is_finite) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Downloaded gas velocity is not finite",
                ));
            }
            let mut species: Vec<(MaterialIdentifier, f32)> = Vec::new();
            for (species_index, identifier) in gas_identifiers.iter().enumerate() {
                let concentration: f32 = f32::from_bits(Self::u32_at(
                    bytes,
                    offset + (4 + species_index) * 4,
                ));
                if !concentration.is_finite() || concentration < 0.0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Downloaded gas concentration is invalid",
                    ));
                }
                if concentration > STREAMING_CONCENTRATION_EPSILON {
                    species.push((*identifier, concentration));
                }
            }
            if !species.is_empty() { cells.push(ChunkGasCell { coordinates, velocity, species }); }
        }
        Ok(cells)
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

}
