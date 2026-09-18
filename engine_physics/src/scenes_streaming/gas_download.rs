// Copyright Rob Gage 2026

use std::io;

use engine_compute::Accelerator;

use crate::chunks::ChunkGasCell;
use crate::materials::MaterialIdentifier;
use crate::tiles::CellCoordinates;
use crate::tiles::TileArea;

const STREAMING_CONCENTRATION_EPSILON: f32 = 0.0001;

/// Tracks one nonblocking export of gas leaving Accelerator residency
pub struct GasDownload {
    /// The outgoing world-tile strip owned by this transfer
    pub area: TileArea,
    /// Dense staging records for every outgoing world cell
    pub buffer: wgpu::Buffer,
    /// Whether Accelerator extraction and readback have been submitted
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
            buffer: accelerator
                .wgpu_device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Gas download buffer"),
                    size: u64::from(maximum_cell_count) * u64::from(5 + gas_count) * 4,
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

    /// Decodes fixed Accelerator records and drops negligible concentration tails
    pub fn deserialize(
        bytes: &[u8],
        area: TileArea,
        gas_identifiers: &[MaterialIdentifier],
    ) -> Result<Vec<ChunkGasCell>, io::Error> {
        let dimensions: [u16; 2] = area.dimensions();
        let gas_cell_count: usize = usize::from(dimensions[0]) * usize::from(dimensions[1]) * 64;
        let gas_record_stride: usize = 5 + gas_identifiers.len();
        if bytes.len() != gas_cell_count * gas_record_stride * 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid gas download size",
            ));
        }
        let mut gas_cells: Vec<ChunkGasCell> = Vec::new();
        for gas_cell_index in 0..gas_cell_count {
            let gas_record_offset: usize = gas_cell_index * gas_record_stride * 4;
            let gas_cell_coordinates: CellCoordinates = CellCoordinates {
                x: crate::binary_reader::read_u32_at(bytes, gas_record_offset) as i32,
                y: crate::binary_reader::read_u32_at(bytes, gas_record_offset + 4) as i32,
            };
            if !area.contains(gas_cell_coordinates.tile_coordinates()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Downloaded gas cell is outside its transfer area",
                ));
            }
            let gas_cell_velocity: [f32; 2] = [
                f32::from_bits(crate::binary_reader::read_u32_at(
                    bytes,
                    gas_record_offset + 8,
                )),
                f32::from_bits(crate::binary_reader::read_u32_at(
                    bytes,
                    gas_record_offset + 12,
                )),
            ];
            let gas_cell_temperature: f32 = f32::from_bits(crate::binary_reader::read_u32_at(
                bytes,
                gas_record_offset + 16,
            ));
            if !gas_cell_velocity.into_iter().all(f32::is_finite) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Downloaded gas velocity is not finite",
                ));
            }
            if !gas_cell_temperature.is_finite() || gas_cell_temperature < 0.0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Downloaded gas temperature is invalid",
                ));
            }
            let mut gas_species: Vec<(MaterialIdentifier, f32)> = Vec::new();
            for (gas_species_index, gas_identifier) in gas_identifiers.iter().enumerate() {
                let concentration: f32 = f32::from_bits(crate::binary_reader::read_u32_at(
                    bytes,
                    gas_record_offset + (5 + gas_species_index) * 4,
                ));
                if !concentration.is_finite() || concentration < 0.0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Downloaded gas concentration is invalid",
                    ));
                }
                if concentration > STREAMING_CONCENTRATION_EPSILON {
                    gas_species.push((*gas_identifier, concentration));
                }
            }
            if !gas_species.is_empty() {
                gas_cells.push(ChunkGasCell {
                    coordinates: gas_cell_coordinates,
                    velocity: gas_cell_velocity,
                    temperature: gas_cell_temperature,
                    species: gas_species,
                });
            }
        }
        Ok(gas_cells)
    }
}
