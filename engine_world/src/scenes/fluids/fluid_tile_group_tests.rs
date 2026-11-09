use engine_host::acceleration::Accelerator;

fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap();
    let brace = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, c) in source[brace..].char_indices() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return source[start..brace + offset + 1].to_owned();
            }
        }
    }
    panic!("unterminated function {name}")
}

fn struct_definition(source: &str, name: &str) -> String {
    let start = source.find(&format!("struct {name} {{")).unwrap();
    let end = start + source[start..].find("};").unwrap() + 2;
    source[start..end].to_owned()
}

fn const_definition(source: &str, name: &str) -> String {
    let start = source.find(&format!("const {name}:")).unwrap();
    let end = start + source[start..].find(';').unwrap() + 1;
    source[start..end].to_owned()
}

fn extracted_shader(original: &str) -> String {
    let mut source = String::from(
        r#"
struct Params {
    region_width: u32,
    region_height: u32,
    cell_width: u32,
    cell_height: u32,
    rebase_x: f32,
    rebase_y: f32,
    erasure_count: u32,
    actor_count: u32,
};
@group(0) @binding(0) var<uniform> uniforms: Params;
@group(0) @binding(1) var<storage, read_write> control: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> boundary_tile_data: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read> boundary_node_map: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> erasure_masks: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read> fluid_actors: array<FluidActor>;
override TILE_WORKGROUP_SIZE: u32 = 1u;
"#,
    );
    source.push_str(&struct_definition(original, "FluidActor"));
    for name in [
        "SLEEP_TILE_STRIDE",
        "SLEEP_TILE_SLEEPING",
        "SLEEP_TILE_HASH_VALID",
        "SLEEP_TILE_HAS_PARTICLE",
        "SLEEP_TILE_WAKE_THIS_TICK",
        "SLEEP_TILE_WAKE_PENDING",
        "SLEEP_TILE_SOLID_CHANGED",
        "SLEEP_REDUCTION_EMPTY",
        "SLEEP_TICK_COUNT",
        "SLEEP_SPEED",
        "SLEEP_DENSITY_ERROR",
        "WAKE_SPEED",
        "BOUNDARY_TILE_PARTICLE",
    ] {
        source.push_str(&const_definition(original, name));
        source.push('\n');
    }
    for name in [
        "sleep_state_offset",
        "sleep_tile_offset",
        "wake_count_offset",
        "tile_is_sleeping",
        "wake_tile",
        "load_boundary_node",
    ] {
        source.push_str(&function(original, name));
        source.push('\n');
    }
    for name in [
        "prepare_sleep",
        "wake_actor_neighbors",
        "wake_solid_neighbors",
        "apply_pending_wakes",
        "finish_sleep",
    ] {
        source.push_str("@compute @workgroup_size(TILE_WORKGROUP_SIZE)\n");
        source.push_str(&function(original, name));
        source.push('\n');
    }
    source
}

const SLEEPING: u32 = 1;
const HASH_VALID: u32 = 2;
const HAS_PARTICLE: u32 = 4;
const WAKE_PENDING: u32 = 16;

fn buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    words: &[u32],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let data: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: data.len() as u64,
        usage,
        mapped_at_creation: false,
    });
    if !data.is_empty() {
        queue.write_buffer(&buffer, 0, &data);
    }
    buffer
}

