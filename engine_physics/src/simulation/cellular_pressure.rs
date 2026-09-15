// Copyright Rob Gage 2026

use crate::{
    materials::{
        Material,
        MaterialIdentifier,
        MaterialRegistry,
    },
    tiles::{
        CellCoordinates,
        TileCoordinates,
    },
};
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};
use super::{
    rigid_granular_readback_slot::RigidGranularReadbackSlot,
    rigid_granular_readback_status::RigidGranularReadbackStatus,
    RigidGranularReactionBatch,
};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        Arc,
        Mutex,
    },
};

const PRESSURE_DAMAGE_RATE: f32 = 10.0;
const RIGID_REACTION_READBACK_SLOT_COUNT: usize = 3;

/// Applies transient directional cellular pressure and static integrity damage
pub struct CellularPressure {
    static_properties: AcceleratorBuffer,
    dynamic_properties: AcceleratorBuffer,
    fluid_properties: AcceleratorBuffer,
    pending_impulses: AcceleratorBuffer,
    pressure_a: AcceleratorBuffer,
    pressure_b: AcceleratorBuffer,
    retained_pressure: AcceleratorBuffer,
    /// Transient logical-tile mask covering current pressure sources and their stencil halo
    active_tiles: AcceleratorBuffer,
    /// Dense logical indices of the currently active pressure tiles
    active_tile_indices: AcceleratorBuffer,
    /// GPU-written indirect dispatch record for active pressure tiles
    indirect_dispatch: wgpu::Buffer,
    rigid_contact_statistics: AcceleratorBuffer,
    rigid_reactions: AcceleratorBuffer,
    rigid_shadow_velocity: AcceleratorBuffer,
    rigid_sweep_reactions: AcceleratorBuffer,
    rigid_reaction_readback_slots: Box<[RigidGranularReadbackSlot]>,
    rigid_reaction_completed: BTreeMap<u64, RigidGranularReactionBatch>,
    rigid_reaction_sequence_next: u64,
    rigid_reaction_sequence_apply_next: u64,
    rigid_topology_revision: u64,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Compaction-only binding for the indirect dispatch record
    indirect_bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    /// Clears the coarse pressure work mask at the start of each fixed tick
    clear_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Finds current pressure sources and activates every tile their fixed stencil can reach
    mark_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Compacts the coarse mask for indirect cell-stage dispatches
    compact_active_tiles_pipeline: wgpu::ComputePipeline,
    gather_rigid_contacts_pipelines: [wgpu::ComputePipeline; 4],
    resolve_contacts_pipelines: [wgpu::ComputePipeline; 4],
    rigid_shadow_initialize_pipeline: wgpu::ComputePipeline,
    rigid_contact_support_clear_pipeline: wgpu::ComputePipeline,
    rigid_shadow_advance_pipeline: wgpu::ComputePipeline,
    seed_pipeline: wgpu::ComputePipeline,
    propagate_a_pipeline: wgpu::ComputePipeline,
    propagate_b_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    gas_count: u32,
    tick: u32,
}

impl CellularPressure {

