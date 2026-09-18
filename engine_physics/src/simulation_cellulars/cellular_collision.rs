// Copyright Rob Gage 2026

use std::io;
use std::sync::Arc;
use std::sync::Mutex;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use super::CollisionReadbackSlot;
use super::CollisionReadbackStatus;
use crate::simulation::CollisionOccupancySnapshot;
use crate::simulation::simulation_constants::READBACK_SLOT_COUNT;
use crate::tiles::TileCoordinates;

/// Derives compact collision occupancy from the authoritative Accelerator cellular state
pub struct CellularCollision {
    /// The Accelerator buffer containing static and dynamic occupancy words per logical buffered tile
    occupancy: AcceleratorBuffer,
    /// The buffered dimensions and tile-ring offsets used by the compute shader
    cellular_collision_parameters: wgpu::Buffer,
    /// The cellular input, occupancy output, and parameter bindings
    bind_group: wgpu::BindGroup,
    /// The pipeline that derives tile occupancy
    pipeline: wgpu::ComputePipeline,
    /// Fixed staging buffers used for asynchronous CPU readback
    readback_slots: Box<[CollisionReadbackSlot]>,
    /// The newest completed CPU occupancy snapshot
    pub latest: Option<CollisionOccupancySnapshot>,
    /// The number of logical tiles processed by each dispatch
    tile_count: u32,
    /// The sequence assigned to the next dispatched snapshot
    sequence_next: u64,
    /// The newest completion accepted even after its snapshot is consumed
    sequence_latest: Option<u64>,
}

