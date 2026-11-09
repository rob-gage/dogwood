use engine_host::acceleration::Accelerator;

#[test]
#[ignore = "requires GPU; compares complete pressure substep and refinement outputs with exact no-op shortcuts"]
fn pressure_noop_shortcuts_preserve_results() {
    let a = Accelerator::default();
    let d = a.device();
    let entries: Vec<_> = (0..23)
        .map(|i| wgpu::BindGroupLayoutEntry {
            binding: i,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: if i == 0 {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage {
                        read_only: [7, 8, 9, 10, 11, 12, 21, 22].contains(&i),
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
    let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    for gas in [false, true] {
        let pipelines: Vec<_> = [false, true]
            .into_iter()
            .map(|enabled| {
                let tiles = 2usize;
                let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("pressure derived cache regression"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("pressure_cellular_tile.wgsl").into(),
                    ),
                });
                let pipeline = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&pl),
                    module: &module,
                    entry_point: Some(if gas { "step_gas" } else { "step" }),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("PRESSURE_SKIP_NOOPS", if enabled { 1.0 } else { 0.0 })],
                        zero_initialize_workgroup_memory: false,
                        ..Default::default()
                    },
                    cache: None,
                });
                let refine = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&pl),
                    module: &module,
                    entry_point: Some("refine"),
                    compilation_options: Default::default(),
                    cache: None,
                });
                (pipeline, refine, tiles)
            })
            .collect();
        for (width, height) in [(4u32, 4u32), (5, 3)] {
            let tile_count = (width * height) as usize;
            for ring in [false, true] {
                for solid in [false, true] {
                    for resting in [false, true] {
                        let mut data = vec![vec![0u32; 8192]; 23];
                        data[0] = vec![
                            width,
                            height,
                            u32::from(ring),
                            u32::from(ring) * 2,
                            4096,
                            1f32.to_bits(),
                            0,
                            4096f32.to_bits(),
                            0,
                            (1f32 / 3.0).to_bits(),
                            0.05f32.to_bits(),
                            0.9f32.to_bits(),
                            0.01f32.to_bits(),
                            0.001f32.to_bits(),
                            0.01f32.to_bits(),
                            3f32.to_bits(),
                            0.2f32.to_bits(),
                            0.1f32.to_bits(),
                            0.1f32.to_bits(),
                            0.01f32.to_bits(),
                            0.01f32.to_bits(),
                            0,
                            0,
                            1f32.to_bits(),
                            0.1f32.to_bits(),
                            0.01f32.to_bits(),
                            273f32.to_bits(),
                            1f32.to_bits(),
                            0,
                            (1f32 / 273.0).to_bits(),
                            0,
                            0,
                        ];
                        for i in 0..tile_count {
                            data[4][i] = if i % 5 == 3 { 0 } else { 4 };
                        }
                        data[0][21] = 1f32.to_bits();
                        for i in 0..tile_count * 64 {
                            data[1][i] = if solid && i % 5 == 0 { 1 } else { 0 };
                            data[20][i * 2 + 1] = 65536;
                        }
                        data[22][2] = 0.01f32.to_bits();
                        data[22][3] = 1f32.to_bits();
                        data[6][0] = tile_count as u32;
                        for i in 0..tile_count * 16 {
                            let is_solid = solid && i % 7 == 0;
                            if gas {
                                data[2][i * 8] = (i as u32 % 3) << 24 | (100 + i as u32 % 200);
                            }
                            data[2][i * 8 + 4] = 4096 + i as u32 % 400;
                            data[2][i * 8 + 5] = if resting {
                                0f32.to_bits()
                            } else {
                                (0.01 * (i as f32 % 11.0 - 5.0)).to_bits()
                            };
                            data[2][i * 8 + 6] = if resting {
                                (-0f32).to_bits()
                            } else {
                                (0.01 * (i as f32 % 7.0 - 3.0)).to_bits()
                            };
                            data[2][i * 8 + 7] = if is_solid {
                                (i as i32 % 17 - 8) as u32
                            } else {
                                (270.0 + i as f32 % 9.0).to_bits()
                            };
                            let base = 4 + tile_count * 3 + i * 4;
                            data[6][base] = if is_solid { 0 } else { 65536 };
                            data[6][base + 2] = u32::from(is_solid);
                            data[6][base + 3] = 1f32.to_bits();
                        }
                        for i in 0..4096 {
                            data[7][i] = match i % 8 {
                                0 => 1f32.to_bits(),
                                1 => 0.01f32.to_bits(),
                                2 => 0.0001f32.to_bits(),
                                _ => 0,
                            };
                            data[11][i] = 1f32.to_bits();
                            data[21][i] = 1f32.to_bits();
                        }
                        let mut results = Vec::new();
                        for (pipeline, refine, tiles) in &pipelines {
                            let mut data = data.clone();
                            let blocks: Vec<u32> = (0..height)
                                .step_by(*tiles as usize)
                                .flat_map(|y| {
                                    (0..width)
                                        .step_by(*tiles as usize)
                                        .map(move |x| y * width + x)
                                })
                                .collect();
                            data[6][3] = blocks.len() as u32;
                            data[6][4 + tile_count..4 + tile_count + blocks.len()]
                                .copy_from_slice(&blocks);
                            data[6][4 + tile_count * 2..4 + tile_count * 2 + blocks.len()]
                                .copy_from_slice(&blocks);
                            let buffers: Vec<_> = data
                                .iter()
                                .enumerate()
                                .map(|(i, words)| {
                                    let b = d.create_buffer(&wgpu::BufferDescriptor {
                                        label: None,
                                        size: words.len() as u64 * 4,
                                        usage: if i == 0 {
                                            wgpu::BufferUsages::UNIFORM
                                                | wgpu::BufferUsages::COPY_DST
                                        } else {
                                            wgpu::BufferUsages::STORAGE
                                                | wgpu::BufferUsages::COPY_DST
                                                | wgpu::BufferUsages::COPY_SRC
                                        },
                                        mapped_at_creation: false,
                                    });
                                    a.queue().write_buffer(
                                        &b,
                                        0,
                                        &words
                                            .iter()
                                            .flat_map(|x| x.to_le_bytes())
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
                                layout: &layout,
                                entries: &entries,
                            });
                            let read = d.create_buffer(&wgpu::BufferDescriptor {
                                label: None,
                                size: 8192 * 4 * 4,
                                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                                mapped_at_creation: false,
                            });
                            let mut e = d.create_command_encoder(&Default::default());
                            {
                                let mut p = e.begin_compute_pass(&Default::default());
                                p.set_pipeline(pipeline);
                                p.set_bind_group(0, &group, &[]);
                                p.dispatch_workgroups(blocks.len() as u32, 1, 1);
                            }
                            let refine_entries: Vec<_> = buffers
                                .iter()
                                .enumerate()
                                .map(|(i, b)| wgpu::BindGroupEntry {
                                    binding: i as u32,
                                    resource: if i == 2 {
                                        buffers[3].as_entire_binding()
                                    } else if i == 3 {
                                        buffers[2].as_entire_binding()
                                    } else {
                                        b.as_entire_binding()
                                    },
                                })
                                .collect();
                            let refine_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: None,
                                layout: &layout,
                                entries: &refine_entries,
                            });
                            {
                                let mut p = e.begin_compute_pass(&Default::default());
                                p.set_pipeline(refine);
                                p.set_bind_group(0, &refine_group, &[]);
                                p.dispatch_workgroups(blocks.len() as u32, 1, 1);
                            }
                            for (i, binding) in [3, 4, 2, 20].into_iter().enumerate() {
                                e.copy_buffer_to_buffer(
                                    &buffers[binding],
                                    0,
                                    &read,
                                    i as u64 * 8192 * 4,
                                    8192 * 4,
                                );
                            }
                            a.queue().submit([e.finish()]);
                            let (tx, rx) = std::sync::mpsc::channel();
                            read.slice(..)
                                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
                            d.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                            rx.recv().unwrap().unwrap();
                            let mapped = read.slice(..).get_mapped_range().unwrap();
                            results.push(mapped.to_vec());
                            drop(mapped);
                            read.unmap();
                        }
                        for (variant, result) in results.iter().enumerate().skip(1) {
                            assert_eq!(
                                &results[0], result,
                                "gas={gas}, ring={ring}, solid={solid}, variant={variant}"
                            );
                        }
                    }
                }
            }
        }
    }
}
