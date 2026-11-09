use engine_host::acceleration::Accelerator;

fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap();
    let brace = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, c) in source[brace..].char_indices() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                return source[start..brace + offset + 1].to_owned();
            }
        }
    }
    panic!("unterminated function")
}

#[test]
#[ignore = "requires GPU; compares actual actor contact kernel with material filtering disabled"]
fn actor_material_filter_preserves_contacts() {
    let a = Accelerator::default();
    let d = a.device();
    let original = include_str!("fluid_particle.wgsl");
    let actor_start = original.find("struct FluidActor {").unwrap();
    let actor_end = actor_start + original[actor_start..].find("};").unwrap() + 2;
    let mut source = String::from(
        r#"
struct Params { actor_count:u32, fluid_material_count:u32, cell_width:u32, cell_height:u32, actor_time:f32, raster_scale:f32, actor_contact_scale:f32 }
@group(0) @binding(0) var<uniform> uniforms:Params;
@group(0) @binding(1) var<storage,read_write> fluid_raster:array<atomic<u32>>;
@group(0) @binding(2) var<storage,read> material_identifiers:array<u32>;
@group(0) @binding(3) var<storage,read> rigid_body_cells:array<u32>;
@group(0) @binding(4) var<storage,read> fluid_actors:array<FluidActor>;
@group(0) @binding(5) var<storage,read_write> fluid_actor_contacts:array<atomic<i32>>;
@group(0) @binding(6) var<storage,read> fluid_materials:array<vec4<f32>>;
override FILTER_ACTOR_MATERIALS:bool=true;
const PARTICLE_WORKGROUP_SIZE:u32=64u;
const FLUID_ACTOR_SAMPLE_MARGIN:f32=2.5;
const FLUID_TICK_TIME_STEP:f32=1.0/60.0;
fn cell_inside(c:vec2<i32>)->bool { return all(c>=vec2<i32>(0)) && all(c<vec2<i32>(i32(uniforms.cell_width),i32(uniforms.cell_height))); }
fn cell_index(c:vec2<i32>)->u32 { return u32(c.y)*uniforms.cell_width+u32(c.x); }
fn fluid_contact_offset()->u32 { return uniforms.cell_width*uniforms.cell_height*4u; }
fn add_fluid_contact_tile(index:u32) { atomicOr(&fluid_raster[uniforms.cell_width*uniforms.cell_height*6u+index],1u); }
"#,
    );
    source.push_str(&original[actor_start..actor_end]);
    for name in [
        "actor_covers_cell",
        "actor_covers_sample_cell",
        "actor_sample_fixed",
    ] {
        source.push_str(&function(original, name));
    }
    source.push_str("@compute @workgroup_size(64)\n");
    source.push_str(&function(original, "actor_contacts"));
    let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actor material exact comparison"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipelines: Vec<_> = [0.0, 1.0]
        .into_iter()
        .map(|enabled| {
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &module,
                entry_point: Some("actor_contacts"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("FILTER_ACTOR_MATERIALS", enabled)],
                    ..Default::default()
                },
                cache: None,
            })
        })
        .collect();
    for materials in [1u32, 8, 130] {
        for pattern in 0..6u32 {
            let cells = 32usize * 32;
            let mut raster = vec![0u32; cells * 6 + cells/64];
            let mut solid = vec![0u32; cells];
            let mut rigid = solid.clone();
            for i in 0..cells {
                let mass = match pattern {
                    0 => 0,
                    1 => 1024,
                    2 => {
                        if i % 3 == 0 {
                            0
                        } else {
                            1024
                        }
                    }
                    3 => ((i * 7919) % 2048) as u32,
                    4 => {
                        if i % 2 == 0 {
                            1024
                        } else {
                            u32::MAX
                        }
                    }
                    _ => 512,
                };
                raster[i * 4] = mass;
                raster[i * 4 + 1] = (i as i32 * 13 - 4096) as u32;
                raster[i * 4 + 2] = (4096 - i as i32 * 17) as u32;
                raster[i * 4 + 3] = if pattern == 5 {
                    if i % 2 == 0 { 1 } else { 129 }
                } else {
                    (i as u32 % materials) | 0x40000000
                };
                solid[i] = u32::from(i % 31 == 0);
                rigid[i] = u32::from(i % 37 == 0);
            }
            let mut actors = Vec::<u32>::new();
            for shape in 0..3u32 {
                actors.extend([
                    16f32.to_bits(),
                    16f32.to_bits(),
                    0.25f32.to_bits(),
                    (-0.5f32).to_bits(),
                    0.8f32.to_bits(),
                    0.6f32.to_bits(),
                    4f32.to_bits(),
                    2f32.to_bits(),
                    8f32.to_bits(),
                    8f32.to_bits(),
                    24f32.to_bits(),
                    24f32.to_bits(),
                    shape,
                    1f32.to_bits(),
                    pattern % 2,
                    0,
                ]);
            }
            let mut properties = vec![0u32; materials as usize * 8];
            for i in 0..materials as usize {
                properties[i * 8] = (1.0 + i as f32 * 0.1).to_bits();
                properties[i * 8 + 4] = 0.7f32.to_bits();
            }
            let params = vec![
                3,
                materials,
                32,
                32,
                0.5f32.to_bits(),
                1024f32.to_bits(),
                65536f32.to_bits(),
                0,
            ];
            let mut outputs = Vec::new();
            for pipeline in &pipelines {
                let data = [
                    params.clone(),
                    raster.clone(),
                    solid.clone(),
                    rigid.clone(),
                    actors.clone(),
                    vec![0; 24],
                    properties.clone(),
                ];
                let buffers: Vec<_> = data
                    .iter()
                    .enumerate()
                    .map(|(i, words)| {
                        let b = d.create_buffer(&wgpu::BufferDescriptor {
                            label: None,
                            size: words.len() as u64 * 4,
                            usage: (if i == 0 {
                                wgpu::BufferUsages::UNIFORM
                            } else {
                                wgpu::BufferUsages::STORAGE
                            }) | wgpu::BufferUsages::COPY_DST
                                | wgpu::BufferUsages::COPY_SRC,
                            mapped_at_creation: false,
                        });
                        a.queue().write_buffer(
                            &b,
                            0,
                            &words
                                .iter()
                                .flat_map(|v| v.to_le_bytes())
                                .collect::<Vec<_>>(),
                        );
                        b
                    })
                    .collect();
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &entries,
                });
                let size = buffers[1].size() + buffers[5].size();
                let read = d.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut e = d.create_command_encoder(&Default::default());
                {
                    let mut p = e.begin_compute_pass(&Default::default());
                    p.set_pipeline(pipeline);
                    p.set_bind_group(0, &group, &[]);
                    p.dispatch_workgroups(1, 1, 1);
                }
                e.copy_buffer_to_buffer(&buffers[1], 0, &read, 0, buffers[1].size());
                e.copy_buffer_to_buffer(
                    &buffers[5],
                    0,
                    &read,
                    buffers[1].size(),
                    buffers[5].size(),
                );
                a.queue().submit([e.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                read.slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
                d.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = read.slice(..).get_mapped_range().unwrap();
                outputs.push(mapped.to_vec());
                drop(mapped);
                read.unmap();
            }
            assert_eq!(
                outputs[0], outputs[1],
                "materials={materials} pattern={pattern}"
            );
        }
    }
}
