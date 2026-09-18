// Copyright Rob Gage 2026

use crate::{chunks::ChunkFluidParticle, tiles::TileArea};
use engine_compute::Accelerator;
use std::io;

/// Tracks one nonblocking transfer of dormant fluid into Accelerator residency
pub struct FluidUpload {
    /// The incoming world-tile strip owned by this transfer
    pub area: TileArea,
    /// Persistent records held outside their chunks until transfer completion
    pub particles: Vec<ChunkFluidParticle>,
    /// Staging storage receiving one success flag per record
    pub buffer: wgpu::Buffer,
    /// Whether Accelerator reconstruction and readback have been submitted
    pub is_started: bool,
    /// Records the Accelerator pool could not accept, or a readback failure
    pub result: Option<Result<Vec<ChunkFluidParticle>, io::Error>>,
}

impl FluidUpload {
    /// Creates a pending fluid import
    pub fn new(
        accelerator: &Accelerator,
        area: TileArea,
        particles: Vec<ChunkFluidParticle>,
    ) -> Self {
        Self {
            area,
            buffer: accelerator
                .wgpu_device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Fluid upload result buffer"),
                    size: particles.len() as u64 * 4,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            particles,
            is_started: false,
            result: None,
        }
    }

    /// Returns records whose Accelerator free-slot claim failed
    pub fn failed_particles(&self, bytes: &[u8]) -> Result<Vec<ChunkFluidParticle>, io::Error> {
        if bytes.len() != self.particles.len() * 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid fluid upload result size",
            ));
        }
        let mut failed: Vec<ChunkFluidParticle> = Vec::new();
        for (index, particle) in self.particles.iter().enumerate() {
            match u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap()) {
                0 => failed.push(*particle),
                1 => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid fluid upload result",
                    ));
                }
            }
        }
        Ok(failed)
    }
}