fn initial_case(
    width: u32,
    height: u32,
    case: u32,
) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let tiles = (width * height) as usize;
    let cell_width = width * 8;
    let cell_height = height * 8;
    let cells = (cell_width * cell_height) as usize;
    let control_len = 4 + cells + tiles * 7 + 1;
    let state_offset = 4 + cells;
    let empty_hash = (0..16).fold(2_166_136_261u32, |hash, _| hash.wrapping_mul(16_777_619));
    let mut control = vec![0u32; control_len];
    let mut boundary = vec![0u32; tiles * 18];
    let node_count = ((cell_width + 1) * (cell_height + 1)) as usize;
    let mut nodes = vec![0u32; node_count * 4];
    let mut erasures = vec![0u32; tiles * 2];

    // All tiles begin as calm, sleeping fluid so every kernel has a stable baseline.
    for tile in 0..tiles {
        let base = state_offset + tile * 7;
        control[base] = 0.001f32.to_bits();
        control[base + 1] = 0.001f32.to_bits();
        control[base + 2] = 29;
        control[base + 3] = empty_hash;
        control[base + 4] = SLEEPING | HASH_VALID | HAS_PARTICLE;
        control[base + 5] = 0.001f32.to_bits();
        control[base + 6] = HAS_PARTICLE | SLEEPING;
        boundary[tile] = 1; // BOUNDARY_TILE_PARTICLE
    }

    let center = ((height / 2) * width + width / 2) as usize;
    let center_base = state_offset + center * 7;
    let mut actor = vec![0u32; 16];
    let center_x = (width / 2 * 8 + 4) as f32;
    let center_y = (height / 2 * 8 + 4) as f32;
    actor[..8].copy_from_slice(&[
        center_x.to_bits(),
        center_y.to_bits(),
        0.2f32.to_bits(),
        0.0f32.to_bits(),
        1.0f32.to_bits(),
        0.0f32.to_bits(),
        2.0f32.to_bits(),
        2.0f32.to_bits(),
    ]);
    actor[8..12].copy_from_slice(&[
        (center_x - 4.0).to_bits(),
        (center_y - 4.0).to_bits(),
        (center_x + 4.0).to_bits(),
        (center_y + 4.0).to_bits(),
    ]);
    actor[12] = 0; // circle
    actor[13] = 1.0f32.to_bits();

    match case {
        0 => {} // stable sleeping state, including odd partial workgroups
        1 => {} // pending wake is injected after the prediction-side phase below
        2 => {
            // A changed solid hash wakes this tile and its particle neighbors.
            boundary[2 * tiles + center * 16] = 0x0101_0101;
            control[center_base + 3] = 0xfeed_beef;
        }
        3 => {} // overlapping actor wakes the center tile
        4 => {
            // Nonzero boundary velocity at a node of the center tile.
            let node_x = width / 2 * 8 + 4;
            let node_y = height / 2 * 8 + 4;
            let node = (node_y * (cell_width + 1) + node_x) as usize;
            nodes[node * 4 + 3] = 0.25f32.to_bits();
        }
        _ => {
            // Exercise rebase, erasure masks, a fast neighbor snapshot, and edge tiles.
            control[center_base + 5] = 0.5f32.to_bits();
            erasures[center * 2] = 1;
            let neighbor_base = state_offset + 7;
            control[neighbor_base + 5] = 0.5f32.to_bits();
            control[neighbor_base + 6] = HAS_PARTICLE;
        }
    }
    (
        control,
        boundary,
        nodes,
        erasures,
        actor,
        vec![
            width,
            height,
            cell_width,
            cell_height,
            if case == 5 {
                1.0f32.to_bits()
            } else {
                0.0f32.to_bits()
            },
            0.0f32.to_bits(),
            u32::from(case == 5),
            u32::from(case == 3),
        ],
    )
}

