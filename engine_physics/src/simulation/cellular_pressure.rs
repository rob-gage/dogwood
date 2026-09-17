// Copyright Rob Gage 2026

use super::{
    RigidGranularReactionBatch, rigid_granular_readback_slot::RigidGranularReadbackSlot,
    rigid_granular_readback_status::RigidGranularReadbackStatus,
};
use crate::{
    materials::{Material, MaterialIdentifier, MaterialRegistry},
    tiles::{CellCoordinates, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

const PRESSURE_DAMAGE_RATE: f32 = 10.0;
const RIGID_REACTION_READBACK_SLOT_COUNT: usize = 3;
const INITIAL_RIGID_BODY_CAPACITY: usize = 16;

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
    rigid_predicted_motion: AcceleratorBuffer,
    rigid_damage: AcceleratorBuffer,
    rigid_fractures: AcceleratorBuffer,
    rigid_damage_dispatch: wgpu::Buffer,
    rigid_fracture_count: wgpu::Buffer,
    rigid_reaction_readback_slots: Vec<RigidGranularReadbackSlot>,
    rigid_reaction_completed: BTreeMap<u64, RigidGranularReactionBatch>,
    rigid_reaction_sequence_next: u64,
    rigid_reaction_sequence_apply_next: u64,
    rigid_topology_revision: u64,
    rigid_fracture_word_count: u64,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    bound_buffers: Vec<(u32, wgpu::Buffer)>,
    rigid_body_capacity: usize,
    /// Compaction-only binding for the indirect dispatch record
    indirect_bind_group: wgpu::BindGroup,
    rigid_damage_bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    /// Clears the coarse pressure work mask at the start of each fixed tick
    clear_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Finds current pressure sources and activates every tile their fixed stencil can reach
    mark_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Compacts the coarse mask for indirect cell-stage dispatches
    compact_active_tiles_pipeline: wgpu::ComputePipeline,
    gather_rigid_contacts_pipeline: wgpu::ComputePipeline,
    gather_rigid_static_contacts_pipeline: wgpu::ComputePipeline,
    resolve_rigid_static_contacts_pipeline: wgpu::ComputePipeline,
    resolve_contacts_pipelines: [wgpu::ComputePipeline; 4],
    rigid_contact_initialize_pipeline: wgpu::ComputePipeline,
    propagate_pending_pipeline: wgpu::ComputePipeline,
    propagate_a_pipeline: wgpu::ComputePipeline,
    propagate_b_pipeline: wgpu::ComputePipeline,
    finalize_pipeline: wgpu::ComputePipeline,
    apply_rigid_damage_pipeline: wgpu::ComputePipeline,
    mark_pressure_active_tiles_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    gas_count: u32,
    tick: u32,
}

impl CellularPressure {
    /// Returns the transient retained-pressure field for viewport visualization
    pub(crate) const fn retained_pressure(&self) -> &AcceleratorBuffer {
        &self.retained_pressure
    }
    /// Local pressure-source accumulator shared with chemistry before the
    /// ordinary pressure propagation stage.
    pub(crate) const fn pending_pressure(&self) -> &AcceleratorBuffer {
        &self.pending_impulses
    }

    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        material_ids: &AcceleratorBuffer,
        appearances: &AcceleratorBuffer,
        integrities: &AcceleratorBuffer,
        rigid_integrities: &AcceleratorBuffer,
        kinematics: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        external_body_velocity: &AcceleratorBuffer,
        rigid_owners: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_material_identifiers: &AcceleratorBuffer,
        rigid_transforms: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        mechanical_fluid_cells: &AcceleratorBuffer,
        gas_velocity: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        gas_properties: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        mutation_requests: &AcceleratorBuffer,
        mutation_request_count: &AcceleratorBuffer,
        gas_count: u32,
        buffered_cell_count: usize,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let static_values: Vec<[u32; 8]> = materials
            .iter()
            .filter_map(|(_, material)| match material {
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
            })
            .collect();
        let dynamic_values: Vec<[f32; 4]> = materials
            .iter()
            .filter_map(|(_, material)| match material {
                Material::CellularDynamic {
                    mass,
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => Some([*mass, *pressure_transmission, *friction, *restitution]),
                _ => None,
            })
            .collect();
        let fluid_values: Vec<[f32; 4]> = materials
            .iter()
            .filter_map(|(_, material)| match material {
                Material::Fluid {
                    pressure_transmission,
                    friction,
                    restitution,
                    ..
                } => Some([*pressure_transmission, *friction, *restitution, 0.0]),
                _ => None,
            })
            .collect();
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
                &static_values
                    .iter()
                    .flat_map(|values| values.iter().flat_map(|value| value.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
        }
        if !dynamic_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                dynamic_properties.wgpu_buffer(),
                0,
                &dynamic_values
                    .iter()
                    .flat_map(|values| values.iter().flat_map(|value| value.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
        }
        if !fluid_values.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                fluid_properties.wgpu_buffer(),
                0,
                &fluid_values
                    .iter()
                    .flat_map(|values| values.iter().flat_map(|value| value.to_le_bytes()))
                    .collect::<Vec<_>>(),
            );
        }
        let buffered_cell_count: u32 = buffered_cell_count
            .try_into()
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
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rigid_body_capacity: usize = INITIAL_RIGID_BODY_CAPACITY;
        let rigid_contact_statistics: AcceleratorBuffer =
            accelerator.allocate::<[u32; 12]>(rigid_body_capacity);
        let rigid_reactions: AcceleratorBuffer =
            accelerator.allocate::<[i32; 20]>(rigid_body_capacity);
        let rigid_predicted_motion: AcceleratorBuffer =
            accelerator.allocate::<[i32; 4]>(rigid_body_capacity);
        let rigid_damage: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_fractures: AcceleratorBuffer =
            accelerator.allocate::<u32>((buffered_cell_count as usize).div_ceil(32));
        let rigid_damage_dispatch = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid pressure damage indirect dispatch"),
            size: 12,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rigid_fracture_count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid pressure fracture count"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rigid_fracture_word_count = u64::from(buffered_cell_count).div_ceil(32);
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure parameters"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    Self::storage_layout_entry(27, false),
                    Self::storage_layout_entry(28, false),
                    Self::storage_layout_entry(29, true),
                    Self::storage_layout_entry(30, false),
                    Self::storage_layout_entry(31, false),
                    Self::storage_layout_entry(32, false),
                    Self::storage_layout_entry(33, true),
                    Self::storage_layout_entry(34, false),
                    Self::storage_layout_entry(36, false),
                    Self::storage_layout_entry(37, false),
                    Self::storage_layout_entry(38, false),
                ],
            });
        let indirect_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular pressure indirect bind group layout"),
                entries: &[Self::storage_layout_entry(0, false)],
            });
        let rigid_damage_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rigid pressure damage bind group layout"),
                entries: &[Self::storage_layout_entry(1, false)],
            });
        let bound_buffers: Vec<(u32, wgpu::Buffer)> = vec![
            (0, material_ids.wgpu_buffer().clone()),
            (1, appearances.wgpu_buffer().clone()),
            (2, integrities.wgpu_buffer().clone()),
            (3, kinematics.wgpu_buffer().clone()),
            (4, static_properties.wgpu_buffer().clone()),
            (5, dynamic_properties.wgpu_buffer().clone()),
            (6, pending_impulses.wgpu_buffer().clone()),
            (7, pressure_a.wgpu_buffer().clone()),
            (8, pressure_b.wgpu_buffer().clone()),
            (9, retained_pressure.wgpu_buffer().clone()),
            (10, external_body_occupancy.wgpu_buffer().clone()),
            (11, external_body_velocity.wgpu_buffer().clone()),
            (14, active_tiles.wgpu_buffer().clone()),
            (15, active_tile_indices.wgpu_buffer().clone()),
            (16, mechanical_fluid_cells.wgpu_buffer().clone()),
            (17, fluid_properties.wgpu_buffer().clone()),
            (18, gas_velocity.wgpu_buffer().clone()),
            (19, gas_concentrations.wgpu_buffer().clone()),
            (20, gas_properties.wgpu_buffer().clone()),
            (21, fluid_coverage.wgpu_buffer().clone()),
            (22, rigid_owners.wgpu_buffer().clone()),
            (23, rigid_material_identifiers.wgpu_buffer().clone()),
            (24, rigid_transforms.wgpu_buffer().clone()),
            (27, rigid_reactions.wgpu_buffer().clone()),
            (28, rigid_contact_statistics.wgpu_buffer().clone()),
            (29, rigid_cells.wgpu_buffer().clone()),
            (30, rigid_predicted_motion.wgpu_buffer().clone()),
            (31, rigid_integrities.wgpu_buffer().clone()),
            (32, rigid_damage.wgpu_buffer().clone()),
            (33, rigid_claims.wgpu_buffer().clone()),
            (34, rigid_fractures.wgpu_buffer().clone()),
            (36, rigid_fracture_count.clone()),
            (37, mutation_requests.wgpu_buffer().clone()),
            (38, mutation_request_count.wgpu_buffer().clone()),
        ];
        let bind_group: wgpu::BindGroup =
            Self::create_bind_group(device, &layout, &parameters, &bound_buffers);
        let indirect_bind_group: wgpu::BindGroup =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cellular pressure indirect bind group"),
                layout: &indirect_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: indirect_dispatch.as_entire_binding(),
                }],
            });
        let rigid_damage_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rigid pressure damage bind group"),
            layout: &rigid_damage_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 1,
                resource: rigid_damage_dispatch.as_entire_binding(),
            }],
        });
        let shader: wgpu::ShaderModule = super::create_simulation_shader_module(
            device,
            "cellular pressure shader",
            include_str!("cellular_pressure.wgsl"),
            "engine_physics/src/simulation/cellular_pressure.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let compact_pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cellular pressure compaction pipeline layout"),
                bind_group_layouts: &[Some(&layout), Some(&indirect_bind_group_layout)],
                immediate_size: 0,
            });
        let rigid_damage_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rigid pressure damage pipeline layout"),
                bind_group_layouts: &[Some(&layout), Some(&rigid_damage_bind_group_layout)],
                immediate_size: 0,
            });
        let reaction_readback_size: u64 =
            rigid_body_capacity as u64 * 128 + 4 + rigid_fracture_word_count * 4;
        let rigid_reaction_readback_slots: Vec<RigidGranularReadbackSlot> = (0
            ..RIGID_REACTION_READBACK_SLOT_COUNT)
            .map(|_| RigidGranularReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rigid granular reaction readback"),
                    size: reaction_readback_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                status: Arc::new(Mutex::new(RigidGranularReadbackStatus::Available)),
            })
            .collect();
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
            rigid_predicted_motion,
            rigid_damage,
            rigid_fractures,
            rigid_damage_dispatch,
            rigid_fracture_count,
            rigid_reaction_readback_slots,
            rigid_reaction_completed: BTreeMap::new(),
            rigid_reaction_sequence_next: 0,
            rigid_reaction_sequence_apply_next: 0,
            rigid_topology_revision: u64::MAX,
            rigid_fracture_word_count,
            parameters,
            bind_group,
            bind_group_layout: layout,
            bound_buffers,
            rigid_body_capacity,
            indirect_bind_group,
            rigid_damage_bind_group,
            impulse_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular impulse pipeline",
                "queue_cellular_radial_impulse",
            ),
            clear_active_tiles_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular pressure active tile clear pipeline",
                "clear_active_cellular_pressure_tiles",
            ),
            mark_active_tiles_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular pressure active tile marking pipeline",
                "mark_active_cellular_pressure_tiles",
            ),
            compact_active_tiles_pipeline: Self::create_pipeline(
                device,
                &compact_pipeline_layout,
                &shader,
                "cellular pressure active tile compaction pipeline",
                "compact_active_cellular_pressure_tiles",
            ),
            gather_rigid_contacts_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "gather rigid contacts",
                "gather_rigid_contacts",
            ),
            gather_rigid_static_contacts_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "gather rigid static contacts",
                "gather_rigid_static_contacts",
            ),
            resolve_rigid_static_contacts_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "resolve rigid static contacts",
                "resolve_rigid_static_contacts",
            ),
            resolve_contacts_pipelines: [
                "resolve_cellular_contacts_horizontal_even",
                "resolve_cellular_contacts_horizontal_odd",
                "resolve_cellular_contacts_vertical_even",
                "resolve_cellular_contacts_vertical_odd",
            ]
            .map(|entry| Self::create_pipeline(device, &pipeline_layout, &shader, entry, entry)),
            rigid_contact_initialize_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "initialize rigid contact state",
                "initialize_rigid_contact_state",
            ),
            propagate_pending_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular pending pressure propagation",
                "propagate_pending_cellular_pressure",
            ),
            propagate_a_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular pressure A propagation",
                "propagate_cellular_pressure_a",
            ),
            propagate_b_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "cellular pressure B propagation",
                "propagate_cellular_pressure_b",
            ),
            finalize_pipeline: Self::create_pipeline(
                device,
                &rigid_damage_pipeline_layout,
                &shader,
                "cellular pressure finalization",
                "finalize_cellular_pressure",
            ),
            apply_rigid_damage_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "apply rigid pressure damage",
                "apply_rigid_pressure_damage",
            ),
            mark_pressure_active_tiles_pipeline: Self::create_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "mark pressure active cellular pressure tiles",
                "mark_pressure_active_cellular_pressure_tiles",
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
        let min = CellCoordinates {
            x: (center.x as f32 - radius).floor() as i32,
            y: (center.y as f32 - radius).floor() as i32,
        };
        let max = CellCoordinates {
            x: (center.x as f32 + radius).ceil() as i32,
            y: (center.y as f32 + radius).ceil() as i32,
        };
        let min = CellCoordinates {
            x: min.x.max(origin.x * 8),
            y: min.y.max(origin.y * 8),
        };
        let max = CellCoordinates {
            x: max.x.min((origin.x + i32::from(width)) * 8 - 1),
            y: max.y.min((origin.y + i32::from(height)) * 8 - 1),
        };
        if max.x < min.x || max.y < min.y {
            return;
        }
        self.write_parameters(
            accelerator,
            origin,
            width,
            height,
            ring_x,
            ring_y,
            center,
            radius,
            strength,
            0.0,
            [0.0; 2],
            0,
            0,
            min,
            [
                u32::try_from(max.x - min.x + 1).unwrap(),
                u32::try_from(max.y - min.y + 1).unwrap(),
            ],
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("queue cellular radial impulse"),
                });
        let mut pass =
            accelerator.begin_compute_pass(&mut encoder, "queue cellular radial impulse");
        pass.set_pipeline(&self.impulse_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(
            (u32::try_from(max.x - min.x + 1).unwrap() * u32::try_from(max.y - min.y + 1).unwrap())
                .div_ceil(64),
            1,
            1,
        );
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
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
        gravity: [f32; 2],
        rigid_body_count: usize,
        rigid_cell_count: usize,
        rigid_topology_revision: u64,
    ) -> Result<(), io::Error> {
        self.ensure_rigid_body_capacity(accelerator, rigid_body_count);
        let rigid_body_count: u32 = rigid_body_count
            .try_into()
            .map_err(|_| io::Error::other("Rigid body count exceeds GPU indexing range"))?;
        let rigid_cell_count: u32 = rigid_cell_count
            .try_into()
            .map_err(|_| io::Error::other("Rigid cell count exceeds GPU indexing range"))?;
        self.write_parameters(
            accelerator,
            origin,
            width,
            height,
            ring_x,
            ring_y,
            CellCoordinates { x: 0, y: 0 },
            0.0,
            0.0,
            delta_time,
            gravity,
            rigid_body_count,
            rigid_cell_count,
            CellCoordinates { x: 0, y: 0 },
            [0; 2],
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular pressure simulation"),
                });
        if self.rigid_topology_revision != rigid_topology_revision {
            encoder.clear_buffer(self.rigid_reactions.wgpu_buffer(), 0, None);
            self.rigid_reaction_completed
                .retain(|_, batch| batch.topology_revision == rigid_topology_revision);
            self.rigid_reaction_sequence_apply_next = self.rigid_reaction_sequence_next;
            self.rigid_topology_revision = rigid_topology_revision;
        }
        encoder.clear_buffer(self.rigid_fractures.wgpu_buffer(), 0, None);
        encoder.clear_buffer(&self.rigid_fracture_count, 0, None);
        encoder.clear_buffer(&self.rigid_damage_dispatch, 0, None);
        let tile_count: u32 = self.buffered_cell_count / 64;
        for (pipeline, label) in [
            (
                &self.clear_active_tiles_pipeline,
                "clear active cellular pressure tiles",
            ),
            (
                &self.mark_active_tiles_pipeline,
                "mark active cellular pressure tiles",
            ),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            let workgroups: u32 = if label == "mark active cellular pressure tiles" {
                tile_count
            } else {
                tile_count.div_ceil(64)
            };
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass: wgpu::ComputePass<'_> = accelerator
                .begin_compute_pass(&mut encoder, "compact active cellular pressure tiles");
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "initialize rigid contact state");
            pass.set_pipeline(&self.rigid_contact_initialize_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_body_count.max(1).div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (
                &self.gather_rigid_static_contacts_pipeline,
                "gather rigid static contacts",
            ),
            (
                &self.resolve_rigid_static_contacts_pipeline,
                "resolve rigid static contacts",
            ),
        ] {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_cell_count.max(1).div_ceil(64), 1, 1);
        }
        {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "gather rigid grid interfaces");
            pass.set_pipeline(&self.gather_rigid_contacts_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        for pipeline in &self.resolve_contacts_pipelines {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "resolve colored cellular faces");
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        // Contact discovery is intentionally broad.  Rebuild the same compact list from
        // actual pressure sources before running the expensive pressure stencil.
        {
            let mut pass = accelerator.begin_compute_pass(
                &mut encoder,
                "clear pressure active cellular pressure tiles",
            );
            pass.set_pipeline(&self.clear_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass = accelerator
                .begin_compute_pass(&mut encoder, "mark pressure active cellular pressure tiles");
            pass.set_pipeline(&self.mark_pressure_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count, 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass = accelerator.begin_compute_pass(
                &mut encoder,
                "compact pressure active cellular pressure tiles",
            );
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (
                &self.propagate_pending_pipeline,
                "propagate pending cellular pressure",
            ),
            (
                &self.propagate_b_pipeline,
                "propagate cellular pressure B 1",
            ),
            (
                &self.propagate_a_pipeline,
                "propagate cellular pressure A 1",
            ),
            (
                &self.propagate_b_pipeline,
                "propagate cellular pressure B 2",
            ),
            (
                &self.propagate_a_pipeline,
                "propagate cellular pressure A 2",
            ),
            (&self.finalize_pipeline, "finalize cellular pressure"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            if label == "finalize cellular pressure" {
                pass.set_bind_group(1, &self.rigid_damage_bind_group, &[]);
            }
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "apply rigid pressure damage");
            pass.set_pipeline(&self.apply_rigid_damage_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.rigid_damage_dispatch, 0);
        }
        let readback_slot_index: usize = if rigid_body_count == 0 {
            0
        } else if let Some(index) = self.rigid_reaction_readback_slots.iter().position(|slot| {
            slot.status
                .lock()
                .is_ok_and(|status| matches!(*status, RigidGranularReadbackStatus::Available))
        }) {
            index
        } else {
            tracing::warn!("rigid reaction readback pool saturated; skipping this batch");
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.tick = self.tick.wrapping_add(1);
            return Ok(());
        };
        let mut mapping = None;
        if rigid_body_count != 0 {
            {
                let slot = &self.rigid_reaction_readback_slots[readback_slot_index];
                let mut status = slot.status.lock().map_err(|_| {
                    io::Error::other("Rigid granular readback state is unavailable")
                })?;
                if matches!(*status, RigidGranularReadbackStatus::Available) {
                    *status = RigidGranularReadbackStatus::Mapping;
                    let sequence: u64 = self.rigid_reaction_sequence_next;
                    self.rigid_reaction_sequence_next =
                        self.rigid_reaction_sequence_next.wrapping_add(1);
                    let reaction_size: u64 = u64::from(rigid_body_count) * 80;
                    let statistics_size: u64 = u64::from(rigid_body_count) * 48;
                    let statistics_offset: u64 = reaction_size;
                    let fracture_count_offset: u64 = statistics_offset + statistics_size;
                    let fractures_offset: u64 = fracture_count_offset + 4;
                    encoder.copy_buffer_to_buffer(
                        self.rigid_reactions.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        0,
                        reaction_size,
                    );
                    encoder.copy_buffer_to_buffer(
                        self.rigid_contact_statistics.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        statistics_offset,
                        statistics_size,
                    );
                    encoder.clear_buffer(
                        self.rigid_reactions.wgpu_buffer(),
                        0,
                        Some(reaction_size),
                    );
                    encoder.copy_buffer_to_buffer(
                        &self.rigid_fracture_count,
                        0,
                        &slot.buffer,
                        fracture_count_offset,
                        4,
                    );
                    encoder.copy_buffer_to_buffer(
                        self.rigid_fractures.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        fractures_offset,
                        self.rigid_fracture_word_count * 4,
                    );
                    mapping = Some((
                        slot,
                        sequence,
                        fracture_count_offset,
                        fractures_offset,
                        fractures_offset + self.rigid_fracture_word_count * 4,
                    ));
                }
            }
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        if let Some((slot, sequence, fracture_count_offset, fractures_offset, mapped_size)) =
            mapping
        {
            let mapped_buffer: wgpu::Buffer = slot.buffer.clone();
            let callback_status = slot.status.clone();
            let body_count: usize = rigid_body_count as usize;
            mapped_buffer.clone().slice(0..mapped_size).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let result: Result<RigidGranularReactionBatch, String> = match result {
                        Ok(()) => mapped_buffer
                            .slice(0..mapped_size)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let mut reactions = Vec::with_capacity(body_count);
                                let mut constraints = Vec::with_capacity(body_count);
                                let mut supports = Vec::with_capacity(body_count);
                                let mut recovery = Vec::with_capacity(body_count);
                                let mut source_motion = Vec::with_capacity(body_count);
                                for bytes in mapped[..body_count * 80].chunks_exact(80) {
                                    let state = |offset: usize| {
                                        [
                                            i32::from_le_bytes(
                                                bytes[offset..offset + 4].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            i32::from_le_bytes(
                                                bytes[offset + 4..offset + 8].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            i32::from_le_bytes(
                                                bytes[offset + 8..offset + 12].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            u32::from_le_bytes(
                                                bytes[offset + 12..offset + 16].try_into().unwrap(),
                                            ) as f32
                                                / 256.0,
                                        ]
                                    };
                                    constraints.push(state(16));
                                    supports.push(state(32));
                                    recovery.push(state(48));
                                    source_motion.push([
                                        f32::from_le_bytes(bytes[64..68].try_into().unwrap()),
                                        f32::from_le_bytes(bytes[68..72].try_into().unwrap()),
                                        f32::from_le_bytes(bytes[72..76].try_into().unwrap()),
                                        u32::from_le_bytes(bytes[76..80].try_into().unwrap())
                                            as f32,
                                    ]);
                                    let overflow =
                                        i32::from_le_bytes(bytes[12..16].try_into().unwrap());
                                    if overflow != 0 {
                                        drop(mapped);
                                        mapped_buffer.unmap();
                                        return Err(
                                            "Rigid granular reaction accumulator overflowed"
                                                .to_owned(),
                                        );
                                    }
                                    reactions.push([
                                        i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as f32
                                            / 256.0,
                                        i32::from_le_bytes(bytes[4..8].try_into().unwrap()) as f32
                                            / 256.0,
                                        i32::from_le_bytes(bytes[8..12].try_into().unwrap()) as f32
                                            / 64.0,
                                    ]);
                                }
                                let statistics_start: usize = body_count * 80;
                                let contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[0..4].try_into().unwrap())
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let static_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[4..8].try_into().unwrap())
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let granular_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[12..16].try_into().unwrap())
                                            & 0xffff
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let moving_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[12..16].try_into().unwrap()) >> 16
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let energy_budgets = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as f32
                                            / 256.0
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let fracture_count = u32::from_le_bytes(
                                    mapped[fracture_count_offset as usize
                                        ..fracture_count_offset as usize + 4]
                                        .try_into()
                                        .unwrap(),
                                );
                                let fractured_slots = if fracture_count == 0 {
                                    Vec::new()
                                } else {
                                    mapped[fractures_offset as usize..mapped_size as usize]
                                        .chunks_exact(4)
                                        .enumerate()
                                        .flat_map(|(word, bytes)| {
                                            let bits =
                                                u32::from_le_bytes(bytes.try_into().unwrap());
                                            (0..32).filter_map(move |bit| {
                                                ((bits & (1 << bit)) != 0)
                                                    .then_some((word as u32) * 32 + bit)
                                            })
                                        })
                                        .collect()
                                }
                                .into_boxed_slice();
                                drop(mapped);
                                mapped_buffer.unmap();
                                Ok(RigidGranularReactionBatch {
                                    sequence,
                                    topology_revision: rigid_topology_revision,
                                    body_count,
                                    reactions: reactions.into_boxed_slice(),
                                    contact_counts,
                                    static_contact_counts,
                                    granular_contact_counts,
                                    moving_contact_counts,
                                    energy_budgets,
                                    constraints: constraints.into_boxed_slice(),
                                    supports: supports.into_boxed_slice(),
                                    recovery: recovery.into_boxed_slice(),
                                    source_motion: source_motion.into_boxed_slice(),
                                    fractured_slots,
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
            let mut status = slot
                .status
                .lock()
                .map_err(|_| io::Error::other("Rigid granular readback state is unavailable"))?;
            if matches!(*status, RigidGranularReadbackStatus::Complete(_)) {
                let RigidGranularReadbackStatus::Complete(result) =
                    std::mem::replace(&mut *status, RigidGranularReadbackStatus::Available)
                else {
                    unreachable!()
                };
                let batch = result.map_err(io::Error::other)?;
                self.rigid_reaction_completed.insert(batch.sequence, batch);
            }
        }
        self.rigid_reaction_completed
            .retain(|_, batch| batch.topology_revision == self.rigid_topology_revision);
        let mut ordered = Vec::new();
        while let Some(batch) = self
            .rigid_reaction_completed
            .remove(&self.rigid_reaction_sequence_apply_next)
        {
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
            accelerator
                .wgpu_queue()
                .write_buffer(buffer.wgpu_buffer(), offset, &zeroes);
        }
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
        gravity: [f32; 2],
        rigid_body_count: u32,
        rigid_cell_count: u32,
        impulse_min: CellCoordinates,
        impulse_size: [u32; 2],
    ) {
        let values: [u32; 24] = [
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
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            rigid_cell_count,
            0,
            impulse_min.x as u32,
            impulse_min.y as u32,
            impulse_size[0],
            impulse_size[1],
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
    }

    fn ensure_rigid_body_capacity(&mut self, accelerator: &Accelerator, count: usize) {
        if count <= self.rigid_body_capacity {
            return;
        }
        self.rigid_contact_statistics.free();
        self.rigid_reactions.free();
        self.rigid_predicted_motion.free();
        self.rigid_body_capacity = count.next_power_of_two();
        self.rigid_contact_statistics = accelerator.allocate::<[u32; 12]>(self.rigid_body_capacity);
        self.rigid_reactions = accelerator.allocate::<[i32; 20]>(self.rigid_body_capacity);
        self.rigid_predicted_motion = accelerator.allocate::<[i32; 4]>(self.rigid_body_capacity);
        for (binding, buffer) in &mut self.bound_buffers {
            if *binding == 27 {
                *buffer = self.rigid_reactions.wgpu_buffer().clone();
            }
            if *binding == 30 {
                *buffer = self.rigid_predicted_motion.wgpu_buffer().clone();
            }
            if *binding == 28 {
                *buffer = self.rigid_contact_statistics.wgpu_buffer().clone();
            }
        }
        self.bind_group = Self::create_bind_group(
            accelerator.wgpu_device(),
            &self.bind_group_layout,
            &self.parameters,
            &self.bound_buffers,
        );
        let size: u64 =
            self.rigid_body_capacity as u64 * 128 + 4 + self.rigid_fracture_word_count * 4;
        self.rigid_reaction_readback_slots = (0..RIGID_REACTION_READBACK_SLOT_COUNT)
            .map(|_| RigidGranularReadbackSlot {
                buffer: accelerator
                    .wgpu_device()
                    .create_buffer(&wgpu::BufferDescriptor {
                        label: Some("rigid granular reaction readback"),
                        size,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                status: Arc::new(Mutex::new(RigidGranularReadbackStatus::Available)),
            })
            .collect();
        self.rigid_reaction_completed.clear();
        self.rigid_reaction_sequence_apply_next = self.rigid_reaction_sequence_next;
        self.rigid_topology_revision = u64::MAX;
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        parameters: &wgpu::Buffer,
        buffers: &[(u32, wgpu::Buffer)],
    ) -> wgpu::BindGroup {
        let mut entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 13,
            resource: parameters.as_entire_binding(),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular pressure bind group"),
            layout,
            entries: &entries,
        })
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
        self.rigid_predicted_motion.free();
        self.rigid_damage.free();
        self.rigid_fractures.free();
        self.indirect_dispatch.destroy();
        self.rigid_damage_dispatch.destroy();
        self.rigid_fracture_count.destroy();
        self.parameters.destroy();
        for slot in &self.rigid_reaction_readback_slots {
            slot.buffer.destroy();
        }
    }
}

#[cfg(test)]
#[path = "rigid_contact_tests.rs"]
mod rigid_contact_tests;

#[cfg(test)]
mod tests {

    use super::*;
    use engine_graphics::{Color, MaterialAppearance};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    #[test]
    fn colored_face_pipelines_compile_on_gpu() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
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
        let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
        let pressure = CellularPressure::new(
            &accelerator,
            &materials,
            &cells,
            &appearances,
            &integrities,
            &accelerator.allocate::<f32>(64),
            &kinematics,
            &occupancy,
            &velocity,
            &owners,
            &accelerator.allocate::<u32>(64),
            &rigid_materials,
            &transforms,
            &rigid_cells,
            &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<[f32; 2]>(64),
            &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[f32; 4]>(2),
            &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<u32>(1),
            0,
            64,
        );
        accelerator.poll().unwrap();
        drop(pressure);
    }

    #[test]
    fn falling_cell_transfers_momentum_into_anchored_cell_without_pressure_kick() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
            mass: 1.0,
            pressure_ignore_threshold: 1000.0,
            default_integrity: 100.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(200, 180, 130)),
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
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
        let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
        let fluid = accelerator.allocate::<[u32; 4]>(64);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let gas_properties = accelerator.allocate::<[f32; 4]>(2);
        let fluid_coverage = accelerator.allocate::<f32>(64);
        let mut pressure = CellularPressure::new(
            &accelerator,
            &materials,
            &cells,
            &appearances,
            &integrities,
            &accelerator.allocate::<f32>(64),
            &kinematics,
            &occupancy,
            &external_velocity,
            &owners,
            &accelerator.allocate::<u32>(64),
            &rigid_materials,
            &transforms,
            &rigid_cells,
            &fluid,
            &gas_velocity,
            &gas_concentrations,
            &gas_properties,
            &fluid_coverage,
            &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<u32>(1),
            0,
            64,
        );
        accelerator.wgpu_queue().write_buffer(
            cells.wgpu_buffer(),
            0,
            &stone.as_u32().to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            cells.wgpu_buffer(),
            8 * 4,
            &sand.as_u32().to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            kinematics.wgpu_buffer(),
            8 * 16 + 4,
            &(-1.0f32).to_le_bytes(),
        );
        pressure
            .simulate(
                &accelerator,
                TileCoordinates { x: 0, y: 0 },
                1,
                1,
                0,
                0,
                1.0 / 60.0,
                [0.0; 2],
                0,
                0,
                0,
            )
            .unwrap();
        let readback = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("cellular face check"),
                size: 64 * 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular face check"),
                });
        encoder.copy_buffer_to_buffer(kinematics.wgpu_buffer(), 0, &readback, 0, 64 * 16);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let started = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if receiver.try_recv().is_ok() {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let velocity_y = f32::from_le_bytes(mapped[8 * 16 + 4..8 * 16 + 8].try_into().unwrap());
        assert!(
            velocity_y.abs() < 0.01,
            "unsupported pressure kick: {velocity_y}"
        );
    }

    #[test]
    fn rigid_static_overlap_uses_one_coherent_reaction_per_tick_under_readback_backlog() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
            mass: 1.0,
            pressure_ignore_threshold: 1000.0,
            default_integrity: 100.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
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
        let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
        let fluid = accelerator.allocate::<[u32; 4]>(64);
        let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
        let gas_concentrations = accelerator.allocate::<f32>(64);
        let gas_properties = accelerator.allocate::<[f32; 4]>(2);
        let fluid_coverage = accelerator.allocate::<f32>(64);
        let mut pressure = CellularPressure::new(
            &accelerator,
            &materials,
            &cells,
            &appearances,
            &integrities,
            &accelerator.allocate::<f32>(64),
            &kinematics,
            &occupancy,
            &external_velocity,
            &owners,
            &accelerator.allocate::<u32>(64),
            &rigid_materials,
            &transforms,
            &rigid_cells,
            &fluid,
            &gas_velocity,
            &gas_concentrations,
            &gas_properties,
            &fluid_coverage,
            &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<u32>(1),
            0,
            64,
        );
        accelerator.wgpu_queue().write_buffer(
            cells.wgpu_buffer(),
            0,
            &stone.as_u32().to_le_bytes(),
        );
        let rigid_cell: [u32; 8] = [0, 0, 0, stone.as_u32(), 0, 0, 0, 0];
        accelerator.wgpu_queue().write_buffer(
            rigid_cells.wgpu_buffer(),
            0,
            &rigid_cell
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        // The rigid cell overlaps the top of the static cell and is moving into it.
        let transform: [f32; 12] = [
            0.0, 0.08, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0625, 0.1425, 1.0, 0.0,
        ];
        accelerator.wgpu_queue().write_buffer(
            transforms.wgpu_buffer(),
            0,
            &transform
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );

        // Do not poll while submitting: this deliberately exercises the bounded
        // readback pool without permitting an unbounded staging allocation.
        for _ in 0..5 {
            pressure
                .simulate(
                    &accelerator,
                    TileCoordinates { x: 0, y: 0 },
                    1,
                    1,
                    0,
                    0,
                    1.0 / 60.0,
                    [0.0, -9.8],
                    1,
                    1,
                    1,
                )
                .unwrap();
        }
        assert_eq!(
            pressure.rigid_reaction_readback_slots.len(),
            RIGID_REACTION_READBACK_SLOT_COUNT
        );
        let started = Instant::now();
        let mut batches = Vec::new();
        while batches.len() < RIGID_REACTION_READBACK_SLOT_COUNT {
            accelerator.poll().unwrap();
            batches.extend(pressure.collect_rigid_reactions().unwrap());
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        assert_eq!(batches.len(), RIGID_REACTION_READBACK_SLOT_COUNT);
        for batch in batches {
            assert_eq!(batch.body_count, 1);
            assert!(batch.static_contact_counts[0] > 0);
            assert_eq!(batch.granular_contact_counts[0], 0);
            assert!(batch.constraints[0][1].is_finite() && batch.constraints[0][1] > 0.1);
            assert!(
                batch.constraints[0][1] < 5.0,
                "explosive reaction: {:?}",
                batch.constraints[0]
            );
        }
    }
}
