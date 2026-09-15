// Copyright Rob Gage 2026

use super::{
    RigidCellularBody,
    RigidCellularBodyState,
};
use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};

/// Rasterizes physical bodies into transient cellular interaction geometry
pub struct CellularPhysicsBodyProxy {
    occupancy: AcceleratorBuffer,
    velocity: AcceleratorBuffer,
    count: AcceleratorBuffer,
    rigid_material_identifiers: AcceleratorBuffer,
    rigid_appearances: AcceleratorBuffer,
    rigid_claims: AcceleratorBuffer,
    rigid_owners: AcceleratorBuffer,
    rigid_cells: AcceleratorBuffer,
    rigid_transforms: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    pawn_pipeline: wgpu::ComputePipeline,
    rigid_claim_pipeline: wgpu::ComputePipeline,
    rigid_resolve_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    topology_revision: u64,
}

impl CellularPhysicsBodyProxy {

    pub fn new(accelerator: &Accelerator, buffered_cell_count: usize) -> Self {
        let device = accelerator.wgpu_device();
        let buffered_cell_count = buffered_cell_count as u32;
        let occupancy = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let velocity = accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let count = accelerator.allocate::<u32>(1);
        let rigid_material_identifiers = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_appearances = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_claims = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_owners = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let rigid_cells = accelerator.allocate::<[u32; 8]>(buffered_cell_count as usize);
        let rigid_transforms = accelerator.allocate::<[f32; 12]>(buffered_cell_count as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular physics body proxy parameters"), size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false, min_binding_size: None }, count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cellular physics body proxy bind group layout"), entries: &[
                storage(0, false), storage(1, false), storage(2, false),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None }, count: None },
                storage(4, false), storage(5, false), storage(6, false), storage(7, true),
                storage(8, true), storage(9, false),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular physics body proxy bind group"), layout: &layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: occupancy.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: velocity.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: count.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: parameters.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: rigid_material_identifiers.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: rigid_appearances.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: rigid_claims.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: rigid_cells.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: rigid_transforms.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 9, resource: rigid_owners.wgpu_buffer().as_entire_binding() },
            ],
        });
        let shader = super::create_simulation_shader_module(device,
            "cellular physics body proxy shader", include_str!("cellular_physics_body_proxy.wgsl"),
            "engine_physics/src/simulation/cellular_physics_body_proxy.wgsl");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cellular physics body proxy pipeline layout"), bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point, label| device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor { label: Some(label), layout: Some(&pipeline_layout),
                module: &shader, entry_point: Some(entry_point),
                compilation_options: Default::default(), cache: None });
        Self { occupancy, velocity, count, rigid_material_identifiers, rigid_appearances,
            rigid_claims, rigid_owners, rigid_cells, rigid_transforms, parameters, bind_group,
            clear_pipeline: pipeline("clear_cellular_physics_body_proxy", "cellular physics body proxy clear pipeline"),
            pawn_pipeline: pipeline("rasterize_pawn_proxy", "cellular pawn proxy raster pipeline"),
            rigid_claim_pipeline: pipeline("claim_rigid_cell_proxy", "rigid cellular proxy claim pipeline"),
            rigid_resolve_pipeline: pipeline("resolve_rigid_cell_proxy", "rigid cellular proxy resolve pipeline"),
            buffered_cell_count, topology_revision: u64::MAX }
    }

    pub const fn occupancy_buffer(&self) -> &AcceleratorBuffer { &self.occupancy }
    pub const fn velocity_buffer(&self) -> &AcceleratorBuffer { &self.velocity }
    pub const fn count_buffer(&self) -> &AcceleratorBuffer { &self.count }
    pub const fn rigid_material_identifiers_buffer(&self) -> &AcceleratorBuffer
    { &self.rigid_material_identifiers }
    pub const fn rigid_appearances_buffer(&self) -> &AcceleratorBuffer { &self.rigid_appearances }
    pub(crate) const fn rigid_owners_buffer(&self) -> &AcceleratorBuffer { &self.rigid_owners }
    pub(crate) const fn rigid_transforms_buffer(&self) -> &AcceleratorBuffer { &self.rigid_transforms }

    pub(crate) fn rasterize(
        &mut self, accelerator: &Accelerator, origin: TileCoordinates, width: u16, height: u16,
        ring_offset_x: u16, ring_offset_y: u16, gravity: [f32; 2],
        pawn: Option<([f32; 2], [f32; 2], [f32; 2], [f32; 2])>, is_fluid_permeable: bool,
        bodies: &[RigidCellularBody],
        body_states: &[RigidCellularBodyState], topology_revision: u64,
    ) {
        // ponytail: fixed resident-cell capacity; growable topology storage if dense body counts matter
        let rigid_cell_count: u32 = bodies.iter().map(|body| body.cells.len() as u32).sum::<u32>()
            .min(self.buffered_cell_count);
        if self.topology_revision != topology_revision {
            let cells: Vec<u8> = bodies.iter().enumerate().flat_map(|(body, rigid_body)| {
                rigid_body.cells.iter().map(move |(local, material, appearance)| [
                    local[0] as u32, local[1] as u32, body as u32, material.as_u32(),
                    appearance.0, 0, 0, 0,
                ])
            }).take(self.buffered_cell_count as usize).flatten()
                .flat_map(u32::to_le_bytes).collect();
            accelerator.wgpu_queue().write_buffer(self.rigid_cells.wgpu_buffer(), 0, &cells);
            self.topology_revision = topology_revision;
        }
        let transforms: Vec<u8> = body_states.iter().flat_map(|state| [
                state.translation[0], state.translation[1], state.angle.cos(), state.angle.sin(),
                state.linear_velocity[0], state.linear_velocity[1], state.angular_velocity, 0.0,
                state.center_of_mass[0], state.center_of_mass[1], state.inverse_mass,
                state.inverse_angular_inertia,
            ]).flat_map(f32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(self.rigid_transforms.wgpu_buffer(), 0, &transforms);
        let (center, velocity, collider, drive, occupancy) = pawn.map_or(
            ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2], 0u32),
            |(center, velocity, collider, drive)| (center, velocity, collider, drive,
                if is_fluid_permeable { 2u32 } else { 1u32 }));
        let values = [origin.x as u32, origin.y as u32, u32::from(width), u32::from(height),
            u32::from(ring_offset_x), u32::from(ring_offset_y), center[0].to_bits(), center[1].to_bits(),
            velocity[0].to_bits(), velocity[1].to_bits(), drive[0].to_bits(), drive[1].to_bits(),
            collider[0].to_bits(), collider[1].to_bits(), gravity[0].to_bits(), gravity[1].to_bits(),
            self.buffered_cell_count, occupancy, rigid_cell_count, bodies.len() as u32, 0, 0, 0, 0];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
        let mut encoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular physics body proxy rasterization") });
        for (pipeline, label, count) in [
            (&self.clear_pipeline, "clear cellular physics body proxy", self.buffered_cell_count),
            (&self.pawn_pipeline, "rasterize cellular pawn proxy", self.buffered_cell_count),
            (&self.rigid_claim_pipeline, "claim rigid cellular proxy", rigid_cell_count),
            (&self.rigid_resolve_pipeline, "resolve rigid cellular proxy", rigid_cell_count),
        ] {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for CellularPhysicsBodyProxy {

    fn drop(&mut self) {
        self.occupancy.free(); self.velocity.free(); self.count.free();
        self.rigid_material_identifiers.free(); self.rigid_appearances.free();
        self.rigid_claims.free(); self.rigid_owners.free(); self.rigid_cells.free();
        self.rigid_transforms.free();
        self.parameters.destroy();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        materials::{
            MaterialForm,
            MaterialIdentifier,
        },
        tiles::CellularAppearance,
    };
    use rapier2d::prelude::RigidBodyHandle;
    use std::{
        sync::mpsc,
        time::{
            Duration,
            Instant,
        },
    };

    #[test]
    fn axis_aligned_rigid_block_raster_has_every_cell_once() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 3);
        let body = RigidCellularBody {
            handle: RigidBodyHandle::invalid(),
            cells: (2..6).flat_map(|y| (2..6).map(move |x| {
                ([x, y], material, CellularAppearance::NEUTRAL)
            })).collect(),
        };
        assert!(std::mem::size_of::<[u32; 8]>() == 32);
        println!("rigid bodies: 1, rigid cells: {}", body.cells.len());
        let buffered_cell_count: usize = 72 * 51 * 64;
        let mut proxy = CellularPhysicsBodyProxy::new(&accelerator, buffered_cell_count);
        let bodies = [body];
        let body_states = [RigidCellularBodyState {
            translation: [0.0; 2], angle: 0.0, linear_velocity: [0.0; 2],
            angular_velocity: 0.0, center_of_mass: [0.5; 2], inverse_mass: 1.0,
            inverse_angular_inertia: 1.0,
        }];
        proxy.rasterize(
            &accelerator,
            TileCoordinates { x: -12, y: -12 },
            72,
            51,
            0,
            0,
            [0.0, -1.0],
            None,
            false,
            &bodies,
            &body_states,
            0,
        );
        let readback = accelerator.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid material raster test readback"),
            size: buffered_cell_count as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("rigid material raster test copy") },
        );
        encoder.copy_buffer_to_buffer(
            proxy.rigid_material_identifiers.wgpu_buffer(), 0, &readback, 0,
            buffered_cell_count as u64 * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
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
        let actual: Vec<u32> = mapped.chunks_exact(4).map(|bytes| {
            u32::from_le_bytes(bytes.try_into().unwrap())
        }).collect();
        drop(mapped);
        readback.unmap();
        assert!(actual.iter().filter(|identifier| **identifier != 0).count() == 16);
        for y in 2..6 {
            for x in 2..6 {
                let index = (12 * 72 + 12) * 64 + y * 8 + x;
                assert!(actual[index] == material.as_u32(),
                    "missing rigid raster at world cell ({x}, {y})");
            }
        }
    }

}
