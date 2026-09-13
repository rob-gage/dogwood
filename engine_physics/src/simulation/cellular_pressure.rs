// Copyright Rob Gage 2026

use crate::{
    materials::{Material, MaterialForm, MaterialIdentifier, MaterialRegistry},
    tiles::{CellCoordinates, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};

const PRESSURE_PROPAGATION_ITERATIONS: u32 = 4;
const IMPULSE_TO_PRESSURE: f32 = 4.0;
const PRESSURE_DAMAGE_RATE: f32 = 10.0;

/// Applies radial impulse and concrete scalar structural pressure to cellular state.
pub struct CellularPressure {
    static_material_properties: AcceleratorBuffer,
    pending_impulses: AcceleratorBuffer,
    contact_loads: AcceleratorBuffer,
    pressure_a: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    contact_pipeline: wgpu::ComputePipeline,
    clear_contact_pipeline: wgpu::ComputePipeline,
    seed_pipeline: wgpu::ComputePipeline,
    propagate_pipeline: wgpu::ComputePipeline,
    commit_pipeline: wgpu::ComputePipeline,
    damage_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    tick: u32,
}

impl CellularPressure {

    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        cellular_material_identifiers: &AcceleratorBuffer,
        cellular_appearances: &AcceleratorBuffer,
        cellular_integrities: &AcceleratorBuffer,
        cellular_kinematics: &AcceleratorBuffer,
        buffered_cell_count: usize,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let buffered_cell_count: u32 = buffered_cell_count as u32;
        let properties: Vec<[u32; 4]> = materials.iter().filter_map(|(_, material)| {
            let Material::CellularStatic {
                pressure_ignore_threshold,
                default_integrity,
                debris_material,
                debris_yield_rate,
                ..
            } = material else { return None; };
            if let Some(identifier) = debris_material {
                assert!(matches!(materials.get(*identifier), Some(Material::CellularDynamic { .. })));
                assert!(identifier.form() == MaterialForm::CellularDynamic);
            }
            Some([
                pressure_ignore_threshold.to_bits(),
                default_integrity.to_bits(),
                debris_material.unwrap_or(MaterialIdentifier::NULL).as_u32(),
                debris_yield_rate.to_bits(),
            ])
        }).collect();
        let static_material_properties: AcceleratorBuffer =
            accelerator.allocate::<[u32; 4]>(properties.len().max(1));
        let property_bytes: Vec<u8> = properties.iter().flat_map(|value| {
            value.iter().flat_map(|word| word.to_le_bytes()).collect::<Vec<u8>>()
        }).collect();
        if !property_bytes.is_empty() {
            accelerator.wgpu_queue().write_buffer(
                static_material_properties.wgpu_buffer(), 0, &property_bytes,
            );
        }
        let pending_impulses = accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let contact_loads = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let pressure_a = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular pressure parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entries = [
            Self::storage(0, false), Self::storage(1, false), Self::storage(2, false),
            Self::storage(3, false), Self::storage(4, true), Self::storage(5, false),
            Self::storage(6, false), Self::storage(7, false),
            wgpu::BindGroupLayoutEntry {
                binding: 8, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: None }, count: None,
            },
        ];
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cellular pressure bind group layout"), entries: &entries,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular pressure bind group"), layout: &layout,
            entries: &[
                Self::binding(0, cellular_material_identifiers),
                Self::binding(1, cellular_appearances),
                Self::binding(2, cellular_integrities),
                Self::binding(3, cellular_kinematics),
                Self::binding(4, &static_material_properties),
                Self::binding(5, &pending_impulses),
                Self::binding(6, &contact_loads),
                Self::binding(7, &pressure_a),
                wgpu::BindGroupEntry { binding: 8, resource: parameters.as_entire_binding() },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cellular pressure shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cellular_pressure.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cellular pressure pipeline layout"),
            bind_group_layouts: &[Some(&layout)], immediate_size: 0,
        });
        let pipeline = |entry_point: &str, label: &str| device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some(label), layout: Some(&pipeline_layout), module: &shader,
                entry_point: Some(entry_point), compilation_options: Default::default(), cache: None,
            }
        );
        Self {
            static_material_properties, pending_impulses, contact_loads, pressure_a,
            parameters, bind_group,
            impulse_pipeline: pipeline("apply_cellular_radial_impulse", "cellular impulse pipeline"),
            contact_pipeline: pipeline("accumulate_cellular_contacts", "cellular contact load pipeline"),
            clear_contact_pipeline: pipeline("clear_cellular_contact_loads", "cellular contact clear pipeline"),
            seed_pipeline: pipeline("seed_cellular_pressure", "cellular pressure seed pipeline"),
            propagate_pipeline: pipeline("propagate_cellular_pressure", "cellular pressure propagation pipeline"),
            commit_pipeline: pipeline("commit_cellular_pressure", "cellular pressure commit pipeline"),
            damage_pipeline: pipeline("damage_cellular_integrity", "cellular integrity damage pipeline"),
            buffered_cell_count, tick: 0,
        }
    }

    pub fn apply_radial_impulse(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
    ) {
        let parameters = Self::parameters(
            buffered_origin, buffered_width, buffered_height, ring_offset_x, ring_offset_y,
            center, radius, strength, 0.0, self.tick, self.buffered_cell_count,
        );
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &parameters);
        let mut encoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular radial impulse") },
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cellular radial impulse pass"), timestamp_writes: None,
        });
        pass.set_pipeline(&self.impulse_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    pub fn simulate(
        &mut self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        delta_time: f32,
    ) {
        let parameters = Self::parameters(
            buffered_origin, buffered_width, buffered_height, ring_offset_x, ring_offset_y,
            CellCoordinates { x: 0, y: 0 }, 0.0, 0.0, delta_time, self.tick,
            self.buffered_cell_count,
        );
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &parameters);
        let mut encoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular pressure simulation") },
        );
        self.dispatch(&mut encoder, &self.clear_contact_pipeline, "clear cellular contact loads");
        self.dispatch(&mut encoder, &self.contact_pipeline, "cellular contact loads");
        self.dispatch(&mut encoder, &self.seed_pipeline, "cellular pressure seed");
        for _ in 0..PRESSURE_PROPAGATION_ITERATIONS {
            self.dispatch(&mut encoder, &self.propagate_pipeline, "cellular pressure propagation");
            self.dispatch(&mut encoder, &self.commit_pipeline, "cellular pressure commit");
        }
        self.dispatch(&mut encoder, &self.damage_pipeline, "cellular integrity damage");
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.tick = self.tick.wrapping_add(1);
    }

    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::ComputePipeline, label: &str) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label), timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
    }

    fn parameters(
        origin: TileCoordinates, width: u16, height: u16, ring_x: u16, ring_y: u16,
        center: CellCoordinates, radius: f32, strength: f32, delta_time: f32,
        tick: u32, count: u32,
    ) -> Vec<u8> {
        let values = [
            origin.x as u32, origin.y as u32, u32::from(width), u32::from(height),
            u32::from(ring_x), u32::from(ring_y),
            (center.x as f32 + 0.5).to_bits(), (center.y as f32 + 0.5).to_bits(),
            radius.to_bits(), strength.to_bits(), delta_time.to_bits(), tick, count,
            IMPULSE_TO_PRESSURE.to_bits(), PRESSURE_DAMAGE_RATE.to_bits(), 0,
        ];
        values.into_iter().flat_map(u32::to_le_bytes).collect()
    }

    fn storage(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false, min_binding_size: None }, count: None }
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry { binding, resource: buffer.wgpu_buffer().as_entire_binding() }
    }

}

impl Drop for CellularPressure {

    fn drop(&mut self) {
        self.static_material_properties.free();
        self.pending_impulses.free();
        self.contact_loads.free();
        self.pressure_a.free();
        self.parameters.destroy();
    }

}