#[test]
#[ignore = "requires GPU; compares exact extracted fluid tile kernels at workgroup sizes 1 and 64"]
fn tile_workgroup_grouping_preserves_control_state() {
    let accelerator = Accelerator::default();
    let device = accelerator.device();
    let original = include_str!("fluid_particle.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("exact fluid tile group comparison"),
        source: wgpu::ShaderSource::Wgsl(extracted_shader(original).into()),
    });
    let names = [
        "prepare_sleep",
        "wake_actor_neighbors",
        "wake_solid_neighbors",
        "apply_pending_wakes",
        "finish_sleep",
    ];
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fluid tile comparison bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
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
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fluid tile comparison pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipelines: Vec<Vec<_>> = [1u32, 64]
        .into_iter()
        .map(|group_size| {
            names
                .iter()
                .map(|name| {
                    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(*name),
                        layout: Some(&pipeline_layout),
                        module: &shader,
                        entry_point: Some(*name),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &[("TILE_WORKGROUP_SIZE", group_size as f64)],
                            ..Default::default()
                        },
                        cache: None,
                    })
                })
                .collect()
        })
        .collect();
    for (width, height) in [(3u32, 5u32), (17u32, 19u32)] {
        for case in 0..6u32 {
            let mut outputs = Vec::new();
            for group_index in 0..2 {
                let group_size = if group_index == 0 { 1 } else { 64 };
                let (control_data, boundary_data, node_data, erasure_data, actor_data, params) =
                    initial_case(width, height, case);
                let control = buffer(
                    device,
                    accelerator.queue(),
                    "tile control",
                    &control_data,
                    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                );
                let boundary = buffer(
                    device,
                    accelerator.queue(),
                    "boundary tile data",
                    &boundary_data,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                let nodes = buffer(
                    device,
                    accelerator.queue(),
                    "boundary nodes",
                    &node_data,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                let erasures = buffer(
                    device,
                    accelerator.queue(),
                    "erasure masks",
                    &erasure_data,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                let actors = buffer(
                    device,
                    accelerator.queue(),
                    "fluid actors",
                    &actor_data,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                );
                let uniform = buffer(
                    device,
                    accelerator.queue(),
                    "tile params",
                    &params,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                );
                let buffers = [&uniform, &control, &boundary, &nodes, &erasures, &actors];

                let entry_points = &pipelines[group_index];
                let bind_groups: Vec<_> = entry_points
                    .iter()
                    .map(|_pipeline| {
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("fluid tile state"),
                            layout: &bind_group_layout,
                            entries: &buffers
                                .iter()
                                .enumerate()
                                .map(|(binding, buffer)| wgpu::BindGroupEntry {
                                    binding: binding as u32,
                                    resource: buffer.as_entire_binding(),
                                })
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                let dispatch = (width * height).div_ceil(group_size);
                let mut encoder = device.create_command_encoder(&Default::default());
                for phase in 0..3 {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&entry_points[phase]);
                    pass.set_bind_group(0, &bind_groups[phase], &[]);
                    pass.dispatch_workgroups(dispatch, 1, 1);
                }
                accelerator.queue().submit([encoder.finish()]);

                if case == 1 {
                    // Model the prediction pass adding one pending wake between these phases.
                    let pending_tile = (height * width - 1) as usize;
                    let flags_word = 4 + (width * 8 * height * 8) as usize + pending_tile * 7 + 4;
                    accelerator.queue().write_buffer(
                        &control,
                        (flags_word * 4) as u64,
                        &(SLEEPING | HASH_VALID | HAS_PARTICLE | WAKE_PENDING).to_le_bytes(),
                    );
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                for phase in 3..5 {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&entry_points[phase]);
                    pass.set_bind_group(0, &bind_groups[phase], &[]);
                    pass.dispatch_workgroups(dispatch, 1, 1);
                }
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("tile control readback"),
                    size: control.size(),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_buffer_to_buffer(&control, 0, &readback, 0, control.size());
                accelerator.queue().submit([encoder.finish()]);
                let (sender, receiver) = std::sync::mpsc::channel();
                readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        sender.send(result).unwrap();
                    });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                receiver.recv().unwrap().unwrap();
                let mapped = readback.slice(..).get_mapped_range().unwrap();
                outputs.push(mapped.to_vec());
                drop(mapped);
                readback.unmap();
            }
            assert_eq!(outputs[0], outputs[1], "tile={width}x{height} case={case}");
        }
    }
}

fn boundary_dilation_shader(original: &str) -> String {
    let mut source = String::from(
        r#"
struct Params {
    region_width: u32,
    region_height: u32,
    padding_0: u32,
    padding_1: u32,
};
@group(0) @binding(0) var<uniform> uniforms: Params;
@group(0) @binding(1) var<storage, read_write> control: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> boundary_tile_data: array<atomic<u32>>;
"#,
    );
    for name in ["BOUNDARY_TILE_PARTICLE", "BOUNDARY_TILE_MARGIN"] {
        source.push_str(&const_definition(original, name));
        source.push('\n');
    }
    source.push_str(&function(original, "list_boundary_tile"));
    source.push('\n');
    source.push_str("@compute @workgroup_size(64)\n");
    source.push_str(&function(original, "dilate_boundary_tiles"));
    source
}

fn run_boundary_dilation(
    accelerator: &Accelerator,
    pipeline: &wgpu::ComputePipeline,
    width: u32,
    height: u32,
    particle_tiles: &[u32],
    workgroups: u32,
) -> (Vec<u32>, Vec<u32>, u32) {
    let device = accelerator.device();
    let tile_count = (width * height) as usize;
    let mut control_words = vec![0u32; 4];
    control_words[2] = particle_tiles.len() as u32;
    let mut boundary_words = vec![0u32; tile_count * 2];
    for (list_index, &tile) in particle_tiles.iter().enumerate() {
        boundary_words[tile as usize] = 1;
        boundary_words[tile_count + list_index] = tile;
    }
    let params = [width, height, 0, 0];
    let control = buffer(
        device,
        accelerator.queue(),
        "dilation control",
        &control_words,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
    );
    let boundary = buffer(
        device,
        accelerator.queue(),
        "dilation boundary tiles",
        &boundary_words,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
    );
    let uniform = buffer(
        device,
        accelerator.queue(),
        "dilation params",
        &params,
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("boundary dilation state"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: control.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: boundary.as_entire_binding(),
            },
        ],
    });
    let readback_size = control.size() + boundary.size();
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("boundary dilation readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&control, 0, &readback, 0, control.size());
    encoder.copy_buffer_to_buffer(&boundary, 0, &readback, control.size(), boundary.size());
    accelerator.queue().submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let words: Vec<u32> = mapped
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    drop(mapped);
    readback.unmap();
    let boundary_start = control_words.len();
    let returned_control = &words[..boundary_start];
    let returned_boundary = &words[boundary_start..];
    let listed_count = returned_control[2];
    (
        returned_boundary[..tile_count].to_vec(),
        returned_boundary[tile_count..tile_count + listed_count as usize].to_vec(),
        listed_count,
    )
}

