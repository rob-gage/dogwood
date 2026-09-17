// Copyright Rob Gage 2026

use super::*;

impl CellularPressure {
    /// Creates pressure fields, material tables, and compute pipelines for a scene
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
            .expect("Cellular pressure buffer exceeds Accelerator indexing range");
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
        let parameters = crate::simulation::create_simulation_uniform_buffer(
            device,
            "cellular pressure parameters",
            96,
        );
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
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "cellular pressure shader",
            concat!(
                include_str!("cellular_pressure_shader_header.wgsl"),
                include_str!("cellular_pressure_shader_damage.wgsl"),
                include_str!("cellular_pressure_shader_activation.wgsl"),
                include_str!("cellular_pressure_shader_contact_dispatch.wgsl"),
                include_str!("cellular_pressure_shader_contact_resolution.wgsl"),
                include_str!("cellular_pressure_shader_rigid_cellular_contacts.wgsl"),
                include_str!("cellular_pressure_shader_propagation.wgsl"),
                include_str!("cellular_pressure_shader_rigid_static_contacts.wgsl"),
                include_str!("cellular_pressure_shader_coordinates.wgsl"),
            ),
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
}
