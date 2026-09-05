// Copyright Rob Gage 2026

use engine_graphics::MaterialGraphics;
use std::{
    io,
    path::PathBuf,
};

/// Configuration used by a `Scene`
pub struct SceneConfiguration {
    /// The graphics properties of this scene's materials
    pub material_graphics: MaterialGraphics,
    /// The path used to store this `Scene`'s data
    pub data_path: PathBuf,
    /// The simulation width of the `Scene`
    pub simulation_width: u16,
    /// The simulation height of the `Scene`
    pub simulation_height: u16,
    /// The size of the buffer around the simulation area that is loaded on the GPU but not
    /// simulated
    pub simulation_buffer_size: u8,
    /// The required batch size when loading and unloading tiles from the `Scene`
    pub tile_streaming_batch_size: u8,
}

impl SceneConfiguration {

    /// Validates that this configuration can create a streaming tile buffer
    pub fn validate(&self) -> Result<(), io::Error> {
        if self.simulation_width == 0 || self.simulation_height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation dimensions must not be zero",
            ));
        }
        if self.tile_streaming_batch_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Tile streaming batch size must not be zero",
            ));
        }
        if u16::from(self.tile_streaming_batch_size) > self.simulation_width ||
                u16::from(self.tile_streaming_batch_size) > self.simulation_height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Tile streaming batch size exceeds the simulation dimensions",
            ));
        }
        if self.simulation_buffer_size % self.tile_streaming_batch_size != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation buffer size must be a multiple of tile streaming batch size",
            ));
        }
        let buffer_size: u16 = u16::from(self.simulation_buffer_size) * 2;
        if self.simulation_width.checked_add(buffer_size).is_none() ||
                self.simulation_height.checked_add(buffer_size).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Simulation buffer exceeds the maximum scene dimensions",
            ));
        }
        Ok(())
    }

}
