// Copyright Rob Gage 2026

use std::path::PathBuf;

/// Configuration used by a `Scene`
pub struct SceneConfiguration {
    /// The path used to store this `Scene`'s data
    pub data_path: PathBuf,
    /// The simulation width of the `Scene`
    pub simulation_width: u16,
    /// The simulation height of the `Scene`
    pub simulation_height: u16,
    /// The required batch size when loading and unloading tiles from the `Scene`
    pub tile_streaming_batch_size: u8,
}