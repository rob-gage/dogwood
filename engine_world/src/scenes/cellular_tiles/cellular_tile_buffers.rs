// Copyright Rob Gage 2026

use super::CELLULAR_TILE_SIZE;
use engine_host::acceleration::Accelerator;

/// GPU storage for every cellular tile property, with tiles stored contiguously
pub struct CellularTileBuffers {
    /// Accelerator owning these buffers, kept so a dropped scene can return its tiles
    accelerator: Accelerator,
    /// Tile-major material identifiers with one `u32` per row-major cell
    material_identifier_buffer: wgpu::Buffer,
    /// Tile-major packed RGBA color and packed radiance with two `u32` words per row-major cell
    color_buffer: wgpu::Buffer,
    /// Tile-major packed velocities and timing of dynamic cells with two `u32` per row-major cell
    kinematics_buffer: wgpu::Buffer,
    /// Whether each tile's dynamic cells must be simulated, with one `u32` per tile
    awake_buffer: wgpu::Buffer,
    /// Tile-major rigid body cell rasterized into each row-major cell plus one, zero for none
    rigid_body_cell_buffer: wgpu::Buffer,
    /// Tile-major gas of each row-major cell as eight `u32`: four species slots with a gas index in
    /// the top eight bits and an amount below, the phantom air amount, the flows through the cell's
    /// right and top faces as `f32` bits, and the stored pressure of a static cell as `i32` bits
    gas_buffer: wgpu::Buffer,
    /// Whether each tile's pressure must be simulated, with one `u32` per tile
    gas_awake_buffer: wgpu::Buffer,
    /// Tile-major pressure of each row-major cell above the ambient pressure in atmospheres as one
    /// `f32`, published by the pressure simulation's last tick in the cell's tile
    pressure_buffer: wgpu::Buffer,
    /// Tile-major matter of each row-major cell as two `u32`: the temperature in kelvin of the
    /// cell's cellular matter and gas as `f32` bits, past which a transitioning material banks its
    /// latent heat, and the amount of its cellular matter in units of
    /// `CELLULAR_TILE_AMOUNT_PER_UNIT`
    matter_buffer: wgpu::Buffer,
    /// Tile-major change in kelvin and fixed-point heat added by conversions for each row-major
    /// cell, as two `u32` words
    temperature_change_buffer: wgpu::Buffer,
    /// Whether each tile's temperatures must be simulated, with one `u32` per tile
    thermal_awake_buffer: wgpu::Buffer,
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
                size: size * 2,
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
        let gas: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.gas_buffer`"),
                size: size * 8,
                usage,
                mapped_at_creation: false,
            });
        let gas_awake: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.gas_awake_buffer`"),
                size: (u64::from(width) * u64::from(height)).max(1) * 4,
                usage,
                mapped_at_creation: false,
            });
        let pressure: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.pressure_buffer`"),
                size,
                usage,
                mapped_at_creation: false,
            });
        let matter: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.matter_buffer`"),
                size: size * 2,
                usage,
                mapped_at_creation: false,
            });
        let temperature_changes: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.temperature_change_buffer`"),
                size: size * 2,
                usage,
                mapped_at_creation: false,
            });
        let thermal_awake: wgpu::Buffer = accelerator.device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("`CellularTileBuffers.thermal_awake_buffer`"),
                size: (u64::from(width) * u64::from(height)).max(1) * 4,
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
            gas_buffer: gas,
            gas_awake_buffer: gas_awake,
            pressure_buffer: pressure,
            matter_buffer: matter,
            temperature_change_buffer: temperature_changes,
            thermal_awake_buffer: thermal_awake,
        }
    }

    /// Returns the accelerator owning these buffers
    pub fn accelerator(&self) -> &Accelerator { &self.accelerator }

    /// Returns the material identifier buffer
    pub fn material_identifier_buffer(&self) -> &wgpu::Buffer { &self.material_identifier_buffer }

    /// Returns the packed RGBA color and radiance buffer, two `u32` words per row-major cell
    pub fn color_buffer(&self) -> &wgpu::Buffer { &self.color_buffer }

    /// Returns the packed kinematics buffer
    pub fn kinematics_buffer(&self) -> &wgpu::Buffer { &self.kinematics_buffer }

    /// Returns the tile awake buffer
    pub fn awake_buffer(&self) -> &wgpu::Buffer { &self.awake_buffer }

    /// Returns the rigid body cell buffer
    pub fn rigid_body_cell_buffer(&self) -> &wgpu::Buffer { &self.rigid_body_cell_buffer }

    /// Returns the gas buffer
    pub fn gas_buffer(&self) -> &wgpu::Buffer { &self.gas_buffer }

    /// Returns the tile gas awake buffer
    pub fn gas_awake_buffer(&self) -> &wgpu::Buffer { &self.gas_awake_buffer }

    /// Returns the published pressure buffer
    pub fn pressure_buffer(&self) -> &wgpu::Buffer { &self.pressure_buffer }

    /// Returns the matter buffer of temperatures and amounts
    pub fn matter_buffer(&self) -> &wgpu::Buffer { &self.matter_buffer }

    /// Returns per-cell temperature changes and fixed-point heat added by matter changes
    pub fn temperature_change_buffer(&self) -> &wgpu::Buffer { &self.temperature_change_buffer }

    /// Returns the tile thermal awake buffer
    pub fn thermal_awake_buffer(&self) -> &wgpu::Buffer { &self.thermal_awake_buffer }

}
