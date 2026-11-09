use crate::materials::{
    MaterialPropertiesChemical, MaterialPropertiesGas, MaterialPropertiesThermal, MaterialReactant,
    MaterialReaction, MaterialReactionProduct, MaterialRegistry,
};
use engine_graphics::world::materials::{MATERIAL_RADIANCE_WGSL, MaterialGraphicsGas};
use engine_host::acceleration::Accelerator;

/// Exercise the complete reaction solver, including product placement into other owned cells.
/// Only one block writes, so differences cannot be hidden by inter-block scheduling races.
#[test]
#[ignore = "requires GPU; compares complete chemistry outputs with paired cache enabled/disabled"]
fn paired_cache_matches_full_solver() {
    let a = Accelerator::default();
    let d = a.device();
    let mut registry = MaterialRegistry::new();
    for i in 0..140 {
        registry
            .register_gas(
                format!("g{i}"),
                MaterialPropertiesGas {
                    mass: 1.,
                    diffusivity: 0.1,
                    cohesion: 0.,
                    graphics: MaterialGraphicsGas {
                        color: 0xff808080,
                        radiance: 0,
                        opacity: 1.,
                        texture: 0.,
                        texture_scale: 1.,
                        drift: 0.,
                        flicker: 0.,
                    },
                    thermal: MaterialPropertiesThermal {
                        heat_capacity: 1.,
                        conduction: 0.3,
                        ..Default::default()
                    },
                    chemical: MaterialPropertiesChemical::default(),
                },
            )
            .unwrap();
    }
    for (from, to, rate) in [(0, 1, 1000.), (3, 0, 0.35), (5, 2, 50.), (130, 131, 1000.)] {
        registry.register_reaction(
            MaterialReaction::new(
                vec![MaterialReactant::material(format!("g{from}"), 0.03)],
                vec![MaterialReactionProduct::new(format!("g{to}"), 0.03)],
            )
            .with_rate(rate),
        );
    }
    for (first, second, product) in [(1, 2, 3), (131, 2, 3), (1, 131, 5)] {
        registry.register_reaction(
            MaterialReaction::new(
                vec![
                    MaterialReactant::material(format!("g{first}"), 0.02),
                    MaterialReactant::material(format!("g{second}"), 0.),
                ],
                vec![MaterialReactionProduct::new(format!("g{product}"), 0.02)],
            )
            .with_rate(1000.)
            .with_heat(2.)
            .with_pressure(0.01),
        );
    }
    let table = registry.compile(&a).unwrap();
    let source = [
        MATERIAL_RADIANCE_WGSL,
        include_str!("chemistry_cellular_tile.wgsl"),
        include_str!("../thermal/thermal_transition.wgsl"),
        include_str!("../thermal/thermal_conversion.wgsl"),
    ]
    .join("\n");
    let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("full chemistry cache proof"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let entries: Vec<_> = (0..25)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: if binding == 0 {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage {
                        read_only: matches!(binding,13..=18|20|22..=24),
                    }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        })
        .collect();
    let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });
    let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipelines: Vec<_> = [0., 1.]
        .into_iter()
        .map(|enabled| {
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("full chemistry cache comparison"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("react_tiles"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("CACHE_PAIR_UNITS", enabled)],
                    ..Default::default()
                },
                cache: None,
            })
        })
        .collect();
    let bytes = |words: &[u32]| {
        words
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let mutable = [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 19, 21];
    for case in 0u32..4 {
        let mut outputs = Vec::new();
        for pipeline in &pipelines {
            let buffers: Vec<_> = (0..25)
                .map(|binding| {
                    d.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: if binding == 0 { 96 } else { 16384 },
                        usage: wgpu::BufferUsages::COPY_DST
                            | wgpu::BufferUsages::COPY_SRC
                            | if binding == 0 {
                                wgpu::BufferUsages::UNIFORM
                            } else {
                                wgpu::BufferUsages::STORAGE
                            },
                        mapped_at_creation: false,
                    })
                })
                .collect();
            let mut matter = vec![0u32; 4096];
            let mut gas = vec![0u32; 4096];
            for cell in 0..512usize {
                matter[cell * 2] = (310. + ((cell as u32 * 17 + case * 31) % 130) as f32).to_bits();
                matter[cell * 2 + 1] = 65536;
                // A unary event completely consumes this seed, creating a paired reactant.
                let seed = if (cell as u32 + case) & 1 == 0 {
                    0u32
                } else {
                    130u32
                };
                gas[cell * 8] = (seed << 24) | 123;
                gas[cell * 8 + 1] = (2u32 << 24) | 4096;
                gas[cell * 8 + 2] = ((if cell % 3 == 0 { 3u32 } else { 139u32 }) << 24) | 1024;
                // Keep a slot free for real products rather than testing rejected placement.
                gas[cell * 8 + 4] = 4096;
            }
            a.queue().write_buffer(&buffers[2], 0, &bytes(&matter));
            a.queue().write_buffer(&buffers[4], 0, &bytes(&gas));
            a.queue().write_buffer(&buffers[12], 0, &bytes(&[4; 8]));
            a.queue()
                .write_buffer(&buffers[13], 0, &bytes(&[4, 5, 1, 0, 0, 0, 0, 0, 0]));
            let resources: Vec<_> = (0..25)
                .map(|binding| match binding {
                    14 => table.thermal_properties_buffer(),
                    15 => table.cellular_static_properties_buffer(),
                    16 => table.reaction_buffer(),
                    17 => table.reaction_color_buffer(),
                    18 => table.reaction_lookup_buffer(),
                    23 => table.fluid_properties_buffer(),
                    24 => table.gas_properties_buffer(),
                    _ => &buffers[binding],
                })
                .collect();
            let bindings: Vec<_> = resources
                .iter()
                .enumerate()
                .map(|(binding, buffer)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: buffer.as_entire_binding(),
                })
                .collect();
            let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &bindings,
            });
            for tick in 0u32..24 {
                let offsets = table.thermal_offsets();
                let mut words = vec![
                    4,
                    2,
                    if case & 1 != 0 { 3 } else { 0 },
                    if case & 1 != 0 { 1 } else { 0 },
                ];
                words.extend(offsets);
                words.extend([
                    table.thermal_properties_buffer().size() as u32 / 64,
                    case >> 1,
                    tick,
                    (1f32 / 60.).to_bits(),
                    4096f32.to_bits(),
                    64f32.to_bits(),
                    1024f32.to_bits(),
                    64f32.to_bits(),
                    (1f32 / 4096.).to_bits(),
                    7,
                    4,
                    4,
                    12,
                    2,
                    3,
                    4,
                ]);
                a.queue().write_buffer(&buffers[0], 0, &bytes(&words));
                let mut e = d.create_command_encoder(&Default::default());
                {
                    let mut pass = e.begin_compute_pass(&Default::default());
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(1, 1, 1);
                }
                a.queue().submit([e.finish()]);
            }
            let size = mutable.len() as u64 * 16384;
            let read = d.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut e = d.create_command_encoder(&Default::default());
            for (i, binding) in mutable.iter().enumerate() {
                e.copy_buffer_to_buffer(&buffers[*binding], 0, &read, i as u64 * 16384, 16384);
            }
            a.queue().submit([e.finish()]);
            read.slice(..)
                .map_async(wgpu::MapMode::Read, |result| result.unwrap());
            d.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let output = read.slice(..).get_mapped_range().unwrap().to_vec();
            read.unmap();
            outputs.push(output);
        }
        assert_eq!(
            outputs[0], outputs[1],
            "full chemistry state changed in case {case}"
        );
        // Prove the fixture exercised real events rather than an all-no-op comparison.
        let warmed = outputs[0][16384..2 * 16384]
            .chunks_exact(8)
            .take(512)
            .enumerate()
            .any(|(cell, word)| {
                u32::from_le_bytes(word[..4].try_into().unwrap())
                    != (310. + ((cell as u32 * 17 + case * 31) % 130) as f32).to_bits()
            });
        assert!(warmed, "fixture produced no reaction heat in case {case}");
    }
}
