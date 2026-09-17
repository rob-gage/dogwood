// Copyright Rob Gage 2026

use crate::{
    materials::{Material, MaterialRegistry},
    scenes::{GasDownload, GasUpload},
    tiles::{TileArea, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};

const PRESSURE_ITERATION_COUNT: u32 = 12;
const VORTICITY_CONFINEMENT: f32 = 0.2;
const BUOYANCY_COEFFICIENT: f32 = 0.05;
const MAXIMUM_SPEED_CELLS_PER_SECOND: f32 = 8.0;
const FLUID_OBSTACLE_COVERAGE: f32 = 0.85;
const AMBIENT_DENSITY: f32 = 1.0;
const AUTHORED_CONCENTRATION: f32 = 1.0;

/// Owns the shared Eulerian gas velocity and per-species concentration fields
pub struct Gases {
    /// Authoritative shared gas-mixture velocity in each physical cell
    velocity: AcceleratorBuffer,
    /// Velocity advection scratch field
    velocity_scratch: AcceleratorBuffer,
    /// Authoritative species-major concentrations
    concentrations: AcceleratorBuffer,
    /// Persistent shared gas-mixture temperature per physical cell.
    gas_temperature: AcceleratorBuffer,
    /// Species advection and diffusion scratch field
    concentration_scratch: AcceleratorBuffer,
    /// Projection divergence scratch field
    divergence: AcceleratorBuffer,
    /// First pressure Jacobi buffer
    pressure_a: AcceleratorBuffer,
    /// Second and final pressure Jacobi buffer
    pressure_b: AcceleratorBuffer,
    /// Scalar two-dimensional vorticity field
    curl: AcceleratorBuffer,
    /// Dense fixed-stride records used only during residency export
    streaming_data: AcceleratorBuffer,
    /// Buffered ring mapping, gravity, and solver constants
    parameters: wgpu::Buffer,
    /// All concrete gas solver bindings
    bind_group: wgpu::BindGroup,
    advect_velocity_pipeline: wgpu::ComputePipeline,
    curl_pipeline: wgpu::ComputePipeline,
    force_pipeline: wgpu::ComputePipeline,
    divergence_pipeline: wgpu::ComputePipeline,
    pressure_clear_pipeline: wgpu::ComputePipeline,
    pressure_a_pipeline: wgpu::ComputePipeline,
    pressure_b_pipeline: wgpu::ComputePipeline,
    projection_pipeline: wgpu::ComputePipeline,
    concentration_pipeline: wgpu::ComputePipeline,
    clear_area_pipeline: wgpu::ComputePipeline,
    export_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    gas_count: u32,
    ambient_temperature: f32,
}

impl Gases {
    /// Creates dense gas fields matching the physical cellular tile ring
    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        cellular_material_identifiers: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        gas_properties: &AcceleratorBuffer,
        buffered_cell_count: usize,
        ambient_temperature: f32,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let buffered_cell_count: u32 = buffered_cell_count
            .try_into()
            .expect("Gas buffer exceeds GPU indexing range");
        let gas_count: u32 = materials
            .iter()
            .filter(|(_, material)| matches!(material, Material::Gas { .. }))
            .count()
            .try_into()
            .expect("Gas species count exceeds GPU indexing range");
        let concentration_count: u32 = buffered_cell_count
            .checked_mul(gas_count)
            .expect("Gas concentration buffer exceeds GPU indexing range");
        let streaming_value_count: u32 = buffered_cell_count
            .checked_mul(
                5u32.checked_add(gas_count)
                    .expect("Gas streaming record is too large"),
            )
            .expect("Gas streaming buffer exceeds GPU indexing range");
        let velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(buffered_cell_count as usize);
        let velocity_scratch: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(buffered_cell_count as usize);
        let concentrations: AcceleratorBuffer =
            accelerator.allocate::<f32>(concentration_count.max(1) as usize);
        let gas_temperature = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let concentration_scratch: AcceleratorBuffer =
            accelerator.allocate::<f32>(concentration_count.max(1) as usize);
        let divergence: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let pressure_a: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let pressure_b: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let curl: AcceleratorBuffer = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let streaming_data: AcceleratorBuffer =
            accelerator.allocate::<u32>(streaming_value_count as usize);
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gas simulation parameters"),
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
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gas simulation bind group layout"),
                entries: &[
                    storage(0, false),
                    storage(1, false),
                    storage(2, false),
                    storage(3, false),
                    storage(4, false),
                    storage(5, false),
                    storage(6, false),
                    storage(7, false),
                    storage(8, true),
                    storage(9, true),
                    storage(10, true),
                    storage(11, true),
                    storage(12, false),
                    storage(14, false),
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
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gas simulation bind group"),
            layout: &layout,
            entries: &[
                Self::binding(0, &velocity),
                Self::binding(1, &velocity_scratch),
                Self::binding(2, &concentrations),
                Self::binding(3, &concentration_scratch),
                Self::binding(4, &divergence),
                Self::binding(5, &pressure_a),
                Self::binding(6, &pressure_b),
                Self::binding(7, &curl),
                Self::binding(8, cellular_material_identifiers),
                Self::binding(9, external_body_occupancy),
                Self::binding(10, fluid_coverage),
                Self::binding(11, gas_properties),
                Self::binding(12, &streaming_data),
                Self::binding(14, &gas_temperature),
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let shader: wgpu::ShaderModule = super::create_simulation_shader_module(
            device,
            "gas simulation shader",
            include_str!("gases.wgsl"),
            "engine_physics/src/simulation/gases.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gas simulation pipeline layout"),
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
            velocity,
            velocity_scratch,
            concentrations,
            gas_temperature,
            concentration_scratch,
            divergence,
            pressure_a,
            pressure_b,
            curl,
            streaming_data,
            parameters,
            bind_group,
            advect_velocity_pipeline: pipeline(
                "advect_gas_velocity",
                "gas velocity advection pipeline",
            ),
            curl_pipeline: pipeline("calculate_gas_curl", "gas curl pipeline"),
            force_pipeline: pipeline("apply_gas_forces", "gas force pipeline"),
            divergence_pipeline: pipeline("calculate_gas_divergence", "gas divergence pipeline"),
            pressure_clear_pipeline: pipeline("clear_gas_pressure", "gas pressure clear pipeline"),
            pressure_a_pipeline: pipeline("solve_gas_pressure_a", "gas pressure A pipeline"),
            pressure_b_pipeline: pipeline("solve_gas_pressure_b", "gas pressure B pipeline"),
            projection_pipeline: pipeline(
                "project_gas_velocity",
                "gas velocity projection pipeline",
            ),
            concentration_pipeline: pipeline(
                "advect_gas_concentrations",
                "gas concentration advection pipeline",
            ),
            clear_area_pipeline: pipeline("clear_gas_area", "gas streamed area clear pipeline"),
            export_pipeline: pipeline("export_gas_area", "gas streamed area export pipeline"),
            buffered_cell_count,
            gas_count,
            ambient_temperature,
        }
    }

    /// Returns the authoritative species-major concentration allocation
    pub const fn concentrations_buffer(&self) -> &AcceleratorBuffer {
        &self.concentrations
    }

    pub(crate) const fn temperature_buffer(&self) -> &AcceleratorBuffer {
        &self.gas_temperature
    }

    /// Returns the number of independently registered gas species
    pub const fn gas_count(&self) -> u32 {
        self.gas_count
    }

    /// Applies stable authored concentrations and explicit cell clears
    pub fn apply_edits(
        &self,
        accelerator: &Accelerator,
        concentrations: &[(usize, u32, f32)],
        clear_cells: &[usize],
    ) {
        fn runs(indices: &[usize], mut write: impl FnMut(usize, usize)) {
            let mut start = 0;
            while start < indices.len() {
                let mut end = start + 1;
                while end < indices.len() && indices[end] == indices[end - 1] + 1 {
                    end += 1;
                }
                write(indices[start], end - start);
                start = end;
            }
        }
        runs(clear_cells, |index, count| {
            accelerator.wgpu_queue().write_buffer(
                self.velocity.wgpu_buffer(),
                index as u64 * 8,
                &vec![0; count * 8],
            )
        });
        for species in 0..self.gas_count {
            runs(clear_cells, |index, count| {
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    (u64::from(species) * u64::from(self.buffered_cell_count) + index as u64) * 4,
                    &vec![0; count * 4],
                )
            });
            let mut authored: Vec<usize> = concentrations
                .iter()
                .filter_map(|(cell, value, _)| (*value == species).then_some(*cell))
                .collect();
            authored.sort_unstable();
            runs(&authored, |index, count| {
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    (u64::from(species) * u64::from(self.buffered_cell_count) + index as u64) * 4,
                    &vec![AUTHORED_CONCENTRATION.to_bits().to_le_bytes(); count]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>(),
                )
            });
        }
        let mut temperatures: Vec<(usize, f32)> = concentrations
            .iter()
            .map(|&(cell, _, temperature)| (cell, temperature))
            .collect();
        temperatures.sort_unstable_by_key(|&(cell, _)| cell);
        let mut start = 0;
        while start < temperatures.len() {
            let mut end = start + 1;
            while end < temperatures.len() && temperatures[end].0 == temperatures[end - 1].0 + 1 {
                end += 1;
            }
            let bytes: Vec<u8> = temperatures[start..end]
                .iter()
                .flat_map(|&(_, temperature)| temperature.to_bits().to_le_bytes())
                .collect();
            accelerator.wgpu_queue().write_buffer(
                self.gas_temperature.wgpu_buffer(),
                temperatures[start].0 as u64 * 4,
                &bytes,
            );
            start = end;
        }
    }

    /// Advances the full resident gas field without CPU readback
    pub fn simulate(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.simulate_pre_coupling(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            gravity,
            delta_time,
        );
        self.simulate_post_coupling(accelerator);
    }

    pub(crate) const fn velocity_buffer(&self) -> &AcceleratorBuffer {
        &self.velocity
    }

    pub(crate) fn simulate_pre_coupling(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        if self.gas_count == 0 {
            return;
        }
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            gravity,
            delta_time,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas simulation"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.advect_velocity_pipeline,
            self.buffered_cell_count,
            "advect gas velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.curl_pipeline,
            self.buffered_cell_count,
            "calculate gas curl",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.force_pipeline,
            self.buffered_cell_count,
            "apply gas buoyancy and vorticity confinement",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    pub(crate) fn simulate_post_coupling(&self, accelerator: &Accelerator) {
        if self.gas_count == 0 {
            return;
        }
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas projection and transport"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.divergence_pipeline,
            self.buffered_cell_count,
            "calculate gas divergence",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.pressure_clear_pipeline,
            self.buffered_cell_count,
            "clear gas pressure",
        );
        for iteration in 0..PRESSURE_ITERATION_COUNT {
            let (pipeline, label): (&wgpu::ComputePipeline, &str) = if iteration % 2 == 0 {
                (&self.pressure_a_pipeline, "solve gas pressure into A")
            } else {
                (&self.pressure_b_pipeline, "solve gas pressure into B")
            };
            self.dispatch(
                accelerator,
                &mut encoder,
                pipeline,
                self.buffered_cell_count,
                label,
            );
        }
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.projection_pipeline,
            self.buffered_cell_count,
            "project gas velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.concentration_pipeline,
            self.buffered_cell_count * self.gas_count,
            "advect gas concentrations",
        );
        encoder.copy_buffer_to_buffer(
            self.concentration_scratch.wgpu_buffer(),
            0,
            self.concentrations.wgpu_buffer(),
            0,
            u64::from(self.buffered_cell_count) * u64::from(self.gas_count) * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Clears physical slots after they acquire a new world interpretation
    pub fn clear_area(
        &self,
        accelerator: &Accelerator,
        area: TileArea,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        if self.gas_count == 0 {
            return;
        }
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            [0.0; 2],
            0.0,
            Some(area),
        );
        let dimensions: [u16; 2] = area.dimensions();
        let count: u32 = u32::from(dimensions[0]) * u32::from(dimensions[1]) * 64;
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas streamed area clear"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.clear_area_pipeline,
            count,
            "clear incoming gas area",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Restores validated dormant gas directly into its dense authoritative slots
    pub fn import(
        &self,
        accelerator: &Accelerator,
        upload: &GasUpload,
        physical_indices: &[usize],
    ) {
        assert!(upload.cells.len() == physical_indices.len());
        for (physical_index, cell) in physical_indices.iter().zip(&upload.cells) {
            let mut velocity: Vec<u8> = Vec::with_capacity(8);
            for value in cell.velocity {
                velocity.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            accelerator.wgpu_queue().write_buffer(
                self.velocity.wgpu_buffer(),
                *physical_index as u64 * 8,
                &velocity,
            );
            accelerator.wgpu_queue().write_buffer(
                self.gas_temperature.wgpu_buffer(),
                *physical_index as u64 * 4,
                &cell.temperature.to_bits().to_le_bytes(),
            );
            for (identifier, concentration) in &cell.species {
                let index: u64 = u64::from(identifier.index())
                    * u64::from(self.buffered_cell_count)
                    + *physical_index as u64;
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    index * 4,
                    &concentration.to_bits().to_le_bytes(),
                );
            }
        }
    }

    /// Captures and clears an outgoing strip under the current ring interpretation
    pub fn export(
        &self,
        accelerator: &Accelerator,
        download: &GasDownload,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            [0.0; 2],
            0.0,
            Some(download.area),
        );
        let dimensions: [u16; 2] = download.area.dimensions();
        let count: u32 = u32::from(dimensions[0]) * u32::from(dimensions[1]) * 64;
        let byte_count: u64 = u64::from(count) * u64::from(5 + self.gas_count) * 4;
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas export"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.export_pipeline,
            count,
            "export outgoing gas area",
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_data.wgpu_buffer(),
            0,
            &download.buffer,
            0,
            byte_count,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn dispatch(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        count: u32,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
    }

    fn write_parameters(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
        streaming_area: Option<TileArea>,
    ) {
        let streaming_origin: TileCoordinates =
            streaming_area.map_or(TileCoordinates { x: 0, y: 0 }, TileArea::origin);
        let streaming_dimensions: [u16; 2] = streaming_area.map_or([0, 0], TileArea::dimensions);
        let streaming_cell_count: u32 =
            u32::from(streaming_dimensions[0]) * u32::from(streaming_dimensions[1]) * 64;
        let values: [u32; 24] = [
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            delta_time.to_bits(),
            self.buffered_cell_count,
            self.gas_count,
            streaming_cell_count,
            streaming_origin.x as u32,
            streaming_origin.y as u32,
            u32::from(streaming_dimensions[0]),
            u32::from(streaming_dimensions[1]),
            VORTICITY_CONFINEMENT.to_bits(),
            BUOYANCY_COEFFICIENT.to_bits(),
            MAXIMUM_SPEED_CELLS_PER_SECOND.to_bits(),
            FLUID_OBSTACLE_COVERAGE.to_bits(),
            AMBIENT_DENSITY.to_bits(),
            self.ambient_temperature.to_bits(),
            0,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }
}

impl Drop for Gases {
    fn drop(&mut self) {
        self.velocity.free();
        self.velocity_scratch.free();
        self.concentrations.free();
        self.gas_temperature.free();
        self.concentration_scratch.free();
        self.divergence.free();
        self.pressure_a.free();
        self.pressure_b.free();
        self.curl.free();
        self.streaming_data.free();
        self.parameters.destroy();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        chunks::ChunkGasCell,
        materials::{MaterialForm, MaterialIdentifier},
        scenes::GasDownload,
    };
    use engine_graphics::{Color, MaterialAppearance};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn read_species(accelerator: &Accelerator, gases: &Gases, species: u32) -> Vec<f32> {
        let count = gases.buffered_cell_count as usize;
        let readback = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("gas concentration test readback"),
                size: count as u64 * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder = accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(
            gases.concentrations.wgpu_buffer(),
            u64::from(species) * count as u64 * 4,
            &readback,
            0,
            count as u64 * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let values = mapped
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        drop(mapped);
        readback.unmap();
        values
    }

    #[test]
    fn gas_obstacles_displace_without_destroying_inventory() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let vapor = materials.register(Material::Gas {
            name: "Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.65,
            diffusivity: 0.0,
            extinction: 0.0,
            dissipation: 0.0,
            compressibility: 0.0,
        });
        let graphics = materials.build_material_graphics(&accelerator);
        let width = 3u16;
        let height = 3u16;
        let count = usize::from(width) * usize::from(height) * 64;
        let cellular = accelerator.allocate::<u32>(count);
        let body = accelerator.allocate::<u32>(count);
        let fluid = accelerator.allocate::<f32>(count);
        let gases = Gases::new(
            &accelerator,
            &materials,
            &cellular,
            &body,
            &fluid,
            &graphics.gas_properties,
            count,
            293.15,
        );
        let physical = |x: u32, y: u32| -> usize {
            ((y / 8 * u32::from(width) + x / 8) * 64 + (y % 8) * 8 + x % 8) as usize
        };
        let center = physical(8, 8);
        let all_cells: Vec<usize> = (0..count).collect();
        gases.apply_edits(&accelerator, &[], &all_cells);
        gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &[]);
        let solid = MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32();
        accelerator.wgpu_queue().write_buffer(
            cellular.wgpu_buffer(),
            center as u64 * 4,
            &solid.to_le_bytes(),
        );
        gases.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
            [0.0, 0.0],
            1.0 / 60.0,
        );
        let displaced = read_species(&accelerator, &gases, 0);
        let total: f32 = displaced.iter().sum();
        assert!(displaced[center] < 0.01);
        assert!(displaced[physical(7, 8)] > 0.1 || displaced[physical(9, 8)] > 0.1);
        assert!((total - 1.0).abs() < 0.02);

        accelerator.wgpu_queue().write_buffer(
            cellular.wgpu_buffer(),
            center as u64 * 4,
            &0u32.to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            fluid.wgpu_buffer(),
            center as u64 * 4,
            &1.0f32.to_le_bytes(),
        );
        gases.apply_edits(&accelerator, &[], &all_cells);
        gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &[]);
        gases.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
            [0.0, 0.0],
            1.0 / 60.0,
        );
        let fluid_displaced = read_species(&accelerator, &gases, 0);
        assert!((fluid_displaced.iter().sum::<f32>() - total).abs() < 0.02);

        gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &all_cells);
        for step in 0..8u32 {
            let obstacle = physical(8 + step, 8);
            accelerator.wgpu_queue().write_buffer(
                fluid.wgpu_buffer(),
                obstacle as u64 * 4,
                &1.0f32.to_le_bytes(),
            );
            gases.simulate(
                &accelerator,
                TileCoordinates { x: 0, y: 0 },
                width,
                height,
                0,
                0,
                [0.0, 0.0],
                1.0 / 60.0,
            );
            accelerator.wgpu_queue().write_buffer(
                fluid.wgpu_buffer(),
                obstacle as u64 * 4,
                &0.0f32.to_le_bytes(),
            );
        }
        let moving = read_species(&accelerator, &gases, 0);
        assert!((moving.iter().sum::<f32>() - 1.0).abs() < 0.02);

        let neighbors = [
            physical(7, 8),
            physical(9, 8),
            physical(8, 7),
            physical(8, 9),
        ];
        gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &all_cells);
        for index in [center].into_iter().chain(neighbors) {
            accelerator.wgpu_queue().write_buffer(
                cellular.wgpu_buffer(),
                index as u64 * 4,
                &solid.to_le_bytes(),
            );
        }
        gases.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
            [0.0, 0.0],
            1.0 / 60.0,
        );
        let trapped = read_species(&accelerator, &gases, 0);
        assert!(trapped[center] > 0.95);
    }

    #[test]
    fn coexisting_species_remain_spread_inside_a_circular_enclosure_for_one_minute() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Accelerator = Accelerator::new().unwrap();
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let vapor: MaterialIdentifier = materials.register(Material::Gas {
            name: "Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.65,
            diffusivity: 0.8,
            extinction: 0.08,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        let tracer: MaterialIdentifier = materials.register(Material::Gas {
            name: "Tracer".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(190, 120, 140)),
            density: 0.65,
            diffusivity: 0.8,
            extinction: 0.2,
            dissipation: 0.0001,
            compressibility: 0.1,
        });
        let graphics = materials.build_material_graphics(&accelerator);
        let width: u16 = 4;
        let height: u16 = 4;
        let cell_count: usize = usize::from(width) * usize::from(height) * 64;
        let cellular: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
        let body: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
        let fluid: AcceleratorBuffer = accelerator.allocate::<f32>(cell_count);
        let gases: Gases = Gases::new(
            &accelerator,
            &materials,
            &cellular,
            &body,
            &fluid,
            &graphics.gas_properties,
            cell_count,
            293.15,
        );
        let physical_index = |x: u32, y: u32| -> usize {
            let tile_x: u32 = x / 8;
            let tile_y: u32 = y / 8;
            ((tile_y * u32::from(width) + tile_x) * 64 + (y % 8) * 8 + x % 8) as usize
        };
        let mut edits: Vec<(usize, u32, f32)> = Vec::new();
        for y in 5..11 {
            for x in 12..20 {
                edits.push((physical_index(x, y), vapor.index(), 400.0));
                edits.push((physical_index(x, y), tracer.index(), 300.0));
            }
        }
        gases.apply_edits(&accelerator, &edits, &[]);
        let solid: u32 = MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32();
        for y in 0..32 {
            for x in 0..32 {
                let offset_x: i32 = x as i32 - 16;
                let offset_y: i32 = y as i32 - 16;
                if offset_x * offset_x + offset_y * offset_y < 169 {
                    continue;
                }
                accelerator.wgpu_queue().write_buffer(
                    cellular.wgpu_buffer(),
                    physical_index(x, y) as u64 * 4,
                    &solid.to_le_bytes(),
                );
            }
        }
        for _ in 0..3600 {
            gases.simulate(
                &accelerator,
                TileCoordinates { x: 0, y: 0 },
                width,
                height,
                0,
                0,
                [0.0, -18.0],
                1.0 / 60.0,
            );
        }
        let area: TileArea = TileArea::new(TileCoordinates { x: 0, y: 0 }, width, height);
        let download: GasDownload =
            GasDownload::new(&accelerator, area, cell_count as u32, gases.gas_count());
        gases.export(
            &accelerator,
            &download,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
        );
        let byte_count: u64 = cell_count as u64 * u64::from(5 + gases.gas_count()) * 4;
        let (sender, receiver) = mpsc::sync_channel(1);
        download
            .buffer
            .slice(0..byte_count)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let started: Instant = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = download
            .buffer
            .slice(0..byte_count)
            .get_mapped_range()
            .unwrap();
        let bytes: Vec<u8> = mapped.to_vec();
        drop(mapped);
        download.buffer.unmap();
        let cells: Vec<ChunkGasCell> =
            GasDownload::deserialize(&bytes, area, &[vapor, tracer]).unwrap();
        assert!(!cells.is_empty());
        let total: f32 = cells
            .iter()
            .flat_map(|cell| cell.species.iter())
            .filter(|(identifier, _)| *identifier == vapor)
            .map(|(_, value)| value)
            .sum();
        let tracer_total: f32 = cells
            .iter()
            .flat_map(|cell| cell.species.iter())
            .filter(|(identifier, _)| *identifier == tracer)
            .map(|(_, value)| value)
            .sum();
        let maximum: f32 = cells
            .iter()
            .flat_map(|cell| cell.species.iter())
            .filter(|(identifier, _)| *identifier == vapor)
            .map(|(_, value)| *value)
            .fold(0.0, f32::max);
        let center_y: f32 = cells
            .iter()
            .map(|cell| {
                let concentration: f32 = cell
                    .species
                    .iter()
                    .filter(|(identifier, _)| *identifier == vapor)
                    .map(|(_, value)| value)
                    .sum();
                (cell.coordinates.y as f32 + 0.5) * concentration
            })
            .sum::<f32>()
            / total;
        let occupied_y: Vec<i32> = cells
            .iter()
            .filter_map(|cell| {
                cell.species
                    .iter()
                    .find(|(identifier, concentration)| {
                        *identifier == vapor && *concentration > 0.01
                    })
                    .map(|_| cell.coordinates.y)
            })
            .collect();
        assert!(total > 47.0);
        assert!(maximum < 1.0);
        assert!(center_y > 8.5);
        assert!(cells.iter().all(|cell| {
            let offset_x: i32 = cell.coordinates.x - 16;
            let offset_y: i32 = cell.coordinates.y - 16;
            offset_x * offset_x + offset_y * offset_y < 169
        }));
        assert!(occupied_y.iter().max().unwrap() - occupied_y.iter().min().unwrap() >= 6);
        assert!(
            cells
                .iter()
                .flat_map(|cell| &cell.species)
                .any(|(_, concentration)| (0.01..0.99).contains(concentration))
        );
        assert!(cells.iter().any(|cell| cell.species.len() == 2));
        assert!(cells.iter().any(|cell| cell.velocity[0] < -0.01));
        assert!(cells.iter().any(|cell| cell.velocity[0] > 0.01));
        assert!(tracer_total > total * 0.98);
        assert!(tracer_total < total);
        cellular.free();
        body.free();
        fluid.free();
    }

    #[test]
    #[ignore = "full-size GPU performance smoke"]
    fn full_demo_sized_gas_field_runs_sixty_ticks() {
        let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
        let accelerator: Accelerator = Accelerator::new().unwrap();
        eprintln!(
            "gas performance adapter: {:?}",
            accelerator.wgpu_adapter().get_info()
        );
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        materials.register(Material::Gas {
            name: "Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.65,
            diffusivity: 0.12,
            extinction: 0.08,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        materials.register(Material::Gas {
            name: "Smoke".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(68, 72, 76)),
            density: 0.85,
            diffusivity: 0.04,
            extinction: 2.5,
            dissipation: 0.0001,
            compressibility: 0.1,
        });
        let graphics = materials.build_material_graphics(&accelerator);
        let width: u16 = 72;
        let height: u16 = 51;
        let cell_count: usize = usize::from(width) * usize::from(height) * 64;
        let cellular: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
        let body: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
        let fluid: AcceleratorBuffer = accelerator.allocate::<f32>(cell_count);
        let gases: Gases = Gases::new(
            &accelerator,
            &materials,
            &cellular,
            &body,
            &fluid,
            &graphics.gas_properties,
            cell_count,
            293.15,
        );
        let started: Instant = Instant::now();
        for _ in 0..60 {
            gases.simulate(
                &accelerator,
                TileCoordinates { x: -12, y: -12 },
                width,
                height,
                0,
                0,
                [0.0, -18.0],
                1.0 / 60.0,
            );
        }
        accelerator
            .wgpu_device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let elapsed: Duration = started.elapsed();
        eprintln!("full demo-sized gas 60-tick GPU smoke: {elapsed:?}");
        assert!(elapsed < Duration::from_secs(30));
        cellular.free();
        body.free();
        fluid.free();
    }
}
