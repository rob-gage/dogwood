// Copyright Rob Gage 2026

/// One moving thing that pushes the motion field, in tiles and tiles per second relative to the
/// camera
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneGraphicsDecorationMover {
    /// Center of its bounds
    pub position: [f32; 2],
    /// Half the size of its bounds
    pub half_size: [f32; 2],
    /// Velocity
    pub velocity: [f32; 2],
}

/// Plants to draw in one scene frame
#[derive(Clone, Debug)]
pub struct SceneGraphicsDecorations<'a> {
    /// Atlas of grayscale art and color ramps, whose pixels with alpha below one glow
    pub atlas: &'a wgpu::TextureView,
    /// Static batches of decorations, one per chunk
    pub batches: Vec<SceneGraphicsDecorationBatch<'a>>,
    /// Nearby moving things that push the motion field
    pub movers: [SceneGraphicsDecorationMover; 8],
    /// Seconds of simulated time, which moves the air
    pub time: f32,
}

/// One chunk's static decorations
#[derive(Clone, Debug)]
pub struct SceneGraphicsDecorationBatch<'a> {
    /// Instances, the back layer first
    pub buffer: &'a wgpu::Buffer,
    /// Number of instances in the back layer
    pub back_count: u32,
    /// Number of instances in the front layer
    pub front_count: u32,
    /// Chunk corner relative to the camera in tiles
    pub origin: [f32; 2],
}
