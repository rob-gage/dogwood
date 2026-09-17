// Copyright Rob Gage 2026

use super::*;

impl CellularPhysicsBodyProxy {
    pub fn new(accelerator: &Accelerator, buffered_cell_count: usize) -> Self {
        let device = accelerator.wgpu_device();
        let buffered_cell_count = buffered_cell_count as u32;
        let occupancy = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let velocity = accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let actor_capacity = 1;
        let actor_counts = accelerator.allocate::<u32>(actor_capacity);
        let actor_claims = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let actor_proxies = accelerator.allocate::<[u32; 12]>(actor_capacity);
        let rigid_material_identifiers = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_appearances = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_claims = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_owners = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_cells = accelerator.allocate::<[u32; 8]>(buffered_cell_count as usize);
        let rigid_transforms = accelerator.allocate::<[f32; 12]>(buffered_cell_count as usize);
        let destroy_requests = accelerator.allocate::<u32>(buffered_cell_count as usize + 1);
        let destroy_results = accelerator.allocate::<[u32; 2]>(buffered_cell_count as usize);
        let destroy_completed = Arc::new(Mutex::new(Vec::new()));
        let parameters = crate::simulation::create_simulation_uniform_buffer(
            device,
            "cellular physics body proxy parameters",
            96,
        );
        let storage = crate::simulation::storage_bind_group_layout_entry;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cellular physics body proxy bind group layout"),
            entries: &[
                storage(0, false),
                storage(1, false),
                storage(2, false),
                crate::simulation::uniform_bind_group_layout_entry(3),
                storage(4, false),
                storage(5, false),
                storage(6, false),
                storage(7, true),
                storage(8, true),
                storage(9, false),
                storage(10, true),
                storage(11, false),
                storage(12, true),
                storage(13, false),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular physics body proxy bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: occupancy.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: velocity.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: actor_counts.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: parameters.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: rigid_material_identifiers.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: rigid_appearances.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: rigid_claims.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: rigid_cells.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: rigid_transforms.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: rigid_owners.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: actor_proxies.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: actor_claims.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: destroy_requests.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: destroy_results.wgpu_buffer().as_entire_binding(),
                },
            ],
        });
        let shader = crate::simulation::create_simulation_shader_module(
            device,
            "cellular physics body proxy shader",
            include_str!("cellular_physics_body_proxy.wgsl"),
            "engine_physics/src/simulation/cellular_physics_body_proxy.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cellular physics body proxy pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point, label| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            occupancy,
            velocity,
            actor_counts,
            actor_claims,
            actor_proxies,
            rigid_material_identifiers,
            rigid_appearances,
            rigid_claims,
            rigid_owners,
            rigid_cells,
            rigid_transforms,
            destroy_requests,
            destroy_results,
            destroy_completed,
            parameters,
            bind_group,
            bind_group_layout: layout,
            clear_pipeline: pipeline(
                "clear_cellular_physics_body_proxy",
                "cellular physics body proxy clear pipeline",
            ),
            actor_count_clear_pipeline: pipeline(
                "clear_actor_counts",
                "actor proxy count clear pipeline",
            ),
            actor_claim_pipeline: pipeline("claim_actor_proxy", "actor proxy claim pipeline"),
            actor_count_pipeline: pipeline("count_actor_proxy", "actor proxy count pipeline"),
            actor_resolve_pipeline: pipeline("resolve_actor_proxy", "actor proxy resolve pipeline"),
            rigid_claim_pipeline: pipeline(
                "claim_rigid_cell_proxy",
                "rigid cellular proxy claim pipeline",
            ),
            rigid_resolve_pipeline: pipeline(
                "resolve_rigid_cell_proxy",
                "rigid cellular proxy resolve pipeline",
            ),
            destroy_resolve_pipeline: pipeline(
                "resolve_rigid_destruction",
                "resolve rigid destruction",
            ),
            buffered_cell_count,
            topology_revision: u64::MAX,
            actor_capacity,
        }
    }
}
