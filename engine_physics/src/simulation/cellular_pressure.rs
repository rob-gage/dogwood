// Copyright Rob Gage 2026

use crate::{materials::{Material, MaterialIdentifier, MaterialRegistry}, tiles::{CellCoordinates, TileCoordinates}};
use engine_compute::{Accelerator, AcceleratorBuffer};

const PRESSURE_DAMAGE_RATE: f32 = 10.0;

/// Applies transient directional cellular pressure and static integrity damage.
pub struct CellularPressure {
    static_properties: AcceleratorBuffer,
    dynamic_properties: AcceleratorBuffer,
    pending_impulses: AcceleratorBuffer,
    pressure_a: AcceleratorBuffer,
    pressure_b: AcceleratorBuffer,
    retained_pressure: AcceleratorBuffer,
    /// Transient logical-tile mask covering current pressure sources and their stencil halo
    active_tiles: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    /// Clears the coarse pressure work mask at the start of each fixed tick
    clear_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Finds current pressure sources and activates every tile their fixed stencil can reach
    mark_active_tiles_pipeline: wgpu::ComputePipeline,
    copy_contact_velocity_pipeline: wgpu::ComputePipeline,
    resolve_contacts_pipeline: wgpu::ComputePipeline,
    seed_pipeline: wgpu::ComputePipeline,
    propagate_a_pipeline: wgpu::ComputePipeline,
    propagate_b_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
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
        external_body_count: &AcceleratorBuffer,
        buffered_cell_count: usize,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let static_values: Vec<[u32; 8]> = materials.iter().filter_map(|(_, material)| match material {
            Material::CellularStatic { pressure_ignore_threshold, pressure_transmission,
                debris_material, debris_yield_rate, friction, restitution, .. } => Some([
                    pressure_ignore_threshold.to_bits(), pressure_transmission.to_bits(),
                    debris_material.unwrap_or(MaterialIdentifier::NULL).as_u32(), debris_yield_rate.to_bits(),
                    friction.to_bits(), restitution.to_bits(), 0, 0,
                ]),
            _ => None,
        }).collect();
        let dynamic_values: Vec<[f32; 4]> = materials.iter().filter_map(|(_, material)| match material {
            Material::CellularDynamic { mass, pressure_transmission, friction, restitution, .. } =>
                Some([*mass, *pressure_transmission, *friction, *restitution]),
            _ => None,
        }).collect();
        let static_properties = accelerator.allocate::<[u32; 8]>(static_values.len().max(1));
        let dynamic_properties = accelerator.allocate::<[f32; 4]>(dynamic_values.len().max(1));
        if !static_values.is_empty() { accelerator.wgpu_queue().write_buffer(static_properties.wgpu_buffer(), 0,
            &static_values.iter().flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes())).collect::<Vec<_>>()); }
        if !dynamic_values.is_empty() { accelerator.wgpu_queue().write_buffer(dynamic_properties.wgpu_buffer(), 0,
            &dynamic_values.iter().flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes())).collect::<Vec<_>>()); }
        let count = buffered_cell_count as u32;
        let pending_impulses = accelerator.allocate::<[f32; 4]>(buffered_cell_count);
        let pressure_a = accelerator.allocate::<[f32; 4]>(buffered_cell_count);
        let pressure_b = accelerator.allocate::<[f32; 4]>(buffered_cell_count);
        let retained_pressure = accelerator.allocate::<[f32; 4]>(buffered_cell_count);
        let active_tiles = accelerator.allocate::<u32>(buffered_cell_count / 64);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor { label: Some("cellular pressure parameters"),
            size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false,
                min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("cellular pressure bind group layout"), entries: &[
            storage(0, false), storage(1, false), storage(2, false), storage(3, false), storage(4, true), storage(5, true),
            storage(6, false), storage(7, false), storage(8, false), storage(9, false), storage(10, true), storage(11, true), storage(12, true),
            wgpu::BindGroupLayoutEntry { binding: 13, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            storage(14, false),
        ] });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("cellular pressure bind group"), layout: &layout, entries: &[
            Self::binding(0, material_ids), Self::binding(1, appearances), Self::binding(2, integrities), Self::binding(3, kinematics), Self::binding(4, &static_properties),
            Self::binding(5, &dynamic_properties), Self::binding(6, &pending_impulses), Self::binding(7, &pressure_a), Self::binding(8, &pressure_b),
            Self::binding(9, &retained_pressure), Self::binding(10, external_body_occupancy), Self::binding(11, external_body_velocity),
            Self::binding(12, external_body_count), wgpu::BindGroupEntry { binding: 13, resource: parameters.as_entire_binding() },
            Self::binding(14, &active_tiles),
        ] });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("cellular pressure shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cellular_pressure.wgsl").into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("cellular pressure pipeline layout"),
            bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry_point, label| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(label),
            layout: Some(&pipeline_layout), module: &shader, entry_point: Some(entry_point), compilation_options: Default::default(), cache: None });
        Self { static_properties, dynamic_properties, pending_impulses, pressure_a, pressure_b, retained_pressure, active_tiles, parameters, bind_group,
            impulse_pipeline: pipeline("queue_cellular_radial_impulse", "cellular impulse pipeline"),
            clear_active_tiles_pipeline: pipeline("clear_active_pressure_tiles", "cellular pressure active tile clear pipeline"),
            mark_active_tiles_pipeline: pipeline("mark_active_pressure_tiles", "cellular pressure active tile marking pipeline"),
            copy_contact_velocity_pipeline: pipeline("copy_contact_velocity", "cellular contact velocity copy pipeline"),
            resolve_contacts_pipeline: pipeline("resolve_cellular_contacts", "cellular contact resolution pipeline"),
            seed_pipeline: pipeline("seed_cellular_pressure", "cellular pressure seed pipeline"),
            propagate_a_pipeline: pipeline("propagate_pressure_a", "cellular pressure A propagation"), propagate_b_pipeline: pipeline("propagate_pressure_b", "cellular pressure B propagation"),
            apply_pipeline: pipeline("apply_retained_pressure", "cellular retained pressure pipeline"), buffered_cell_count: count, tick: 0 }
    }

    pub fn apply_radial_impulse(&self, accelerator: &Accelerator, origin: TileCoordinates, width: u16, height: u16, ring_x: u16, ring_y: u16,
        center: CellCoordinates, radius: f32, strength: f32) {
        self.write_parameters(accelerator, origin, width, height, ring_x, ring_y, center, radius, strength, 0.0);
        self.dispatch(accelerator, &self.impulse_pipeline, "queue cellular radial impulse");
    }

    pub fn simulate(&mut self, accelerator: &Accelerator, origin: TileCoordinates, width: u16, height: u16, ring_x: u16, ring_y: u16, delta_time: f32) {
        self.write_parameters(accelerator, origin, width, height, ring_x, ring_y, CellCoordinates { x: 0, y: 0 }, 0.0, 0.0, delta_time);
        let mut encoder = accelerator.wgpu_device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cellular pressure simulation") });
        let tile_count = self.buffered_cell_count / 64;
        for (pipeline, count, label) in [
            (&self.clear_active_tiles_pipeline, tile_count, "clear active cellular pressure tiles"),
            (&self.mark_active_tiles_pipeline, tile_count, "mark active cellular pressure tiles"),
            (&self.copy_contact_velocity_pipeline, self.buffered_cell_count, "copy cellular contact velocities"),
            (&self.resolve_contacts_pipeline, self.buffered_cell_count, "resolve cellular contacts"),
            (&self.seed_pipeline, self.buffered_cell_count, "seed cellular pressure"),
            (&self.propagate_a_pipeline, self.buffered_cell_count, "propagate cellular pressure A 1"),
            (&self.propagate_b_pipeline, self.buffered_cell_count, "propagate cellular pressure B 1"),
            (&self.propagate_a_pipeline, self.buffered_cell_count, "propagate cellular pressure A 2"),
            (&self.propagate_b_pipeline, self.buffered_cell_count, "propagate cellular pressure B 2"),
            (&self.propagate_a_pipeline, self.buffered_cell_count, "propagate cellular pressure A 3"),
            (&self.propagate_b_pipeline, self.buffered_cell_count, "propagate cellular pressure B 3"),
            (&self.apply_pipeline, self.buffered_cell_count, "apply retained cellular pressure"),
        ] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(label), timestamp_writes: None });
            pass.set_pipeline(pipeline); pass.set_bind_group(0, &self.bind_group, &[]); pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn clear_transient_state(&self, accelerator: &Accelerator, cell_start: usize, cell_count: usize) {
        let zeroes = vec![0; cell_count * 16];
        let offset = cell_start as u64 * 16;
        for buffer in [&self.pending_impulses, &self.pressure_a, &self.pressure_b, &self.retained_pressure] {
            accelerator.wgpu_queue().write_buffer(buffer.wgpu_buffer(), offset, &zeroes);
        }
    }

    fn dispatch(&self, accelerator: &Accelerator, pipeline: &wgpu::ComputePipeline, label: &str) {
        let mut encoder = accelerator.wgpu_device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(label), timestamp_writes: None });
        pass.set_pipeline(pipeline); pass.set_bind_group(0, &self.bind_group, &[]); pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
        drop(pass); accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn write_parameters(&self, accelerator: &Accelerator, origin: TileCoordinates, width: u16, height: u16, ring_x: u16, ring_y: u16,
        center: CellCoordinates, radius: f32, strength: f32, delta_time: f32) {
        let values = [origin.x as u32, origin.y as u32, u32::from(width), u32::from(height), u32::from(ring_x), u32::from(ring_y),
            (center.x as f32 + 0.5).to_bits(), (center.y as f32 + 0.5).to_bits(), radius.to_bits(), strength.to_bits(), delta_time.to_bits(), self.tick,
            self.buffered_cell_count, PRESSURE_DAMAGE_RATE.to_bits(), 0, 0];
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &values.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>());
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry { binding, resource: buffer.wgpu_buffer().as_entire_binding() }
    }
}

impl Drop for CellularPressure {
    fn drop(&mut self) { self.static_properties.free(); self.dynamic_properties.free(); self.pending_impulses.free(); self.pressure_a.free(); self.pressure_b.free(); self.retained_pressure.free(); self.active_tiles.free(); self.parameters.destroy(); }
}
