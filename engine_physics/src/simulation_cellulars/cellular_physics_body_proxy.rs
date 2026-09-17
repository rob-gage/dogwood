// Copyright Rob Gage 2026

use crate::actors::ActorCellularProxyState;
use crate::simulation::{RigidCellularBody, RigidCellularBodyState};
use crate::tiles::TileCoordinates;
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::sync::{Arc, Mutex};

/// Rasterizes physical bodies into transient cellular interaction geometry
pub struct CellularPhysicsBodyProxy {
    occupancy: AcceleratorBuffer,
    velocity: AcceleratorBuffer,
    actor_counts: AcceleratorBuffer,
    actor_claims: AcceleratorBuffer,
    actor_proxies: AcceleratorBuffer,
    rigid_material_identifiers: AcceleratorBuffer,
    rigid_appearances: AcceleratorBuffer,
    rigid_claims: AcceleratorBuffer,
    rigid_owners: AcceleratorBuffer,
    rigid_cells: AcceleratorBuffer,
    rigid_transforms: AcceleratorBuffer,
    destroy_requests: AcceleratorBuffer,
    destroy_results: AcceleratorBuffer,
    destroy_completed: Arc<Mutex<Vec<[u32; 2]>>>,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    clear_pipeline: wgpu::ComputePipeline,
    actor_count_clear_pipeline: wgpu::ComputePipeline,
    actor_claim_pipeline: wgpu::ComputePipeline,
    actor_count_pipeline: wgpu::ComputePipeline,
    actor_resolve_pipeline: wgpu::ComputePipeline,
    rigid_claim_pipeline: wgpu::ComputePipeline,
    rigid_resolve_pipeline: wgpu::ComputePipeline,
    destroy_resolve_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    topology_revision: u64,
    actor_capacity: usize,
}

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
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular physics body proxy parameters"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cellular physics body proxy bind group layout"),
            entries: &[
                storage(0, false),
                storage(1, false),
                storage(2, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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

    pub const fn occupancy_buffer(&self) -> &AcceleratorBuffer {
        &self.occupancy
    }
    pub const fn velocity_buffer(&self) -> &AcceleratorBuffer {
        &self.velocity
    }
    pub const fn rigid_material_identifiers_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_material_identifiers
    }
    pub const fn rigid_appearances_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_appearances
    }
    pub(crate) const fn rigid_owners_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_owners
    }
    /// The winning raster source for each world cell; this is the authoritative
    /// bridge from a transient raster cell back to a rigid cell descriptor.
    pub(crate) const fn rigid_claims_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_claims
    }
    pub(crate) const fn rigid_cells_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cells
    }
    pub(crate) const fn rigid_transforms_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_transforms
    }

    pub(crate) fn rigid_cell_count(&self, bodies: &[RigidCellularBody]) -> usize {
        bodies
            .iter()
            .map(|body| body.cells.len())
            .sum::<usize>()
            .min(self.buffered_cell_count as usize)
    }

    pub(crate) const fn rigid_cell_capacity(&self) -> usize {
        self.buffered_cell_count as usize
    }

    /// Resolves world physical cells through the current raster without a CPU body scan.
    pub(crate) fn resolve_destruction_requests(
        &mut self,
        accelerator: &Accelerator,
        indices: &[usize],
    ) {
        if indices.is_empty() {
            return;
        }
        let count = indices.len().min(self.buffered_cell_count as usize);
        let mut requests = Vec::with_capacity(count + 1);
        requests.push(count as u32);
        requests.extend(indices[..count].iter().map(|&index| index as u32));
        accelerator.wgpu_queue().write_buffer(
            self.destroy_requests.wgpu_buffer(),
            0,
            &requests
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let readback = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("rigid destruction readback"),
                size: (count * 8) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("resolve rigid destruction"),
                });
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "resolve rigid destruction");
            pass.set_pipeline(&self.destroy_resolve_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups((count as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            self.destroy_results.wgpu_buffer(),
            0,
            &readback,
            0,
            (count * 8) as u64,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let completed = self.destroy_completed.clone();
        readback
            .clone()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    let mapped = readback.slice(..).get_mapped_range();
                    if let Ok(mapped) = mapped {
                        if let Ok(mut completed) = completed.lock() {
                            completed.extend(mapped.chunks_exact(8).map(|bytes| {
                                [
                                    u32::from_le_bytes(bytes[..4].try_into().unwrap()),
                                    u32::from_le_bytes(bytes[4..].try_into().unwrap()),
                                ]
                            }));
                        }
                    }
                    readback.unmap();
                }
            });
    }

    pub(crate) fn take_destroyed_handles(&self) -> Vec<[u32; 2]> {
        self.destroy_completed
            .lock()
            .map(|mut handles| std::mem::take(&mut *handles))
            .unwrap_or_default()
    }

    pub(crate) fn rasterize(
        &mut self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        actors: &[ActorCellularProxyState],
        bodies: &[RigidCellularBody],
        body_states: &[RigidCellularBodyState],
        topology_revision: u64,
    ) {
        self.ensure_actor_capacity(accelerator, actors.len());
        let rigid_cell_count: u32 = bodies
            .iter()
            .map(|body| body.cells.len() as u32)
            .sum::<u32>()
            .min(self.buffered_cell_count);
        if self.topology_revision != topology_revision {
            let cells: Vec<u8> = bodies
                .iter()
                .enumerate()
                .flat_map(|(body, rigid_body)| {
                    rigid_body.cells.iter().map(move |cell| {
                        [
                            cell.local[0] as u32,
                            cell.local[1] as u32,
                            body as u32,
                            cell.material.as_u32(),
                            cell.appearance.0,
                            cell.state_slot,
                            cell.state_generation,
                            0,
                        ]
                    })
                })
                .take(self.buffered_cell_count as usize)
                .flatten()
                .flat_map(u32::to_le_bytes)
                .collect();
            accelerator
                .wgpu_queue()
                .write_buffer(self.rigid_cells.wgpu_buffer(), 0, &cells);
            self.topology_revision = topology_revision;
        }
        let transforms: Vec<u8> = body_states
            .iter()
            .flat_map(|state| {
                [
                    state.translation[0],
                    state.translation[1],
                    state.angle.cos(),
                    state.angle.sin(),
                    state.linear_velocity[0],
                    state.linear_velocity[1],
                    state.angular_velocity,
                    0.0,
                    state.center_of_mass[0],
                    state.center_of_mass[1],
                    state.inverse_mass,
                    state.inverse_angular_inertia,
                ]
            })
            .flat_map(f32::to_le_bytes)
            .collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.rigid_transforms.wgpu_buffer(), 0, &transforms);
        let actor_bytes: Vec<u8> = actors
            .iter()
            .flat_map(|actor| {
                let (kind, shape) = actor.shape.gpu_parameters();
                [
                    actor.center[0].to_bits(),
                    actor.center[1].to_bits(),
                    actor.velocity[0].to_bits(),
                    actor.velocity[1].to_bits(),
                    actor.drive[0].to_bits(),
                    actor.drive[1].to_bits(),
                    shape[0].to_bits(),
                    shape[1].to_bits(),
                    kind,
                    actor.occupancy_kind,
                    actor.mass.to_bits(),
                    0,
                ]
            })
            .flat_map(u32::to_le_bytes)
            .collect();
        if !actor_bytes.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                self.actor_proxies.wgpu_buffer(),
                0,
                &actor_bytes,
            );
        }
        let values = [
            origin.x as u32,
            origin.y as u32,
            u32::from(width),
            u32::from(height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            self.buffered_cell_count,
            actors.len() as u32,
            rigid_cell_count,
            bodies.len() as u32,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular physics body proxy rasterization"),
                });
        for (pipeline, label, count, actor_dispatch) in [
            (
                &self.clear_pipeline,
                "clear cellular physics body proxy",
                self.buffered_cell_count,
                false,
            ),
            (
                &self.actor_count_clear_pipeline,
                "clear actor proxy counts",
                actors.len() as u32,
                false,
            ),
            (
                &self.actor_claim_pipeline,
                "claim actor proxies",
                actors.len() as u32,
                true,
            ),
            (
                &self.actor_count_pipeline,
                "count actor proxies",
                actors.len() as u32,
                true,
            ),
            (
                &self.actor_resolve_pipeline,
                "resolve actor proxies",
                actors.len() as u32,
                true,
            ),
            (
                &self.rigid_claim_pipeline,
                "claim rigid cellular proxy",
                rigid_cell_count,
                false,
            ),
            (
                &self.rigid_resolve_pipeline,
                "resolve rigid cellular proxy",
                rigid_cell_count,
                false,
            ),
        ] {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                if actor_dispatch {
                    count
                } else {
                    count.div_ceil(64)
                },
                1,
                1,
            );
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn ensure_actor_capacity(&mut self, accelerator: &Accelerator, count: usize) {
        if count <= self.actor_capacity {
            return;
        }
        self.actor_counts.free();
        self.actor_proxies.free();
        self.actor_capacity = count.next_power_of_two();
        self.actor_counts = accelerator.allocate::<u32>(self.actor_capacity);
        self.actor_proxies = accelerator.allocate::<[u32; 12]>(self.actor_capacity);
        self.bind_group = accelerator
            .wgpu_device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cellular physics body proxy bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.occupancy.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.velocity.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.actor_counts.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.parameters.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self
                            .rigid_material_identifiers
                            .wgpu_buffer()
                            .as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.rigid_appearances.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: self.rigid_claims.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: self.rigid_cells.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: self.rigid_transforms.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: self.rigid_owners.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: self.actor_proxies.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: self.actor_claims.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: self.destroy_requests.wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: self.destroy_results.wgpu_buffer().as_entire_binding(),
                    },
                ],
            });
    }
}

