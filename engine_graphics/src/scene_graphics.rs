// Copyright Rob Gage 2026

use super::MaterialGraphics;
use crate::Color;
use crate::engine_compute::AcceleratorBuffer;
use std::sync::Arc;

/// A small world-space overlay drawn above the simulated scene.
#[derive(Clone, Copy, Debug)]
pub enum SceneOverlay {
    /// A thin circle outline in world/tile units.
    CircleOutline {
        /// Center in world/tile coordinates.
        center: [f32; 2],
        /// Radius in world/tile units.
        radius: f32,
        /// Outline color.
        color: Color,
    },
}

/// Minimal visible representation of a generic scene actor.
#[derive(Copy, Clone, Debug)]
pub struct SceneActorGraphics {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct SceneSpriteSheetGraphics {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct SceneSpriteGraphics {
    pub position: [f32; 2],
    pub world_size: [f32; 2],
    pub world_offset: [f32; 2],
    pub texture_coordinates: [f32; 4],
    pub sprite_sheet: SceneSpriteSheetGraphics,
    pub radiance_sprite_sheet: Option<SceneSpriteSheetGraphics>,
}

/// Borrowed scene resources exposed to the renderer for one frame.
///
/// The scene retains ownership of all buffers. The renderer may bind these
/// references while encoding a frame but must not retain them after that frame.
pub struct SceneGraphics<'a> {
    /// The graphics properties for every material in the scene
    pub material_graphics: &'a MaterialGraphics,
    /// The `Accelerator` buffer containing material identifiers
    pub cellular_material_identifiers: &'a AcceleratorBuffer,
    /// The parallel `Accelerator` buffer containing persistent cell appearances
    pub cellular_appearances: &'a AcceleratorBuffer,
    /// The transient static material identifier rasterized from rigid bodies
    pub rigid_material_identifiers: &'a AcceleratorBuffer,
    /// The transient appearance rasterized from rigid bodies
    pub rigid_appearances: &'a AcceleratorBuffer,
    /// The transient fluid material identifier derived for each physical cell
    pub fluid_material_identifiers: &'a AcceleratorBuffer,
    /// The transient fluid coverage derived for each physical cell
    pub fluid_coverage: &'a AcceleratorBuffer,
    /// Authoritative species-major gas concentrations
    pub gas_concentrations: &'a AcceleratorBuffer,
    /// Number of registered gas species in the concentration allocation
    pub gas_count: u32,
    /// The transient directional pressure retained by each physical cell
    pub cellular_pressure: &'a AcceleratorBuffer,
    /// The tile coordinates of the buffered area's bottom-left corner
    pub buffered_origin: [i32; 2],
    /// The dimensions of the buffered tile area
    pub buffered_tile_size: [u32; 2],
    /// The ring-buffer offset of the buffered area's bottom-left tile
    pub ring_offset: [u32; 2],
    /// The center and size of the possessed walking pawn, if one is active
    pub walking_pawn: Option<([f32; 2], [f32; 2])>,
    pub actors: Vec<SceneActorGraphics>,
    pub sprites: Vec<SceneSpriteGraphics>,
}