    /// Returns the transient retained-pressure field for viewport visualization
    pub(crate) const fn retained_pressure(&self) -> &AcceleratorBuffer {
        &self.retained_pressure
    }

    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        material_ids: &AcceleratorBuffer,
        appearances: &AcceleratorBuffer,
        integrities: &AcceleratorBuffer,
        kinematics: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        external_body_velocity: &AcceleratorBuffer,
        rigid_owners: &AcceleratorBuffer,
        rigid_material_identifiers: &AcceleratorBuffer,
        rigid_transforms: &AcceleratorBuffer,
        mechanical_fluid_cells: &AcceleratorBuffer,
        gas_velocity: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        gas_properties: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        gas_count: u32,
        buffered_cell_count: usize,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let static_values: Vec<[u32; 8]> = materials.iter().filter_map(
            |(_, material)| match material {
                Material::CellularStatic {
                    pressure_ignore_threshold,
                    pressure_transmission,
                    debris_material,
                    debris_yield_rate,
                    friction,
                    restitution,
                    ..
                } => Some([
                    pressure_ignore_threshold.to_bits(),
                    pressure_transmission.to_bits(),
                    debris_material.unwrap_or(MaterialIdentifier::NULL).as_u32(),
                    debris_yield_rate.to_bits(),
                    friction.to_bits(),
                    restitution.to_bits(),
                    0,
                    0,
                ]),
                _ => None,
            },
        ).collect();
        let dynamic_values: Vec<[f32; 4]> = materials.iter().filter_map(
            |(_, material)| match material {
                Material::CellularDynamic {
                    mass,
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => Some([
                    *mass,
                    *pressure_transmission,
                    *friction,
                    *restitution,
                ]),
                _ => None,
            },
        ).collect();
        let fluid_values: Vec<[f32; 4]> = materials.iter().filter_map(
            |(_, material)| match material {
                Material::Fluid { pressure_transmission, friction, restitution, .. } =>
                    Some([*pressure_transmission, *friction, *restitution, 0.0]),
                _ => None,
            },
        ).collect();
        let static_properties: AcceleratorBuffer =
            accelerator.allocate::<[u32; 8]>(static_values.len().max(1));
        let dynamic_properties: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(dynamic_values.len().max(1));
        let fluid_properties: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(fluid_values.len().max(1));
        if !static_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                static_properties.wgpu_buffer(),
                0,
                &static_values.iter().flat_map(|values| {
                    values.iter().flat_map(|value| value.to_le_bytes())
                }).collect::<Vec<_>>(),
            );
        }
        if !dynamic_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                dynamic_properties.wgpu_buffer(),
                0,
                &dynamic_values.iter().flat_map(|values| {
                    values.iter().flat_map(|value| value.to_le_bytes())
                }).collect::<Vec<_>>(),
            );
        }
        if !fluid_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                fluid_properties.wgpu_buffer(), 0,
                &fluid_values.iter().flat_map(|values| {
                    values.iter().flat_map(|value| value.to_le_bytes())
                }).collect::<Vec<_>>(),
            );
        }
        let buffered_cell_count: u32 = buffered_cell_count.try_into()
            .expect("Cellular pressure buffer exceeds GPU indexing range");
        let buffered_tile_count: u32 = buffered_cell_count / 64;
        let pending_impulses: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let pressure_a: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let pressure_b: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let retained_pressure: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let active_tiles: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize);
        let active_tile_indices: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize);
        let indirect_dispatch: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure indirect dispatch"),
            size: 12,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT |
                wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rigid_contact_statistics: AcceleratorBuffer =
            accelerator.allocate::<[u32; 12]>(buffered_cell_count as usize);
        let rigid_reactions: AcceleratorBuffer =
            accelerator.allocate::<[i32; 4]>(buffered_cell_count as usize);
        let rigid_shadow_velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let rigid_sweep_reactions: AcceleratorBuffer =
            accelerator.allocate::<[i32; 4]>(buffered_cell_count as usize);
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular pressure bind group layout"),
                entries: &[
                    Self::storage_layout_entry(0, false),
                    Self::storage_layout_entry(1, false),
                    Self::storage_layout_entry(2, false),
                    Self::storage_layout_entry(3, false),
                    Self::storage_layout_entry(4, true),
                    Self::storage_layout_entry(5, true),
                    Self::storage_layout_entry(6, false),
                    Self::storage_layout_entry(7, false),
                    Self::storage_layout_entry(8, false),
                    Self::storage_layout_entry(9, false),
                    Self::storage_layout_entry(10, true),
                    Self::storage_layout_entry(11, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 13,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    Self::storage_layout_entry(14, false),
                    Self::storage_layout_entry(15, false),
                    Self::storage_layout_entry(16, false),
                    Self::storage_layout_entry(17, true),
                    Self::storage_layout_entry(18, false),
                    Self::storage_layout_entry(19, true),
                    Self::storage_layout_entry(20, true),
                    Self::storage_layout_entry(21, true),
                    Self::storage_layout_entry(22, true),
                    Self::storage_layout_entry(23, true),
                    Self::storage_layout_entry(24, true),
                    Self::storage_layout_entry(25, false),
                    Self::storage_layout_entry(26, false),
                    Self::storage_layout_entry(27, false),
                    Self::storage_layout_entry(28, false),
                ],
            },
        );
        let indirect_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular pressure indirect bind group layout"),
                entries: &[Self::storage_layout_entry(0, false)],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular pressure bind group"),
                layout: &layout,
                entries: &[
                    Self::binding(0, material_ids),
                    Self::binding(1, appearances),
                    Self::binding(2, integrities),
                    Self::binding(3, kinematics),
                    Self::binding(4, &static_properties),
                    Self::binding(5, &dynamic_properties),
                    Self::binding(6, &pending_impulses),
                    Self::binding(7, &pressure_a),
                    Self::binding(8, &pressure_b),
                    Self::binding(9, &retained_pressure),
                    Self::binding(10, external_body_occupancy),
                    Self::binding(11, external_body_velocity),
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: parameters.as_entire_binding(),
                    },
                    Self::binding(14, &active_tiles),
                    Self::binding(15, &active_tile_indices),
                    Self::binding(16, mechanical_fluid_cells),
                    Self::binding(17, &fluid_properties),
                    Self::binding(18, gas_velocity),
                    Self::binding(19, gas_concentrations),
                    Self::binding(20, gas_properties),
                    Self::binding(21, fluid_coverage),
                    Self::binding(22, rigid_owners),
                    Self::binding(23, rigid_material_identifiers),
                    Self::binding(24, rigid_transforms),
                    Self::binding(25, &rigid_shadow_velocity),
                    Self::binding(26, &rigid_sweep_reactions),
                    Self::binding(27, &rigid_reactions),
                    Self::binding(28, &rigid_contact_statistics),
                ],
            },
        );
        let indirect_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular pressure indirect bind group"),
                layout: &indirect_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: indirect_dispatch.as_entire_binding(),
                }],
            },
        );
        let shader: wgpu::ShaderModule = super::create_simulation_shader_module(
            device,
            "cellular pressure shader",
            include_str!("cellular_pressure.wgsl"),
            "engine_physics/src/simulation/cellular_pressure.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            },
        );
        let compact_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure compaction pipeline layout"),
                bind_group_layouts: &[Some(&layout), Some(&indirect_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let reaction_readback_size: u64 = u64::from(buffered_cell_count) * 64;
        let rigid_reaction_readback_slots: Box<[RigidGranularReadbackSlot]> =
            (0..RIGID_REACTION_READBACK_SLOT_COUNT).map(|_| RigidGranularReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rigid granular reaction readback"),
                    size: reaction_readback_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                status: Arc::new(Mutex::new(RigidGranularReadbackStatus::Available)),
            }).collect();
        Self {
            static_properties,
            dynamic_properties,
            fluid_properties,
            pending_impulses,
            pressure_a,
            pressure_b,
            retained_pressure,
            active_tiles,
            active_tile_indices,
            indirect_dispatch,
            rigid_contact_statistics,
            rigid_reactions,
            rigid_shadow_velocity,
            rigid_sweep_reactions,
            rigid_reaction_readback_slots,
            rigid_reaction_completed: BTreeMap::new(),
            rigid_reaction_sequence_next: 0,
            rigid_reaction_sequence_apply_next: 0,
            rigid_topology_revision: u64::MAX,
            parameters,
            bind_group,
            indirect_bind_group,
            impulse_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular impulse pipeline", "queue_cellular_radial_impulse",
            ),
            clear_active_tiles_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure active tile clear pipeline",
                "clear_active_cellular_pressure_tiles",
            ),
            mark_active_tiles_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure active tile marking pipeline",
                "mark_active_cellular_pressure_tiles",
            ),
            compact_active_tiles_pipeline: Self::create_pipeline(
                device, &compact_pipeline_layout, &shader,
                "cellular pressure active tile compaction pipeline",
                "compact_active_cellular_pressure_tiles",
            ),
            gather_rigid_contacts_pipelines: [
                "gather_rigid_contacts_horizontal_even",
                "gather_rigid_contacts_horizontal_odd",
                "gather_rigid_contacts_vertical_even",
                "gather_rigid_contacts_vertical_odd",
            ].map(|entry| Self::create_pipeline(
                device, &pipeline_layout, &shader, entry, entry,
            )),
            resolve_contacts_pipelines: [
                "resolve_cellular_contacts_horizontal_even",
                "resolve_cellular_contacts_horizontal_odd",
                "resolve_cellular_contacts_vertical_even",
                "resolve_cellular_contacts_vertical_odd",
            ].map(|entry| Self::create_pipeline(
                device, &pipeline_layout, &shader, entry, entry,
            )),
            rigid_shadow_initialize_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "initialize rigid shadow velocity", "initialize_rigid_shadow_velocity",
            ),
            rigid_contact_support_clear_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "clear rigid contact support", "clear_rigid_contact_support",
            ),
            rigid_shadow_advance_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "advance rigid shadow velocity", "advance_rigid_shadow_velocity",
            ),
            seed_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure seed pipeline", "seed_cellular_pressure",
            ),
            propagate_a_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure A propagation", "propagate_cellular_pressure_a",
            ),
            propagate_b_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular pressure B propagation", "propagate_cellular_pressure_b",
            ),
            apply_pipeline: Self::create_pipeline(
                device, &pipeline_layout, &shader,
                "cellular retained pressure pipeline", "apply_retained_cellular_pressure",
            ),
            buffered_cell_count,
            gas_count,
            tick: 0,
        }
    }

    pub fn apply_radial_impulse(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
    ) {
        self.write_parameters(
            accelerator, origin, width, height, ring_x, ring_y,
            center, radius, strength, 0.0, 0,
        );
        self.dispatch(accelerator, &self.impulse_pipeline, "queue cellular radial impulse");
    }

    pub fn simulate(
        &mut self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        delta_time: f32,
        rigid_body_count: usize,
        rigid_topology_revision: u64,
    ) -> Result<(), io::Error> {
        let rigid_body_count: u32 = rigid_body_count.try_into()
            .map_err(|_| io::Error::other("Rigid body count exceeds GPU indexing range"))?;
        if rigid_body_count > self.buffered_cell_count {
            return Err(io::Error::other("Rigid body count exceeds contact buffer capacity"));
        }
        self.write_parameters(
            accelerator, origin, width, height, ring_x, ring_y,
            CellCoordinates { x: 0, y: 0 }, 0.0, 0.0, delta_time, rigid_body_count,
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cellular pressure simulation"),
            });
        if self.rigid_topology_revision != rigid_topology_revision {
            encoder.clear_buffer(self.rigid_reactions.wgpu_buffer(), 0, None);
            self.rigid_topology_revision = rigid_topology_revision;
        }
        let tile_count: u32 = self.buffered_cell_count / 64;
        for (pipeline, label) in [
            (&self.clear_active_tiles_pipeline, "clear active cellular pressure tiles"),
            (&self.mark_active_tiles_pipeline, "mark active cellular pressure tiles"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                &mut encoder,
                "compact active cellular pressure tiles",
            );
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                &mut encoder, "initialize rigid shadow state",
            );
            pass.set_pipeline(&self.rigid_shadow_initialize_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_body_count.max(1).div_ceil(64), 1, 1);
        }
        for _sweep in 0..2 {
            {
                let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                    &mut encoder, "clear rigid contact support",
                );
                pass.set_pipeline(&self.rigid_contact_support_clear_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups(rigid_body_count.max(1).div_ceil(64), 1, 1);
            }
            for pipeline in &self.gather_rigid_contacts_pipelines {
                let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                    &mut encoder, "gather colored rigid contacts",
                );
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
            }
            for pipeline in &self.resolve_contacts_pipelines {
                let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                    &mut encoder, "resolve colored cellular faces",
                );
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
            }
            let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(
                &mut encoder, "advance rigid shadow between sweeps",
            );
            pass.set_pipeline(&self.rigid_shadow_advance_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_body_count.max(1).div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (&self.seed_pipeline, "seed cellular pressure"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 1"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 1"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 2"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 2"),
            (&self.propagate_a_pipeline, "propagate cellular pressure A 3"),
            (&self.propagate_b_pipeline, "propagate cellular pressure B 3"),
            (&self.apply_pipeline, "apply retained cellular pressure"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        let readback_slot: Option<&RigidGranularReadbackSlot> =
            self.rigid_reaction_readback_slots.iter().find(|slot| {
                slot.status.lock().is_ok_and(|status| {
                    matches!(*status, RigidGranularReadbackStatus::Available)
                })
            });
        let mut mapping = None;
        if rigid_body_count != 0 {
            if let Some(slot) = readback_slot {
                let mut status = slot.status.lock().map_err(|_| {
                    io::Error::other("Rigid granular readback state is unavailable")
                })?;
                if matches!(*status, RigidGranularReadbackStatus::Available) {
                    *status = RigidGranularReadbackStatus::Mapping;
                    let sequence: u64 = self.rigid_reaction_sequence_next;
                    self.rigid_reaction_sequence_next =
                        self.rigid_reaction_sequence_next.wrapping_add(1);
                    let reaction_size: u64 = u64::from(rigid_body_count) * 16;
                    let statistics_size: u64 = u64::from(rigid_body_count) * 48;
                    let statistics_offset: u64 = reaction_size;
                    encoder.copy_buffer_to_buffer(
                        self.rigid_reactions.wgpu_buffer(), 0,
                        &slot.buffer, 0, reaction_size,
                    );
                    encoder.copy_buffer_to_buffer(
                        self.rigid_contact_statistics.wgpu_buffer(), 0,
                        &slot.buffer, statistics_offset, statistics_size,
                    );
                    encoder.clear_buffer(self.rigid_reactions.wgpu_buffer(), 0, Some(reaction_size));
                    mapping = Some((slot, sequence, statistics_offset + statistics_size));
                }
            }
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        if let Some((slot, sequence, mapped_size)) = mapping {
            let mapped_buffer: wgpu::Buffer = slot.buffer.clone();
            let callback_status = slot.status.clone();
            let body_count: usize = rigid_body_count as usize;
            mapped_buffer.clone().slice(0..mapped_size).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let result: Result<RigidGranularReactionBatch, String> = match result {
                        Ok(()) => mapped_buffer.slice(0..mapped_size).get_mapped_range()
                            .map_err(|error| error.to_string()).and_then(|mapped| {
                                let mut reactions = Vec::with_capacity(body_count);
                                for bytes in mapped[..body_count * 16].chunks_exact(16) {
                                    let overflow = i32::from_le_bytes(
                                        bytes[12..16].try_into().unwrap(),
                                    );
                                    if overflow != 0 {
                                        drop(mapped);
                                        mapped_buffer.unmap();
                                        return Err(
                                            "Rigid granular reaction accumulator overflowed"
                                                .to_owned(),
                                        );
                                    }
                                    reactions.push([
                                        i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as f32 /
                                            256.0,
                                        i32::from_le_bytes(bytes[4..8].try_into().unwrap()) as f32 /
                                            256.0,
                                        i32::from_le_bytes(bytes[8..12].try_into().unwrap()) as f32 /
                                            64.0,
                                    ]);
                                }
                                let statistics_start: usize = body_count * 16;
                                let contact_counts = mapped[statistics_start..
                                    statistics_start + body_count * 48].chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[0..4].try_into().unwrap())
                                    }).collect::<Vec<_>>().into_boxed_slice();
                                let static_contact_counts = mapped[statistics_start..
                                    statistics_start + body_count * 48].chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[4..8].try_into().unwrap())
                                    }).collect::<Vec<_>>().into_boxed_slice();
                                let energy_budgets = mapped[statistics_start..
                                    statistics_start + body_count * 48].chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[8..12].try_into().unwrap())
                                            as f32 / 256.0
                                    }).collect::<Vec<_>>().into_boxed_slice();
                                drop(mapped);
                                mapped_buffer.unmap();
                                Ok(RigidGranularReactionBatch {
                                    sequence,
                                    topology_revision: rigid_topology_revision,
                                    body_count,
                                    reactions: reactions.into_boxed_slice(),
                                    contact_counts,
                                    static_contact_counts,
                                    energy_budgets,
                                })
                            }),
                        Err(_) => Err("Rigid granular reaction readback failed".to_owned()),
                    };
                    if let Ok(mut status) = callback_status.lock() {
                        *status = RigidGranularReadbackStatus::Complete(result);
                    }
                },
            );
        }
        self.tick = self.tick.wrapping_add(1);
        Ok(())
    }

    /// Collects completed mappings and returns every now-contiguous ordered batch
    pub(crate) fn collect_rigid_reactions(
        &mut self,
    ) -> Result<Vec<RigidGranularReactionBatch>, io::Error> {
        for slot in &self.rigid_reaction_readback_slots {
            let mut status = slot.status.lock().map_err(|_| {
                io::Error::other("Rigid granular readback state is unavailable")
            })?;
            if matches!(*status, RigidGranularReadbackStatus::Complete(_)) {
                let RigidGranularReadbackStatus::Complete(result) =
                    std::mem::replace(&mut *status, RigidGranularReadbackStatus::Available)
                else { unreachable!() };
                let batch = result.map_err(io::Error::other)?;
                self.rigid_reaction_completed.insert(batch.sequence, batch);
            }
        }
        let mut ordered = Vec::new();
        while let Some(batch) = self.rigid_reaction_completed
                .remove(&self.rigid_reaction_sequence_apply_next) {
            ordered.push(batch);
            self.rigid_reaction_sequence_apply_next =
                self.rigid_reaction_sequence_apply_next.wrapping_add(1);
        }
        Ok(ordered)
    }

    pub fn clear_transient_state(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        let zeroes: Vec<u8> = vec![0; cell_count * 16];
        let offset: u64 = cell_start as u64 * 16;
        for buffer in [
            &self.pending_impulses,
            &self.pressure_a,
            &self.pressure_b,
            &self.retained_pressure,
        ] {
            accelerator.wgpu_queue().write_buffer(buffer.wgpu_buffer(), offset, &zeroes);
        }
    }

    fn dispatch(
        &self,
        accelerator: &Accelerator,
        pipeline: &wgpu::ComputePipeline,
        label: &str,
    ) {
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        let mut pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn write_parameters(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
        delta_time: f32,
        rigid_body_count: u32,
    ) {
        let values: [u32; 16] = [
            origin.x as u32,
            origin.y as u32,
            u32::from(width),
            u32::from(height),
            u32::from(ring_x),
            u32::from(ring_y),
            (center.x as f32 + 0.5).to_bits(),
            (center.y as f32 + 0.5).to_bits(),
            radius.to_bits(),
            strength.to_bits(),
            delta_time.to_bits(),
            self.tick,
            self.buffered_cell_count,
            PRESSURE_DAMAGE_RATE.to_bits(),
            self.gas_count,
            rigid_body_count,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
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
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        label: &str,
        entry_point: &str,
    ) -> wgpu::ComputePipeline {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

}

impl Drop for CellularPressure {

    fn drop(&mut self) {
        self.static_properties.free();
        self.dynamic_properties.free();
        self.fluid_properties.free();
        self.pending_impulses.free();
        self.pressure_a.free();
        self.pressure_b.free();
        self.retained_pressure.free();
        self.active_tiles.free();
        self.active_tile_indices.free();
        self.rigid_contact_statistics.free();
        self.rigid_reactions.free();
        self.rigid_shadow_velocity.free();
        self.rigid_sweep_reactions.free();
        self.indirect_dispatch.destroy();
        self.parameters.destroy();
        for slot in &self.rigid_reaction_readback_slots { slot.buffer.destroy(); }
    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use engine_graphics::{Color, MaterialAppearance};
    use std::{sync::mpsc, time::{Duration, Instant}};

    #[test]
    fn colored_face_pipelines_compile_on_gpu() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let materials = MaterialRegistry::new();
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let occupancy = accelerator.allocate::<u32>(64);
        let velocity = accelerator.allocate::<[f32; 4]>(64);
        let owners = accelerator.allocate::<u32>(64);
        let rigid_materials = accelerator.allocate::<u32>(64);
        let transforms = accelerator.allocate::<[f32; 4]>(3);
        let pressure = CellularPressure::new(
            &accelerator, &materials, &cells, &appearances, &integrities,
            &kinematics, &occupancy, &velocity, &owners, &rigid_materials,
            &transforms, &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<[f32; 2]>(64), &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[f32; 4]>(2), &accelerator.allocate::<f32>(64),
            0, 64,
        );
        accelerator.poll().unwrap();
        drop(pressure);
    }

    #[test]
    fn falling_cell_transfers_momentum_into_anchored_cell_without_pressure_kick() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
            mass: 1.0, pressure_ignore_threshold: 1000.0, default_integrity: 100.0,
            debris_material: None, debris_yield_rate: 0.0,
            pressure_transmission: 1.0, friction: 0.5, restitution: 0.0,
        });
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(200, 180, 130)),
            mass: 1.0, pressure_transmission: 1.0, friction: 0.5, restitution: 0.0,
        });
        let cells = accelerator.allocate::<u32>(64);
        let appearances = accelerator.allocate::<u32>(64);
        let integrities = accelerator.allocate::<f32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let occupancy = accelerator.allocate::<u32>(64);
        let external_velocity = accelerator.allocate::<[f32; 4]>(64);
        let owners = accelerator.allocate::<u32>(64);
        let rigid_materials = accelerator.allocate::<u32>(64);
        let transforms = accelerator.allocate::<[f32; 4]>(3);
        let fluid = accelerator.allocate::<[u32; 4]>(64);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let gas_properties = accelerator.allocate::<[f32; 4]>(2);
        let fluid_coverage = accelerator.allocate::<f32>(64);
        let mut pressure = CellularPressure::new(
            &accelerator, &materials, &cells, &appearances, &integrities,
            &kinematics, &occupancy, &external_velocity, &owners, &rigid_materials,
            &transforms, &fluid, &gas_velocity, &gas_concentrations,
            &gas_properties, &fluid_coverage, 0, 64,
        );
        accelerator.wgpu_queue().write_buffer(cells.wgpu_buffer(), 0,
            &stone.as_u32().to_le_bytes());
        accelerator.wgpu_queue().write_buffer(cells.wgpu_buffer(), 8 * 4,
            &sand.as_u32().to_le_bytes());
        accelerator.wgpu_queue().write_buffer(kinematics.wgpu_buffer(), 8 * 16 + 4,
            &(-1.0f32).to_le_bytes());
        pressure.simulate(&accelerator, TileCoordinates { x: 0, y: 0 },
            1, 1, 0, 0, 1.0 / 60.0, 0, 0).unwrap();
        let readback = accelerator.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular face check"), size: 64 * 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular face check") });
        encoder.copy_buffer_to_buffer(kinematics.wgpu_buffer(), 0, &readback, 0, 64 * 16);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
        let started = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if receiver.try_recv().is_ok() { break; }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let velocity_y = f32::from_le_bytes(mapped[8 * 16 + 4..8 * 16 + 8]
            .try_into().unwrap());
        assert!(velocity_y.abs() < 0.01, "unsupported pressure kick: {velocity_y}");
    }

}