impl Drop for CellularPhysicsBodyProxy {
    fn drop(&mut self) {
        self.occupancy.free();
        self.velocity.free();
        self.actor_counts.free();
        self.actor_claims.free();
        self.actor_proxies.free();
        self.rigid_material_identifiers.free();
        self.rigid_appearances.free();
        self.rigid_claims.free();
        self.rigid_owners.free();
        self.rigid_cells.free();
        self.rigid_transforms.free();
        self.destroy_requests.free();
        self.destroy_results.free();
        self.parameters.destroy();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        actors::{ActorCellularProxyState, ActorCollisionShape},
        materials::{MaterialForm, MaterialIdentifier},
        tiles::CellularAppearance,
    };
    use rapier2d::prelude::RigidBodyHandle;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    #[test]
    fn axis_aligned_rigid_block_raster_has_every_cell_once() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Accelerator::new().unwrap();
        let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 3);
        let body = RigidCellularBody {
            id: 0,
            handle: RigidBodyHandle::invalid(),
            cells: (2..6)
                .flat_map(|y| {
                    (2..6).map(move |x| {
                        crate::simulation::RigidCellularBodyCell::test_cell(
                            [x, y],
                            material,
                            CellularAppearance::NEUTRAL,
                        )
                    })
                })
                .collect(),
        };
        assert!(std::mem::size_of::<[u32; 8]>() == 32);
        println!("rigid bodies: 1, rigid cells: {}", body.cells.len());
        let buffered_cell_count: usize = 72 * 51 * 64;
        let mut proxy = CellularPhysicsBodyProxy::new(&accelerator, buffered_cell_count);
        let bodies = [body];
        let body_states = [RigidCellularBodyState {
            translation: [0.0; 2],
            angle: 0.0,
            linear_velocity: [0.0; 2],
            angular_velocity: 0.0,
            sleeping: false,
            center_of_mass: [0.5; 2],
            inverse_mass: 1.0,
            inverse_angular_inertia: 1.0,
        }];
        let actors = [
            ActorCellularProxyState {
                center: [-1.0, 0.0],
                velocity: [0.0; 2],
                drive: [1.0, 0.0],
                shape: ActorCollisionShape::Circle { radius: 0.375 },
                occupancy_kind: 1,
                mass: 1.0,
            },
            ActorCellularProxyState {
                center: [-1.0, 0.0],
                velocity: [0.0; 2],
                drive: [0.0; 2],
                shape: ActorCollisionShape::Capsule {
                    radius: 0.25,
                    height: 0.75,
                },
                occupancy_kind: 2,
                mass: 1.0,
            },
            ActorCellularProxyState {
                center: [1.5, 0.0],
                velocity: [0.0; 2],
                drive: [0.0; 2],
                shape: ActorCollisionShape::Rectangle {
                    width: 0.5,
                    height: 0.75,
                },
                occupancy_kind: 1,
                mass: 1.0,
            },
        ];
        proxy.rasterize(
            &accelerator,
            TileCoordinates { x: -12, y: -12 },
            72,
            51,
            0,
            0,
            [0.0, -1.0],
            &actors,
            &bodies,
            &body_states,
            0,
        );
        let readback = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("rigid material raster test readback"),
                size: buffered_cell_count as u64 * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid material raster test copy"),
                });
        encoder.copy_buffer_to_buffer(
            proxy.rigid_material_identifiers.wgpu_buffer(),
            0,
            &readback,
            0,
            buffered_cell_count as u64 * 4,
        );
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
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let actual: Vec<u32> = mapped
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        drop(mapped);
        readback.unmap();
        assert!(actual.iter().filter(|identifier| **identifier != 0).count() == 16);
        for y in 2..6 {
            for x in 2..6 {
                let index = (12 * 72 + 12) * 64 + y * 8 + x;
                assert!(
                    actual[index] == material.as_u32(),
                    "missing rigid raster at world cell ({x}, {y})"
                );
            }
        }
    }
}