impl CellularCollision {
    pub(crate) fn snapshot_age(&self, sequence: u64) -> u64 {
        self.sequence_next
            .saturating_sub(sequence.saturating_add(1))
    }
    /// Creates the collision extraction pipeline and its fixed-size buffers
    pub fn new(
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        width: u16,
        height: u16,
    ) -> Self {
        // size the occupancy and readback storage for the full buffered area
        let device: &wgpu::Device = accelerator.wgpu_device();
        let tile_count: u32 = u32::from(width) * u32::from(height);
        let byte_size: u64 = u64::from(tile_count) * 16;
        let occupancy: AcceleratorBuffer = accelerator.allocate::<[u32; 4]>(tile_count as usize);
        let cellular_collision_parameters: wgpu::Buffer =
            crate::simulation::create_simulation_uniform_buffer(
                device,
                "cellular collision parameters",
                64,
            );
        // describe the authoritative input, derived output, and ring parameters
        let bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular collision bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    crate::simulation::uniform_bind_group_layout_entry(2),
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular collision bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cellular_material_identifiers
                        .wgpu_buffer()
                        .as_entire_binding(),
                },
                crate::simulation::accelerator_buffer_bind_group_entry(1, &occupancy),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_collision_parameters.as_entire_binding(),
                },
            ],
        });
        // build the concrete occupancy extraction pipeline
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "cellular collision shader",
            include_str!("cellular_collision.wgsl"),
            "engine_physics/src/simulation_cellulars/cellular_collision.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cellular collision pipeline layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let pipeline: wgpu::ComputePipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cellular collision pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("extract_cellular_collision_occupancy"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        // allocate fixed staging slots so readback never blocks a simulation tick
        let readback_slots: Box<[CollisionReadbackSlot]> = (0..READBACK_SLOT_COUNT)
            .map(|_| CollisionReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cellular collision readback"),
                    size: byte_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                status: Arc::new(Mutex::new(CollisionReadbackStatus::Available)),
            })
            .collect();
        Self {
            occupancy,
            cellular_collision_parameters,
            bind_group,
            pipeline,
            readback_slots,
            latest: None,
            tile_count,
            sequence_next: 0,
            sequence_latest: None,
        }
    }

    /// Consumes completed readbacks without waiting and retains the newest snapshot
    pub fn collect_collision(&mut self) -> Result<(), io::Error> {
        for slot in &self.readback_slots {
            let mut status: std::sync::MutexGuard<'_, CollisionReadbackStatus> =
                slot.status.lock().map_err(|_| {
                    io::Error::other("Cellular collision readback state is unavailable")
                })?;
            if matches!(*status, CollisionReadbackStatus::Complete(_)) {
                let CollisionReadbackStatus::Complete(result): CollisionReadbackStatus =
                    std::mem::replace(&mut *status, CollisionReadbackStatus::Available)
                else {
                    unreachable!()
                };
                let snapshot: CollisionOccupancySnapshot = result.map_err(io::Error::other)?;
                if self
                    .sequence_latest
                    .is_none_or(|latest| snapshot.sequence > latest)
                {
                    self.sequence_latest = Some(snapshot.sequence);
                    self.latest = Some(snapshot);
                }
            }
        }
        Ok(())
    }

    /// Dispatches extraction and asynchronous readback if a staging slot is available
    pub fn extract_collision(
        &mut self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) -> Result<bool, io::Error> {
        // claim a free staging slot or skip this snapshot without waiting
        let Some(slot): Option<&CollisionReadbackSlot> = self.readback_slots.iter().find(|slot| {
            slot.status
                .lock()
                .is_ok_and(|status| matches!(*status, CollisionReadbackStatus::Available))
        }) else {
            return Ok(false);
        };
        let mut status: std::sync::MutexGuard<'_, CollisionReadbackStatus> = slot
            .status
            .lock()
            .map_err(|_| io::Error::other("Cellular collision readback state is unavailable"))?;
        if !matches!(*status, CollisionReadbackStatus::Available) {
            return Ok(false);
        }
        *status = CollisionReadbackStatus::Mapping;
        drop(status);
        let sequence: u64 = self.sequence_next;
        self.sequence_next = self.sequence_next.wrapping_add(1);
        // upload the ring mapping captured for this logical snapshot
        let cellular_collision_parameter_values: [u32; 16] = [
            origin.x as u32,
            origin.y as u32,
            u32::from(width),
            u32::from(height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
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
        let mut cellular_collision_parameter_bytes: [u8; 64] = [0; 64];
        for (parameter_index, parameter_value) in
            cellular_collision_parameter_values.into_iter().enumerate()
        {
            cellular_collision_parameter_bytes[parameter_index * 4..parameter_index * 4 + 4]
                .copy_from_slice(&parameter_value.to_le_bytes());
        }
        accelerator.wgpu_queue().write_buffer(
            &self.cellular_collision_parameters,
            0,
            &cellular_collision_parameter_bytes,
        );
        // derive occupancy and copy only the compact masks to the staging slot
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular collision extraction"),
                });
        {
            let mut cellular_collision_compute_pass: wgpu::ComputePass<'_> = accelerator
                .begin_compute_pass(&mut encoder, "cellular collision occupancy extraction");
            cellular_collision_compute_pass.set_pipeline(&self.pipeline);
            cellular_collision_compute_pass.set_bind_group(0, &self.bind_group, &[]);
            cellular_collision_compute_pass.dispatch_workgroups(self.tile_count, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            self.occupancy.wgpu_buffer(),
            0,
            &slot.buffer,
            0,
            u64::from(self.tile_count) * 16,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        // decode the masks asynchronously with their dispatch-time logical metadata
        let mapped_collision_readback_buffer: wgpu::Buffer = slot.buffer.clone();
        let callback_status: Arc<Mutex<CollisionReadbackStatus>> = slot.status.clone();
        slot.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |collision_mapping_result| {
                let collision_snapshot_result: Result<CollisionOccupancySnapshot, String> =
                    match collision_mapping_result {
                        Ok(()) => {
                            let collision_snapshot_result: Result<
                                CollisionOccupancySnapshot,
                                String,
                            > = mapped_collision_readback_buffer
                                .slice(..)
                                .get_mapped_range()
                                .map_err(|error| error.to_string())
                                .map(|mapped| {
                                    let mut static_masks: Vec<[u32; 2]> =
                                        Vec::with_capacity(mapped.len() / 16);
                                    let mut dynamic_masks: Vec<[u32; 2]> =
                                        Vec::with_capacity(mapped.len() / 16);
                                    for collision_record_bytes in mapped.as_chunks::<16>().0 {
                                        static_masks.push([
                                            u32::from_le_bytes(
                                                collision_record_bytes[0..4].try_into().unwrap(),
                                            ),
                                            u32::from_le_bytes(
                                                collision_record_bytes[4..8].try_into().unwrap(),
                                            ),
                                        ]);
                                        dynamic_masks.push([
                                            u32::from_le_bytes(
                                                collision_record_bytes[8..12].try_into().unwrap(),
                                            ),
                                            u32::from_le_bytes(
                                                collision_record_bytes[12..16].try_into().unwrap(),
                                            ),
                                        ]);
                                    }
                                    drop(mapped);
                                    CollisionOccupancySnapshot {
                                        sequence,
                                        origin,
                                        width,
                                        height,
                                        static_masks: static_masks.into_boxed_slice(),
                                        dynamic_masks: dynamic_masks.into_boxed_slice(),
                                    }
                                });
                            mapped_collision_readback_buffer.unmap();
                            collision_snapshot_result
                        }
                        Err(_) => Err("Cellular collision readback failed".to_owned()),
                    };
                if let Ok(mut status) = callback_status.lock() {
                    *status = CollisionReadbackStatus::Complete(collision_snapshot_result);
                }
            });
        Ok(true)
    }
}

impl Drop for CellularCollision {
    /// Destroys the Accelerator buffers owned by this collision bridge
    fn drop(&mut self) {
        self.occupancy.free();
        self.cellular_collision_parameters.destroy();
        for slot in &self.readback_slots {
            slot.buffer.destroy();
        }
    }
}
