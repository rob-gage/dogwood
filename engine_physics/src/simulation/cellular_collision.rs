// Copyright Rob Gage 2026

use super::{
    collision_readback_slot::CollisionReadbackSlot,
    collision_readback_status::CollisionReadbackStatus,
    CollisionOccupancySnapshot,
};
use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};
use std::{
    io,
    sync::{
        Arc,
        Mutex,
    },
};

const READBACK_SLOT_COUNT: usize = 3;

/// Derives compact collision occupancy from the authoritative GPU cellular state
pub struct CellularCollision {
    /// The GPU buffer containing two occupancy words per logical buffered tile
    occupancy: AcceleratorBuffer,
    /// The buffered dimensions and tile-ring offsets used by the compute shader
    parameters: wgpu::Buffer,
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
}

impl CellularCollision {

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
        let byte_size: u64 = u64::from(tile_count) * 8;
        let occupancy: AcceleratorBuffer = accelerator.allocate::<[u32; 2]>(tile_count as usize);
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular collision parameters"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular collision bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cellular_material_identifiers.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: occupancy.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        // build the concrete occupancy extraction pipeline
        let shader: wgpu::ShaderModule = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cellular collision shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cellular_collision.wgsl").into()),
        });
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
                entry_point: Some("extract_occupancy"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        // allocate fixed staging slots so readback never blocks a simulation tick
        let readback_slots: Box<[CollisionReadbackSlot]> = (0..READBACK_SLOT_COUNT).map(|_| {
            CollisionReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cellular collision readback"),
                    size: byte_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                status: Arc::new(Mutex::new(CollisionReadbackStatus::Available)),
            }
        }).collect();
        Self {
            occupancy,
            parameters,
            bind_group,
            pipeline,
            readback_slots,
            latest: None,
            tile_count,
            sequence_next: 0,
        }
    }

    /// Consumes completed readbacks without waiting and retains the newest snapshot
    pub fn collect_completed(&mut self) -> Result<(), io::Error> {
        for slot in &self.readback_slots {
            let mut status: std::sync::MutexGuard<'_, CollisionReadbackStatus> =
                slot.status.lock().map_err(|_| {
                io::Error::other("Cellular collision readback state is unavailable")
            })?;
            if matches!(*status, CollisionReadbackStatus::Complete(_)) {
                let CollisionReadbackStatus::Complete(result): CollisionReadbackStatus =
                    std::mem::replace(&mut *status, CollisionReadbackStatus::Available)
                else { unreachable!() };
                let snapshot: CollisionOccupancySnapshot = result.map_err(io::Error::other)?;
                if self.latest.as_ref().is_none_or(|latest| snapshot.sequence > latest.sequence) {
                    self.latest = Some(snapshot);
                }
            }
        }
        Ok(())
    }

    /// Dispatches extraction and asynchronous readback if a staging slot is available
    pub fn extract(
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
            slot.status.lock().is_ok_and(|status| {
                matches!(*status, CollisionReadbackStatus::Available)
            })
        }) else { return Ok(false); };
        let mut status: std::sync::MutexGuard<'_, CollisionReadbackStatus> =
            slot.status.lock().map_err(|_| {
                io::Error::other("Cellular collision readback state is unavailable")
            })?;
        if !matches!(*status, CollisionReadbackStatus::Available) { return Ok(false); }
        *status = CollisionReadbackStatus::Mapping;
        drop(status);
        let sequence: u64 = self.sequence_next;
        self.sequence_next = self.sequence_next.wrapping_add(1);
        // upload the ring mapping captured for this logical snapshot
        let mut bytes: [u8; 16] = [0; 16];
        for (index, value) in [
            u32::from(width),
            u32::from(height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
        ].into_iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
        // derive occupancy and copy only the compact masks to the staging slot
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular collision extraction") },
        );
        {
            let mut pass: wgpu::ComputePass<'_> =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("cellular collision occupancy extraction"),
                    timestamp_writes: None,
                });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.tile_count, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            self.occupancy.wgpu_buffer(),
            0,
            &slot.buffer,
            0,
            u64::from(self.tile_count) * 8,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        // decode the masks asynchronously with their dispatch-time logical metadata
        let mapped_buffer: wgpu::Buffer = slot.buffer.clone();
        let callback_status: Arc<Mutex<CollisionReadbackStatus>> = slot.status.clone();
        slot.buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let result: Result<CollisionOccupancySnapshot, String> = match result {
                Ok(()) => {
                    let result: Result<CollisionOccupancySnapshot, String> =
                        mapped_buffer.slice(..).get_mapped_range()
                        .map_err(|error| error.to_string()).map(|mapped| {
                            let masks: Box<[[u32; 2]]> = mapped.chunks_exact(8).map(|bytes| [
                                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                                u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                            ]).collect();
                            drop(mapped);
                            CollisionOccupancySnapshot { sequence, origin, width, height, masks }
                        });
                    mapped_buffer.unmap();
                    result
                }
                Err(_) => Err("Cellular collision readback failed".to_owned()),
            };
            if let Ok(mut status) = callback_status.lock() {
                *status = CollisionReadbackStatus::Complete(result);
            }
        });
        Ok(true)
    }

}

impl Drop for CellularCollision {

    /// Destroys the GPU buffers owned by this collision bridge
    fn drop(&mut self) {
        self.occupancy.free();
        self.parameters.destroy();
        for slot in &self.readback_slots { slot.buffer.destroy(); }
    }

}
