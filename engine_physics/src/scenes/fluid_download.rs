// Copyright Rob Gage 2026

use crate::{chunks::ChunkFluidParticle, tiles::TileArea};
use engine_compute::Accelerator;
use std::io;

/// Tracks one nonblocking export of fluid leaving GPU residency
pub struct FluidDownload {
    /// The outgoing world-tile strip owned by this transfer
    pub area: TileArea,
    /// Staging storage containing an aligned count followed by GPU particle records
    pub buffer: wgpu::Buffer,
    /// Whether GPU compaction and readback have been submitted
    pub is_started: bool,
    /// Completed persistent records or a readback failure
    pub result: Option<Result<Vec<ChunkFluidParticle>, io::Error>>,
}

impl FluidDownload {
    /// Creates a pending fixed-capacity fluid export
    pub fn new(accelerator: &Accelerator, area: TileArea, particle_capacity: u32) -> Self {
        Self {
            area,
            buffer: accelerator
                .wgpu_device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Fluid download buffer"),
                    size: 16 + u64::from(particle_capacity) * ChunkFluidParticle::GPU_SIZE as u64,
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

    /// Decodes compacted GPU records from mapped staging storage
    pub fn deserialize(
        bytes: &[u8],
        particle_capacity: u32,
    ) -> Result<Vec<ChunkFluidParticle>, io::Error> {
        let count: usize = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        if count > particle_capacity as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Downloaded fluid particle count exceeds pool capacity",
            ));
        }
        (0..count)
            .map(|index| {
                let start: usize = 16 + index * ChunkFluidParticle::GPU_SIZE;
                ChunkFluidParticle::deserialize_gpu(
                    &bytes[start..start + ChunkFluidParticle::GPU_SIZE],
                )
            })
            .collect()
    }
}
