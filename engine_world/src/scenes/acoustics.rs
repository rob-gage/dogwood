// Copyright Rob Gage 2026

//! Bounded, asynchronous coarse simulation observations for game-defined acoustic palettes.
use super::{
    ScenePosition,
    actor_sensing::{form_index_counts, tag_masks},
    cellular_tiles::{CellularTileBuffers, CellularTileRegion},
};
use crate::materials::MaterialTable;
use engine_host::acceleration::{Accelerator, AcceleratorReadback};

#[derive(Clone, Debug)]
pub struct AcousticBucket {
    pub position: ScenePosition,
    /// Moving fluid, burning gas, acid contact, moving hot fluid, pressure, moving granular matter.
    pub activity: [f32; 6],
}
struct Metadata {
    origin: ScenePosition,
    step: f64,
    width: u32,
    height: u32,
}
pub(crate) struct Acoustics {
    pipeline: wgpu::ComputePipeline,
    uniforms: wgpu::Buffer,
    masks: wgpu::Buffer,
    results: wgpu::Buffer,
    reads: AcceleratorReadback<Metadata>,
}
impl Acoustics {
    pub fn new(a: &Accelerator, table: &MaterialTable, tags: &[String; 4]) -> Self {
        let d = a.device();
        let buffer = |label, size, usage| {
            d.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("acoustics"),
            source: wgpu::ShaderSource::Wgsl(include_str!("acoustics.wgsl").into()),
        });
        let pipeline = engine_host::acceleration::create_compute_pipeline(d, &wgpu::ComputePipelineDescriptor {
            label: Some("acoustics"),
            layout: None,
            module: &shader,
            entry_point: Some("observe"),
            compilation_options: Default::default(),
            cache: None,
        });
        let mask_words = tag_masks(table, tags);
        let masks = buffer(
            "acoustic masks",
            (mask_words.len().max(1) * 4) as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        a.queue().write_buffer(
            &masks,
            0,
            &mask_words
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        Self {
            pipeline,
            uniforms: buffer(
                "acoustic params",
                64,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            masks,
            results: buffer(
                "acoustic results",
                256 * 24,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            ),
            reads: AcceleratorReadback::new(a, "acoustic readback", 256 * 24, 3),
        }
    }
    pub fn sample(
        &mut self,
        a: &Accelerator,
        table: &MaterialTable,
        b: &CellularTileBuffers,
        fluid: &wgpu::Buffer,
        region: CellularTileRegion,
        ring: (u32, u32),
        center: ScenePosition,
        half: [f32; 2],
    ) -> Vec<AcousticBucket> {
        let mut out = Vec::new();
        // Poll only: a busy GPU never blocks the simulation for sound.
        if let Err(e) = self.reads.read(a, false, |meta, bytes| {
            out.clear();
            for y in 0..meta.height {
                for x in 0..meta.width {
                    let base = ((y * meta.width + x) * 24) as usize;
                    let activity = std::array::from_fn(|k| {
                        f32::from_le_bytes(
                            bytes[base + k * 4..base + k * 4 + 4].try_into().unwrap(),
                        )
                    });
                    out.push(AcousticBucket {
                        position: meta
                            .origin
                            .translate((x as f64 + 0.5) * meta.step, (y as f64 + 0.5) * meta.step),
                        activity,
                    });
                }
            }
        }) {
            eprintln!("acoustic readback skipped: {e}");
        }
        let origin = ScenePosition {
            tile_x: region.bottom_left.x,
            tile_y: region.bottom_left.y,
            ..Default::default()
        };
        let (cx, cy) = center.relative_position(origin);
        let min = [
            ((cx - half[0] as f64 - 4.0) * 8.0).max(0.0) as u32,
            ((cy - half[1] as f64 - 4.0) * 8.0).max(0.0) as u32,
        ];
        let max = [
            ((cx + half[0] as f64 + 4.0) * 8.0)
                .max(0.0)
                .min((region.width() * 8) as f64) as u32,
            ((cy + half[1] as f64 + 4.0) * 8.0)
                .max(0.0)
                .min((region.height() * 8) as f64) as u32,
        ];
        if max[0] <= min[0] || max[1] <= min[1] {
            return out;
        }
        let span = (max[0] - min[0]).max(max[1] - min[1]);
        let step = span.div_ceil(16).div_ceil(8).max(8) * 8;
        let width = (max[0] - min[0]).div_ceil(step);
        let height = (max[1] - min[1]).div_ceil(step);
        let sample_stride = ((((max[0] - min[0]) as f64 * (max[1] - min[1]) as f64) / 300_000.0)
            .sqrt()
            .ceil() as u32)
            .clamp(1, 16);
        let counts = form_index_counts(table);
        let words = [
            region.width() as u32,
            region.height() as u32,
            ring.0,
            ring.1,
            min[0],
            min[1],
            max[0],
            max[1],
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            step,
            width,
            height,
            sample_stride,
        ];
        a.queue().write_buffer(
            &self.uniforms,
            0,
            &words
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let buffers = [
            &self.uniforms,
            b.material_identifier_buffer(),
            b.kinematics_buffer(),
            fluid,
            b.gas_buffer(),
            b.pressure_buffer(),
            &self.masks,
            &self.results,
        ];
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = a.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("acoustics"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut encoder = a
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("acoustics"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("acoustics"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(width, height, 1);
        }
        self.reads.copy(
            &mut encoder,
            &self.results,
            0,
            (width * height * 24) as u64,
            Metadata {
                origin: origin.translate(min[0] as f64 / 8.0, min[1] as f64 / 8.0),
                step: step as f64 / 8.0,
                width,
                height,
            },
        );
        let submission = a.queue().submit([encoder.finish()]);
        self.reads.map(submission);
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn acoustic_shader_validates() {
        let module = naga::front::wgsl::parse_str(include_str!("acoustics.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        ).validate(&module).unwrap();
    }
}
