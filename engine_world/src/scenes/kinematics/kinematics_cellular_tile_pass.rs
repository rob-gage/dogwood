// Copyright Rob Gage 2026

use super::{
    KINEMATICS_ACTOR_COUNT_MAXIMUM,
    KINEMATICS_RESOLVE_COUNT,
    KINEMATICS_TIME_STEP,
};
use crate::{
    actors::Actor,
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        collisions::ColliderShape,
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;
use std::{
    collections::HashMap,
    sync::mpsc::{
        Receiver,
        Sender,
        channel,
    },
};

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

/// Number of contact and control readbacks that can each be in flight at once
const READBACK_COUNT: usize = 3;

/// GPU simulation of the dynamic cells of every active cellular tile
pub struct KinematicsCellularTilePass {
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
    contact_readbacks: Vec<(wgpu::Buffer, Option<Vec<(Actor, f32)>>)>,
    /// Sender for contact readback mapping results
    response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Contact readback mapping results
    response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Number of simulated ticks, varying the order of tied moves
    tick: u32,
    /// Mappable copies of the control counts, each with the resolve count it was simulated with
    control_readbacks: Vec<(wgpu::Buffer, Option<usize>)>,
    /// Sender for control readback mapping results
    control_response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Control readback mapping results
    control_response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Substeps encoded per tick, from the substeps recent ticks needed
    substep_count: usize,
    /// Resolve passes encoded per substep, from the chain lengths recent ticks resolved
    resolve_count: usize,
}

impl KinematicsCellularTilePass {

    /// Creates simulation resources for every tile the cellular tile buffers can hold
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
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
                label: Some("`KinematicsCellularTilePass.bind_group_layout`"),
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
                ],
            },
        );
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`KinematicsCellularTilePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        // only the prepare step writes the indirect dispatches the other steps read
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`KinematicsCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`KinematicsCellularTilePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&dispatch_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`KinematicsCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("kinematics_cellular_tile.wgsl").into(),
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
        ] {
            let layout: &wgpu::PipelineLayout = if entry_point == "prepare" {
                &prepare_pipeline_layout
            } else {
                &pipeline_layout
            };
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`KinematicsCellularTilePass.pipelines`"),
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
            "`KinematicsCellularTilePass.uniform_buffer`",
            UNIFORM_STRIDE * (SUBSTEP_COUNT * KINEMATICS_RESOLVE_COUNT) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // each cell's requested move, pick, and two resolution statuses
        let motion_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.motion_buffer`",
            cell_count * 16,
            wgpu::BufferUsages::STORAGE,
        );
        // each cell's incoming material identifier, color, and kinematics
        let staged_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.staged_buffer`",
            cell_count * 16,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.control_buffer`",
            12 + tile_count * 4,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.dispatch_buffer`",
            (1 + SUBSTEP_COUNT as u64) * 12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let actor_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.actor_buffer`",
            KINEMATICS_ACTOR_COUNT_MAXIMUM as u64 * ACTOR_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let contact_buffer: wgpu::Buffer = create_buffer(
            "`KinematicsCellularTilePass.contact_buffer`",
            KINEMATICS_ACTOR_COUNT_MAXIMUM as u64 * CONTACT_BYTE_COUNT,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let contact_readbacks: Vec<(wgpu::Buffer, Option<Vec<(Actor, f32)>>)> =
            (0..READBACK_COUNT).map(|_| (
                create_buffer(
                    "`KinematicsCellularTilePass.contact_readbacks`",
                    contact_buffer.size(),
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
                None,
            )).collect();
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`KinematicsCellularTilePass.bind_group`"),
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
            ],
        });
        let dispatch_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`KinematicsCellularTilePass.dispatch_bind_group`"),
                layout: &dispatch_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            },
        );
        let (response_sender, response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        let (control_response_sender, control_response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        let control_readbacks: Vec<(wgpu::Buffer, Option<usize>)> =
            (0..READBACK_COUNT).map(|_| (
                create_buffer(
                    "`KinematicsCellularTilePass.control_readbacks`",
                    16,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
                None,
            )).collect();
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
            response_sender,
            response_receiver,
            tick: 0,
            control_readbacks,
            control_response_sender,
            control_response_receiver,
            substep_count: SUBSTEP_COUNT,
            resolve_count: KINEMATICS_RESOLVE_COUNT,
        }
    }

    /// Simulates one tick of the dynamic cells of the active tiles against actors placed in tiles
    /// from the region's bottom-left corner, without waiting for the GPU, and returns the velocity
    /// change in tiles per second, contact normal, and depth in tiles of every actor from the
    /// newest contacts read back, if any arrived since the last tick
    pub fn simulate_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        actors: &[(Actor, ColliderShape, [f32; 2], f32, [f32; 2], f32)],
    ) -> Result<Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>>, SceneError> {
        let contacts: Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>> =
            self.simulate_cellular_tiles_read_contacts(accelerator)?;
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let tile_count: u32 = width * height;
        self.simulate_cellular_tiles_read_control(accelerator)?;
        if u64::from(tile_count) * 4 + 12 > self.control_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let actors: &[(Actor, ColliderShape, [f32; 2], f32, [f32; 2], f32)] =
            &actors[..actors.len().min(KINEMATICS_ACTOR_COUNT_MAXIMUM)];
        // the shader measures everything in cells and ticks
        let cell_size: f32 = CELLULAR_TILE_SIZE as f32;
        let gravity: f32 = gravity * cell_size * KINEMATICS_TIME_STEP * KINEMATICS_TIME_STEP;
        let slot_count: usize = SUBSTEP_COUNT * KINEMATICS_RESOLVE_COUNT;
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
                (slot / KINEMATICS_RESOLVE_COUNT) as u32,
                actors.len() as u32,
                gravity.to_bits(),
                (slot % KINEMATICS_RESOLVE_COUNT) as u32,
            ].into_iter().enumerate() {
                substep_bytes[field * 4..field * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        // WGSL vec2<f32> center, velocity, rotation, and size, vec4<f32> bounds, u32 shape,
        // f32 mass, and padding at offsets 0, 8, 16, 24, 32, 48, 52, and 56
        let mut actor_bytes: Vec<u8> = Vec::with_capacity(actors.len() * ACTOR_BYTE_COUNT as usize);
        for (_, shape, center, rotation, velocity, mass) in actors {
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
            } + 1.0;
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
            actor_bytes.extend_from_slice(&[0; 8]);
        }
        if !actor_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.actor_buffer, 0, &actor_bytes);
        }
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene kinematics cellular tiles"),
            });
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(12));
        command_encoder.clear_buffer(&self.contact_buffer, 0, None);
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene kinematics cellular tiles"),
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
                let slot: usize = substep * KINEMATICS_RESOLVE_COUNT;
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
                set_slot(&mut compute_pass, slot);
                for entry_point in ["stage", "commit"] {
                    dispatch(&mut compute_pass, entry_point);
                }
            }
            compute_pass.set_bind_group(0, &self.bind_group, &[0]);
            compute_pass.set_pipeline(&pipelines["contact"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
        }
        // read the contacts back into a free readback, skipping them while every one is in flight
        let free_readback: Option<usize> = self.contact_readbacks.iter()
            .position(|(_, readback_actors)| readback_actors.is_none());
        let readback: Option<usize> = free_readback.filter(|_| !actors.is_empty());
        if let Some(readback) = readback {
            command_encoder.copy_buffer_to_buffer(
                &self.contact_buffer,
                0,
                &self.contact_readbacks[readback].0,
                0,
                actors.len() as u64 * CONTACT_BYTE_COUNT,
            );
        }
        let control_readback: Option<usize> = self.control_readbacks.iter()
            .position(|(_, resolve_count)| resolve_count.is_none());
        if let Some(control_readback) = control_readback {
            command_encoder.copy_buffer_to_buffer(
                &self.control_buffer,
                0,
                &self.control_readbacks[control_readback].0,
                0,
                12,
            );
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        if let Some(control_readback) = control_readback {
            let (buffer, resolve_count): &mut (wgpu::Buffer, Option<usize>) =
                &mut self.control_readbacks[control_readback];
            *resolve_count = Some(self.resolve_count);
            let response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
                self.control_response_sender.clone();
            buffer.map_async(
                wgpu::MapMode::Read,
                ..12,
                move |result: Result<(), wgpu::BufferAsyncError>| {
                    let _ = response_sender.send((control_readback, result));
                },
            );
        }
        if let Some(readback) = readback {
            let (buffer, readback_actors): &mut (wgpu::Buffer, Option<Vec<(Actor, f32)>>) =
                &mut self.contact_readbacks[readback];
            *readback_actors = Some(actors.iter().map(|actor| (actor.0, actor.5)).collect());
            let response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
                self.response_sender.clone();
            buffer.map_async(
                wgpu::MapMode::Read,
                ..,
                move |result: Result<(), wgpu::BufferAsyncError>| {
                    let _ = response_sender.send((readback, result));
                },
            );
        }
        self.tick = self.tick.wrapping_add(1);
        Ok(contacts)
    }

    /// Sizes the next ticks' substeps and resolve passes from the newest control readback, with
    /// a margin for cells that speed up and chains that lengthen before the next readback
    fn simulate_cellular_tiles_read_control(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<(), SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll)
            .map_err(|_| SceneError::KinematicsCellularTilePoll)?;
        while let Ok((readback, result)) = self.control_response_receiver.try_recv() {
            let (buffer, resolve_count): &mut (wgpu::Buffer, Option<usize>) =
                &mut self.control_readbacks[readback];
            let Some(resolve_count) = resolve_count.take() else { continue; };
            result.map_err(|error| SceneError::KinematicsCellularTileResponse { error })?;
            let (step_count, pending_resolves): (u32, u32) = {
                let mapped_bytes: wgpu::BufferView = buffer.get_mapped_range(..12)
                    .expect("mapped kinematics control readback should be readable");
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
            buffer.unmap();
            self.substep_count = (step_count as usize + 1).clamp(1, SUBSTEP_COUNT);
            // the last resolve pass still pending means the chains were longer than it could see
            let needed_resolve_count: usize = match pending_resolves {
                0 => 1,
                _ if 32 - pending_resolves.leading_zeros() as usize >= resolve_count =>
                    KINEMATICS_RESOLVE_COUNT,
                _ => 32 - pending_resolves.leading_zeros() as usize + 1,
            };
            self.resolve_count = (needed_resolve_count + 1).next_multiple_of(2)
                .clamp(2, KINEMATICS_RESOLVE_COUNT);
        }
        Ok(())
    }

    /// Returns the contacts of the newest readback that arrived, releasing every arrived readback
    fn simulate_cellular_tiles_read_contacts(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>>, SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll)
            .map_err(|_| SceneError::KinematicsCellularTilePoll)?;
        let mut contacts: Option<HashMap<Actor, ([f32; 2], [f32; 2], f32)>> = None;
        let mut first_error: Option<SceneError> = None;
        while let Ok((readback, result)) = self.response_receiver.try_recv() {
            let (buffer, readback_actors): &mut (wgpu::Buffer, Option<Vec<(Actor, f32)>>) =
                &mut self.contact_readbacks[readback];
            let Some(actors) = readback_actors.take() else { continue; };
            if let Err(error) = result {
                first_error.get_or_insert(SceneError::KinematicsCellularTileResponse { error });
                continue;
            }
            // readbacks are mapped in submission order, so a later one replaces an earlier one
            let mut readback_contacts: HashMap<Actor, ([f32; 2], [f32; 2], f32)> = HashMap::new();
            {
                let mapped_bytes: wgpu::BufferView = buffer.get_mapped_range(..)
                    .expect("mapped kinematics contact readback should be readable");
                for ((actor, mass), contact_bytes) in
                    actors.iter().zip(mapped_bytes.chunks_exact(CONTACT_BYTE_COUNT as usize))
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
                        1.0 / CELLULAR_TILE_SIZE as f32 / KINEMATICS_TIME_STEP;
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
            }
            buffer.unmap();
            contacts = Some(readback_contacts);
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(contacts),
        }
    }

}
