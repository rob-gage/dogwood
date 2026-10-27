// Copyright Rob Gage 2026

use super::{
    CellularTileBuffers,
    CellularTileRegion,
    CELLULAR_TILE_ACTOR_COUNT_MAXIMUM,
    CELLULAR_TILE_RESOLVE_COUNT,
    CELLULAR_TILE_SIZE,
    CELLULAR_TILE_TIME_STEP,
};
use crate::{
    actors::Actor,
    materials::MaterialTable,
    scenes::{
        collisions::{
            ColliderShape,
            CollisionRigidBodyPass,
            COLLISION_RIGID_BODY_COUNT_MAXIMUM,
        },
        SceneError,
    },
};
use engine_host::acceleration::{Accelerator, AcceleratorReadback, AcceleratorReadbackError};
use std::collections::HashMap;

/// Number of invocations in one compute workgroup, one per cell of a tile
const WORKGROUP_SIZE: u32 = 64;

/// Largest number of substeps in one tick, one per cell of the largest speed
const SUBSTEP_COUNT: usize = 8;

/// Bytes between the uniforms of consecutive substeps, the minimum uniform offset alignment
const UNIFORM_STRIDE: u64 = 256;

/// Bytes of one actor record
const ACTOR_BYTE_COUNT: u64 = 64;

/// Bytes of the contacts accumulated for one actor
const CONTACT_BYTE_COUNT: u64 = 32;

/// Fixed-point units per cell of accumulated contacts
const CONTACT_SCALE: f32 = 1024.0;

/// Bytes of the contacts accumulated for one rigid body
const RIGID_BODY_CONTACT_BYTE_COUNT: u64 = 44;

/// Number of contact and control readbacks that can each be in flight at once
const READBACK_COUNT: usize = 3;

/// GPU simulation of active dynamic cells and their interactions with actors, rigid bodies, and
/// fluid
pub struct CellularTilePass {
    /// Pipelines of every simulation step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind group of the uniforms, cells, tiles, scratch buffers, actors, and contacts
    bind_group: wgpu::BindGroup,
    /// Bind group of the indirect dispatches written by the prepare step
    dispatch_bind_group: wgpu::BindGroup,
    /// Uniforms of every substep, each at its own dynamic offset
    uniform_buffer: wgpu::Buffer,
    /// Active tile count, substep count, resolve passes still pending, and the active tiles
    control_buffer: wgpu::Buffer,
    /// Indirect dispatches of whole ticks followed by those of every substep
    dispatch_buffer: wgpu::Buffer,
    /// Records of the actors pushing and resting on dynamic cells
    actor_buffer: wgpu::Buffer,
    /// Impulses and contacts accumulated for every actor
    contact_buffer: wgpu::Buffer,
    /// Mappable copies of the contacts, each with the actors and masses it was simulated with
    contact_readbacks: AcceleratorReadback<Vec<(Actor, f32)>>,
    /// Number of simulated ticks, varying the order of tied moves
    tick: u32,
    /// Mappable copies of the control counts, each with the resolve count it was simulated with
    control_readbacks: AcceleratorReadback<usize>,
    /// Impulses and fluid contacts accumulated for every rigid body until they are read
    rigid_body_contact_buffer: wgpu::Buffer,
    /// Physical tiles holding rigid body cells or their neighboring fluid cells
    rigid_fluid_tile_buffer: wgpu::Buffer,
    /// Mappable copies of the rigid body contacts, each with the rigid body of every index
    rigid_body_contact_readbacks: AcceleratorReadback<Vec<u64>>,
    /// Rigid body accumulating contacts at every index, zero for none
    rigid_bodies: Vec<u64>,
    /// Substeps encoded per tick, from the substeps recent ticks needed
    substep_count: usize,
    /// Resolve passes encoded per substep, from the chain lengths recent ticks resolved
    resolve_count: usize,
}

impl CellularTilePass {

    /// Returns the actor records uploaded for the current tick
    pub fn actor_buffer(&self) -> &wgpu::Buffer { &self.actor_buffer }

