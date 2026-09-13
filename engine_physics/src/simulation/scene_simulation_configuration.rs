// Copyright Rob Gage 2026

use super::Fluids;
use std::io;

/// Configures the active and buffered simulation area of a `Scene`
pub struct SceneSimulationConfiguration {
    /// Scene gravity acceleration in tiles per second squared
    pub gravity: [f32; 2],
    /// The width of the active simulation area in tiles
    pub width: u16,
    /// The height of the active simulation area in tiles
    pub height: u16,
    /// The size of the GPU-resident buffer around the active area
    pub buffer_size: u8,
    /// The required batch size when streaming tiles into and out of the scene
    pub streaming_batch_size: u8,
}

impl SceneSimulationConfiguration {

    /// Validates that this configuration can create a streaming tile buffer
    pub fn validate(&self) -> Result<(), io::Error> {
        if !self.gravity.into_iter().all(f32::is_finite) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation gravity must be finite",
            ));
        }
        if self.width == 0 || self.height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation dimensions must not be zero",
            ));
        }
        if self.streaming_batch_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation streaming batch size must not be zero",
            ));
        }
        if u16::from(self.streaming_batch_size) > self.width ||
                u16::from(self.streaming_batch_size) > self.height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation streaming batch size exceeds the simulation dimensions",
            ));
        }
        if self.buffer_size % self.streaming_batch_size != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation buffer size must be a multiple of streaming batch size",
            ));
        }
        if u16::from(self.buffer_size) < u16::from(self.streaming_batch_size) +
                u16::from(Fluids::minimum_buffer_tiles()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation buffer cannot contain fluid motion and boundary support",
            ));
        }
        let buffer_size: u16 = u16::from(self.buffer_size) * 2;
        if self.width.checked_add(buffer_size).is_none() ||
                self.height.checked_add(buffer_size).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation buffer exceeds the maximum scene dimensions",
            ));
        }
        Ok(())
    }

}