fn expected_boundary_flags(width: u32, height: u32, particles: &[u32]) -> Vec<u32> {
    let mut expected = vec![0u32; (width * height) as usize];
    for &tile in particles {
        expected[tile as usize] = 1;
    }
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            if expected[index] != 0 {
                continue;
            }
            let near_particle = particles.iter().any(|&particle| {
                let particle_x = particle % width;
                let particle_y = particle / width;
                (x as i32 - particle_x as i32).abs() <= 2
                    && (y as i32 - particle_y as i32).abs() <= 2
            });
            if near_particle {
                expected[index] = 2;
            }
        }
    }
    expected
}

#[test]
#[ignore = "requires GPU; verifies full clipped boundary-tile dilation coverage"]
fn boundary_dilation_dispatch_covers_every_tile() {
    let accelerator = Accelerator::default();
    let device = accelerator.device();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("exact boundary dilation coverage"),
        source: wgpu::ShaderSource::Wgsl(
            boundary_dilation_shader(include_str!("fluid_particle.wgsl")).into(),
        ),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("boundary dilation bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
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
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("boundary dilation layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("boundary dilation"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("dilate_boundary_tiles"),
        compilation_options: Default::default(),
        cache: None,
    });

    for (width, height, particles) in [
        (3u32, 5u32, vec![0u32, 13, 14]),
        (17u32, 19u32, vec![0u32, 16, 161, 306, 322]),
    ] {
        let expected = expected_boundary_flags(width, height, &particles);
        let correct = run_boundary_dilation(
            &accelerator,
            &pipeline,
            width,
            height,
            &particles,
            (width * height).div_ceil(64),
        );
        assert_eq!(correct.0, expected, "flags for region {width}x{height}");
        assert_eq!(
            correct.2 as usize,
            expected.iter().filter(|&&flag| flag != 0).count()
        );
        let mut listed = correct.1.clone();
        listed.sort_unstable();
        assert_eq!(
            listed,
            (0..expected.len() as u32)
                .filter(|&i| expected[i as usize] != 0)
                .collect::<Vec<_>>()
        );

        if width == 17 {
            let legacy = run_boundary_dilation(
                &accelerator,
                &pipeline,
                width,
                height,
                &particles,
                (width * height).div_ceil(128),
            );
            assert_ne!(
                legacy.0, expected,
                "legacy 128-based dispatch must miss upper-region margins"
            );
        }
    }
}
