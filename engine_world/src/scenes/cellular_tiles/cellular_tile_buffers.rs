// Copyright Rob Gage 2026

use super::CELLULAR_TILE_SIZE;
use engine_host::acceleration::Accelerator;

/// GPU storage for every cellular tile property, with tiles stored contiguously
pub struct CellularTileBuffers {
    /// Accelerator owning these buffers, kept so a dropped scene can return its tiles
    accelerator: Accelerator,
    /// Tile-major material identifiers with one `u32` per row-major cell
    material_identifier_buffer: wgpu::Buffer,
    /// Tile-major packed RGBA colors with one `u32` per row-major cell
    color_buffer: wgpu::Buffer,
    /// Tile-major packed velocities and timing of dynamic cells with two `u32` per row-major cell
    kinematics_buffer: wgpu::Buffer,
    /// Whether each tile's dynamic cells must be simulated, with one `u32` per tile
    awake_buffer: wgpu::Buffer,
    /// Tile-major rigid body cell rasterized into each row-major cell plus one, zero for none
    rigid_body_cell_buffer: wgpu::Buffer,
}

impl CellularTileBuffers {

    /// Allocates buffers for a maximum number of cellular tiles along each axis
    pub fn new(accelerator: &Accelerator, width: u32, height: u32) -> Self {
        let cellular_tile_byte_count: u64 = (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u64 * 4;
        let size: u64 = u64::from(width) * u64::from(height) * cellular_tile_byte_count;
        let usage: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;
        let material_identifiers: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.material_identifier_buffer`"),
                size,
                usage,
                mapped_at_creation: false,
            });
        let colors: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.color_buffer`"),
                size,
                usage,
                mapped_at_creation: false,
            });
        let kinematics: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.kinematics_buffer`"),
                size: size * 2,
                usage,
                mapped_at_creation: false,
            });
        let awake: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.awake_buffer`"),
                size: (u64::from(width) * u64::from(height)).max(1) * 4,
                usage,
                mapped_at_creation: false,
            });
        let rigid_body_cells: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.rigid_body_cell_buffer`"),
                size,
                usage,
                mapped_at_creation: false,
            });
        Self {
            accelerator: accelerator.clone(),
            material_identifier_buffer: material_identifiers,
            color_buffer: colors,
            kinematics_buffer: kinematics,
            awake_buffer: awake,
            rigid_body_cell_buffer: rigid_body_cells,
        }
    }

    /// Returns the accelerator owning these buffers
    pub fn accelerator(&self) -> &Accelerator { &self.accelerator }

    /// Returns the material identifier buffer
    pub fn material_identifier_buffer(&self) -> &wgpu::Buffer { &self.material_identifier_buffer }

    /// Returns the packed RGBA color buffer
    pub fn color_buffer(&self) -> &wgpu::Buffer { &self.color_buffer }

    /// Returns the packed kinematics buffer
    pub fn kinematics_buffer(&self) -> &wgpu::Buffer { &self.kinematics_buffer }

    /// Returns the tile awake buffer
    pub fn awake_buffer(&self) -> &wgpu::Buffer { &self.awake_buffer }

    /// Returns the rigid body cell buffer
    pub fn rigid_body_cell_buffer(&self) -> &wgpu::Buffer { &self.rigid_body_cell_buffer }

}