    /// Creates simulation resources for every tile the cellular tile buffers can hold
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        collision_rigid_body_pass: &CollisionRigidBodyPass,
        fluid_raster_buffer: &wgpu::Buffer,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let cell_count: u64 = cellular_tile_buffers.material_identifier_buffer().size() / 4;
        let tile_count: u64 = cell_count / (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u64;
        let storage_entry = |binding: u32, read_only: bool| -> wgpu::BindGroupLayoutEntry {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`CellularTilePass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(48),
                        },
                        count: None,
                    },
                    storage_entry(1, false),
                    storage_entry(2, false),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, true),
                    storage_entry(6, false),
                    storage_entry(7, false),
                    storage_entry(8, false),
                    storage_entry(9, true),
                    storage_entry(10, false),
                    storage_entry(11, true),
                    storage_entry(12, true),
                    storage_entry(13, true),
                    storage_entry(14, true),
                    storage_entry(15, false),
                    storage_entry(16, false),
                    storage_entry(17, true),
                    storage_entry(18, true),
                    storage_entry(19, true),
                    storage_entry(20, false),
                    storage_entry(21, true),
                ],
            },
        );
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`CellularTilePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        // only the prepare step writes the indirect dispatches the other steps read
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`CellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`CellularTilePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&dispatch_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`CellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    concat!(
                        include_str!("cellular_tile.wgsl"),
                        "\n",
                        include_str!("cellular_tile_fluids.wgsl"),
                    ).into(),
                ),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in [
            "collect",
            "clear",
            "prepare",
            "integrate",
            "request",
            "pick",
            "resolve_begin",
            "resolve_even",
            "resolve_odd",
            "stage",
            "commit",
            "contact",
            "rigid_fluid_faces",
            "rigid_fluid",
        ] {
            let layout: &wgpu::PipelineLayout = if entry_point == "prepare" {
                &prepare_pipeline_layout
            } else {
                &pipeline_layout
            };
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`CellularTilePass.pipelines`"),
                    layout: Some(layout),
                    module: &shader_module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                },
            ));
        }
        let create_buffer = |label: &'static str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16),
                usage,
                mapped_at_creation: false,
            })
        };
        let uniform_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.uniform_buffer`",
            UNIFORM_STRIDE * (SUBSTEP_COUNT * CELLULAR_TILE_RESOLVE_COUNT) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // each cell's requested move, pick, and two resolution statuses, the second reused for the
        // velocity its collisions leave it
        let motion_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.motion_buffer`",
            cell_count * 16,
            wgpu::BufferUsages::STORAGE,
        );
        // each cell's incoming material identifier, color, and kinematics, then its temperature and
        // amount
        let staged_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.staged_buffer`",
            cell_count * 32,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.control_buffer`",
            12 + tile_count * 4,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.dispatch_buffer`",
            (1 + SUBSTEP_COUNT as u64) * 12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let actor_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.actor_buffer`",
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM as u64 * ACTOR_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let contact_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.contact_buffer`",
            CELLULAR_TILE_ACTOR_COUNT_MAXIMUM as u64 * CONTACT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let rigid_body_contact_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.rigid_body_contact_buffer`",
            COLLISION_RIGID_BODY_COUNT_MAXIMUM as u64 * RIGID_BODY_CONTACT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let rigid_fluid_tile_buffer: wgpu::Buffer = create_buffer(
            "`CellularTilePass.rigid_fluid_tile_buffer`",
            tile_count * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let rigid_body_contact_readbacks: AcceleratorReadback<Vec<u64>> = AcceleratorReadback::new(
            accelerator,
            "`CellularTilePass.rigid_body_contact_readbacks`",
            rigid_body_contact_buffer.size(),
            READBACK_COUNT,
        );
        let contact_readbacks: AcceleratorReadback<Vec<(Actor, f32)>> = AcceleratorReadback::new(
            accelerator,
            "`CellularTilePass.contact_readbacks`",
            contact_buffer.size(),
            READBACK_COUNT,
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`CellularTilePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(48),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_tile_buffers.color_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cellular_tile_buffers.kinematics_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: cellular_tile_buffers.awake_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: material_table.cellular_dynamic_properties_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 6, resource: motion_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: staged_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: control_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 9, resource: actor_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: contact_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: collision_rigid_body_pass.cellular_tile_cell_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: collision_rigid_body_pass.cellular_tile_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: collision_rigid_body_pass.rigid_body_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: rigid_body_contact_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: fluid_raster_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: material_table.fluid_properties_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: cellular_tile_buffers.pressure_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: rigid_fluid_tile_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: cellular_tile_buffers.matter_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: collision_rigid_body_pass.cellular_tile_state_buffer()
                        .as_entire_binding(),
                },
            ],
        });
        let dispatch_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`CellularTilePass.dispatch_bind_group`"),
                layout: &dispatch_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            },
        );
        let control_readbacks: AcceleratorReadback<usize> = AcceleratorReadback::new(
            accelerator,
            "`CellularTilePass.control_readbacks`",
            16,
            READBACK_COUNT,
        );
        Self {
            pipelines,
            bind_group,
            dispatch_bind_group,
            uniform_buffer,
            control_buffer,
            dispatch_buffer,
            actor_buffer,
            contact_buffer,
            contact_readbacks,
            tick: 0,
            control_readbacks,
            rigid_body_contact_buffer,
            rigid_fluid_tile_buffer,
            rigid_body_contact_readbacks,
            rigid_bodies: vec![0; COLLISION_RIGID_BODY_COUNT_MAXIMUM],
            substep_count: SUBSTEP_COUNT,
            resolve_count: CELLULAR_TILE_RESOLVE_COUNT,
        }
    }

    /// Simulates one tick of the dynamic cells of the active tiles against actors placed in tiles
    /// from the region's bottom-left corner and against rigid bodies at their record indices,
    /// without waiting for the GPU, and returns the velocity change in tiles per second, contact
    /// normal, and depth in tiles of every actor from the newest contacts read back, if any arrived
    /// since the last tick, and likewise the contacts of every rigid body
    pub fn simulate_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i32, i32),
        gravity: f32,
        actors: &[(Actor, ColliderShape, [f32; 2], f32, [f32; 2], f32, bool)],
        rigid_bodies: &[(u32, u64)],
        rigid_fluid_tiles: &[u32],
    ) -> Result<(
        Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>>,
        Option<HashMap<u64, ([f32; 2], f32, [f32; 7])>>,
    ), SceneError> {
        let contacts: Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>> =
            self.simulate_cellular_tiles_read_contacts(accelerator)?;
        let rigid_body_contacts: Option<HashMap<u64, ([f32; 2], f32, [f32; 7])>> =
            self.simulate_cellular_tiles_read_rigid_body_contacts(accelerator)?;
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let tile_count: u32 = width * height;
        self.simulate_cellular_tiles_read_control(accelerator)?;
        if u64::from(tile_count) * 4 + 12 > self.control_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let actors: &[(Actor, ColliderShape, [f32; 2], f32, [f32; 2], f32, bool)] =
            &actors[..actors.len().min(CELLULAR_TILE_ACTOR_COUNT_MAXIMUM)];
        // the shader measures everything in cells and ticks
        let cell_size: f32 = CELLULAR_TILE_SIZE as f32;
        let gravity: f32 = gravity * cell_size * CELLULAR_TILE_TIME_STEP * CELLULAR_TILE_TIME_STEP;
        let slot_count: usize = SUBSTEP_COUNT * CELLULAR_TILE_RESOLVE_COUNT;
        let mut uniform_bytes: Vec<u8> = vec![0; slot_count * UNIFORM_STRIDE as usize];
        for (slot, substep_bytes) in
            uniform_bytes.chunks_exact_mut(UNIFORM_STRIDE as usize).enumerate()
        {
            for (field, value) in [
                width,
                height,
                ring_offset.0,
                ring_offset.1,
                self.tick,
                (slot / CELLULAR_TILE_RESOLVE_COUNT) as u32,
                actors.len() as u32,
                gravity.to_bits(),
                (slot % CELLULAR_TILE_RESOLVE_COUNT) as u32,
                0,
                region_origin.0 as u32,
                region_origin.1 as u32,
            ].into_iter().enumerate() {
                substep_bytes[field * 4..field * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        if !rigid_fluid_tiles.is_empty() {
            let rigid_fluid_tile_bytes: Vec<u8> = rigid_fluid_tiles.iter()
                .flat_map(|tile| tile.to_le_bytes())
                .collect();
            accelerator.queue().write_buffer(
                &self.rigid_fluid_tile_buffer,
                0,
                &rigid_fluid_tile_bytes,
            );
        }
        // WGSL vec2<f32> center, velocity, rotation, and size, vec4<f32> bounds, u32 shape,
        // f32 mass, and swimming flag at offsets 0, 8, 16, 24, 32, 48, 52, and 56
        let mut actor_bytes: Vec<u8> = Vec::with_capacity(actors.len() * ACTOR_BYTE_COUNT as usize);
        for (_, shape, center, rotation, velocity, mass, swimming) in actors {
            let (shape, size): (u32, [f32; 2]) = match *shape {
                ColliderShape::Circle { radius } => (0, [radius, 0.0]),
                ColliderShape::Rectangle { width, height } => (1, [width / 2.0, height / 2.0]),
                ColliderShape::Capsule { width, height } =>
                    (2, [(height - width) / 2.0, width / 2.0]),
            };
            let center: [f32; 2] = [center[0] * cell_size, center[1] * cell_size];
            let size: [f32; 2] = [size[0] * cell_size, size[1] * cell_size];
            // bound the shape by its farthest extent plus one cell of margin
            let extent: f32 = match shape {
                0 => size[0],
                1 => size[0].hypot(size[1]),
                _ => size[0] + size[1],
            } + 2.5;
            for value in [
                center[0],
                center[1],
                (velocity[0] * cell_size).clamp(-8.0, 8.0),
                (velocity[1] * cell_size).clamp(-8.0, 8.0),
                rotation.cos(),
                rotation.sin(),
                size[0],
                size[1],
                center[0] - extent,
                center[1] - extent,
                center[0] + extent,
                center[1] + extent,
            ] {
                actor_bytes.extend_from_slice(&value.to_le_bytes());
            }
            actor_bytes.extend_from_slice(&shape.to_le_bytes());
            actor_bytes.extend_from_slice(&mass.to_le_bytes());
            actor_bytes.extend_from_slice(&u32::from(*swimming).to_le_bytes());
            actor_bytes.extend_from_slice(&[0; 4]);
        }
        if !actor_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.actor_buffer, 0, &actor_bytes);
        }
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene cellular tiles"),
            });
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(12));
        command_encoder.clear_buffer(&self.contact_buffer, 0, None);
        // contacts accumulated for a rigid body never reach the body replacing it at its index
        let mut live_rigid_bodies: Vec<u64> = vec![0; self.rigid_bodies.len()];
        for (index, rigid_body) in rigid_bodies {
            live_rigid_bodies[*index as usize] = *rigid_body;
        }
        for (index, (rigid_body, live_rigid_body)) in
            self.rigid_bodies.iter().zip(&live_rigid_bodies).enumerate()
        {
            if rigid_body == live_rigid_body { continue; }
            command_encoder.clear_buffer(
                &self.rigid_body_contact_buffer,
                index as u64 * RIGID_BODY_CONTACT_BYTE_COUNT,
                Some(RIGID_BODY_CONTACT_BYTE_COUNT),
            );
        }
        self.rigid_bodies = live_rigid_bodies;
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene cellular tiles"),
                    timestamp_writes: None,
                });
            let pipelines: &HashMap<&'static str, wgpu::ComputePipeline> = &self.pipelines;
            compute_pass.set_bind_group(0, &self.bind_group, &[0]);
            // list the active tiles and clear their flags for this tick's simulation to set
            compute_pass.set_pipeline(&pipelines["collect"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["clear"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            compute_pass.set_pipeline(&pipelines["integrate"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // size the substeps by the fastest cell, leaving the rest with no workgroups
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            for substep in 0..self.substep_count {
                let indirect_offset: u64 = 12 + substep as u64 * 12;
                let slot: usize = substep * CELLULAR_TILE_RESOLVE_COUNT;
                let set_slot = |compute_pass: &mut wgpu::ComputePass<'_>, slot: usize| {
                    compute_pass.set_bind_group(
                        0,
                        &self.bind_group,
                        &[(slot as u64 * UNIFORM_STRIDE) as u32],
                    );
                };
                set_slot(&mut compute_pass, slot);
                let dispatch = |compute_pass: &mut wgpu::ComputePass<'_>, entry_point: &str| {
                    compute_pass.set_pipeline(&pipelines[entry_point]);
                    compute_pass.dispatch_workgroups_indirect(
                        &self.dispatch_buffer,
                        indirect_offset,
                    );
                };
                for entry_point in ["request", "pick", "resolve_begin"] {
                    dispatch(&mut compute_pass, entry_point);
                }
                // each resolve pass doubles the length of the chains of cells that move together,
                // and reads its own uniforms so it can report chains still waiting after it
                for resolve in 0..self.resolve_count {
                    set_slot(&mut compute_pass, slot + resolve);
                    dispatch(
                        &mut compute_pass,
                        if resolve % 2 == 0 { "resolve_even" } else { "resolve_odd" },
                    );
                }
                // staging collides cells that stay using the second status, free once the even
                // count of resolve passes leaves the result in the first
                set_slot(&mut compute_pass, slot);
                for entry_point in ["stage", "commit"] {
                    dispatch(&mut compute_pass, entry_point);
                }
            }
            compute_pass.set_bind_group(0, &self.bind_group, &[0]);
            compute_pass.set_pipeline(&pipelines["contact"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // visit the full physical region so sleeping tiles still receive fluid forces, counting
            // each body's fluid faces first so its contacts share its mass
            compute_pass.set_pipeline(&pipelines["rigid_fluid_faces"]);
            compute_pass.dispatch_workgroups(rigid_fluid_tiles.len() as u32, 1, 1);
            compute_pass.set_pipeline(&pipelines["rigid_fluid"]);
            compute_pass.dispatch_workgroups(rigid_fluid_tiles.len() as u32, 1, 1);
        }
        // read the contacts back into a free readback, skipping them while every one is in flight
        if !actors.is_empty() {
            let readback_actors: Vec<(Actor, f32)> =
                actors.iter().map(|actor| (actor.0, actor.5)).collect();
            self.contact_readbacks.copy(
                &mut command_encoder,
                &self.contact_buffer,
                0,
                actors.len() as u64 * CONTACT_BYTE_COUNT,
                readback_actors,
            );
        }
        // rigid body contacts keep accumulating while every readback is in flight, so none are lost
        let rigid_body_readback: bool = if !rigid_bodies.is_empty() {
            self.rigid_body_contact_readbacks.copy(
                &mut command_encoder,
                &self.rigid_body_contact_buffer,
                0,
                self.rigid_body_contact_buffer.size(),
                self.rigid_bodies.clone(),
            )
        } else { false };
        if rigid_body_readback {
            command_encoder.clear_buffer(&self.rigid_body_contact_buffer, 0, None);
        }
        // control readbacks only size later ticks' work, so reading them never waits
        self.control_readbacks.copy(
            &mut command_encoder,
            &self.control_buffer,
            0,
            12,
            self.resolve_count,
        );
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        self.contact_readbacks.map(submission_index.clone());
        self.rigid_body_contact_readbacks.map(submission_index.clone());
        self.control_readbacks.map(submission_index);
        self.tick = self.tick.wrapping_add(1);
        Ok((contacts, rigid_body_contacts))
    }

    /// Returns the contacts of every rigid body summed over every readback that arrived since the
    /// last tick: impulse in mass tiles per second, angular impulse in mass tiles squared per
    /// second, then fluid currents in the newest readback, skipping rigid bodies replaced since
    fn simulate_cellular_tiles_read_rigid_body_contacts(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<HashMap<u64, ([f32; 2], f32, [f32; 7])>>, SceneError> {
        let mut rigid_body_contacts: Option<
            HashMap<u64, ([f32; 2], f32, [f32; 7])>
        > = None;
        let read_result: Result<(), AcceleratorReadbackError> = self.rigid_body_contact_readbacks
            .read(accelerator, true, |readback_rigid_bodies, contact_bytes| {
                let contacts: &mut HashMap<u64, ([f32; 2], f32, [f32; 7])> =
                    rigid_body_contacts.get_or_insert_with(HashMap::new);
                for ((readback_rigid_body, rigid_body), contact_bytes) in readback_rigid_bodies
                    .iter()
                    .zip(&self.rigid_bodies)
                    .zip(contact_bytes.chunks_exact(RIGID_BODY_CONTACT_BYTE_COUNT as usize))
                {
                    if *readback_rigid_body == 0 || readback_rigid_body != rigid_body { continue; }
                    let value = |field: usize| -> f32 {
                        f32::from_le_bytes([
                            contact_bytes[field * 4],
                            contact_bytes[field * 4 + 1],
                            contact_bytes[field * 4 + 2],
                            contact_bytes[field * 4 + 3],
                        ])
                    };
                    // convert impulses in mass cells per tick and arms in cells to tile units
                    let cell_size: f32 = 1.0 / CELLULAR_TILE_SIZE as f32;
                    let tiles_per_second: f32 = cell_size / CELLULAR_TILE_TIME_STEP;
                    let (impulse, angular_impulse, current): &mut ([f32; 2], f32, [f32; 7]) =
                        contacts.entry(*rigid_body).or_insert(([0.0; 2], 0.0, [0.0; 7]));
                    impulse[0] += value(0) * tiles_per_second;
                    impulse[1] += value(1) * tiles_per_second;
                    *angular_impulse += value(2) * cell_size * tiles_per_second;
                    // wet-face fluid mass, momentum in tile units, and arm moments in tiles
                    current[0] += value(4);
                    current[1] += value(5) * tiles_per_second;
                    current[2] += value(6) * tiles_per_second;
                    current[3] += value(7) * cell_size * tiles_per_second;
                    current[4] += value(8) * cell_size * cell_size;
                    current[5] += value(9) * cell_size;
                    current[6] += value(10) * cell_size;
                }
            });
        read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CellularTileResponse { error },
        })?;
        Ok(rigid_body_contacts)
    }

    /// Sizes the next ticks' substeps and resolve passes from the newest control readback, with
    /// a margin for cells that speed up and chains that lengthen before the next readback
    fn simulate_cellular_tiles_read_control(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<(), SceneError> {
        let read_result: Result<(), AcceleratorReadbackError> = self.control_readbacks.read(
            accelerator,
            false,
            |resolve_count, mapped_bytes| {
                let (step_count, pending_resolves): (u32, u32) = {
                    let value = |field: usize| -> u32 {
                        u32::from_le_bytes([
                            mapped_bytes[field * 4],
                            mapped_bytes[field * 4 + 1],
                            mapped_bytes[field * 4 + 2],
                            mapped_bytes[field * 4 + 3],
                        ])
                    };
                    (value(1), value(2))
                };
                self.substep_count = (step_count as usize + 1).clamp(1, SUBSTEP_COUNT);
                // the last resolve pass still pending means the chains were longer than it could see
                let needed_resolve_count: usize = match pending_resolves {
                    0 => 1,
                    _ if 32 - pending_resolves.leading_zeros() as usize >= resolve_count =>
                        CELLULAR_TILE_RESOLVE_COUNT,
                    _ => 32 - pending_resolves.leading_zeros() as usize + 1,
                };
                self.resolve_count = (needed_resolve_count + 1).next_multiple_of(2)
                    .clamp(2, CELLULAR_TILE_RESOLVE_COUNT);
            });
        read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CellularTileResponse { error },
        })
    }

    /// Returns the contacts of the newest readback that arrived, releasing every arrived readback
    fn simulate_cellular_tiles_read_contacts(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>>, SceneError> {
        let mut contacts: Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>> = None;
        let read_result: Result<(), AcceleratorReadbackError> = self.contact_readbacks.read(
            accelerator,
            true,
            |actors, contact_bytes| {
                // readbacks are mapped in submission order, so a later one replaces an earlier one
                let mut readback_contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)> =
                    HashMap::new();
                for ((actor, mass), contact_bytes) in
                    actors.iter().zip(contact_bytes.chunks_exact(CONTACT_BYTE_COUNT as usize))
                {
                    let value = |field: usize| -> f32 {
                        i32::from_le_bytes([
                            contact_bytes[field * 4],
                            contact_bytes[field * 4 + 1],
                            contact_bytes[field * 4 + 2],
                            contact_bytes[field * 4 + 3],
                        ]) as f32 / CONTACT_SCALE
                    };
                    // convert impulses in mass cells per tick and depths in cells to tile units
                    let tiles_per_second: f32 =
                        1.0 / CELLULAR_TILE_SIZE as f32 / CELLULAR_TILE_TIME_STEP;
                    let velocity_change: [f32; 2] =
                        [value(0) / mass * tiles_per_second, value(1) / mass * tiles_per_second];
                    let normal_length: f32 = value(2).hypot(value(3));
                    let normal: [f32; 2] = if normal_length > 0.0 {
                        [value(2) / normal_length, value(3) / normal_length]
                    } else { [0.0, 0.0] };
                    let depth: f32 = value(5) / CELLULAR_TILE_SIZE as f32;
                    if velocity_change != [0.0; 2] || normal != [0.0; 2] {
                        readback_contacts.insert(*actor, (velocity_change, normal, depth));
                    }
                }
                contacts = Some(readback_contacts);
            });
        read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CellularTileResponse { error },
        })?;
        Ok(contacts)
    }

}
