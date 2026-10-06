//! Fixed-query hardware timing, separate from correctness and full-scene frame cost.
use super::*;
#[test]
#[ignore = "explicit medium lookup GPU microbenchmark"]
fn gpu_indexed_medium_fixed_77_tile_query_cost() {
    let mut coarse = Vec::new();
    for x in 0..77 {
        coarse.push(Arc::new(scene::Chunk {
            coarse_water: Some(scene::water::CoarseTile {
                key: lod::TileKey { level: 1, x, z: 0 },
                columns: vec![
                    scene::water::CoarseColumn {
                        coverage: vec![lod::Interval {
                            bottom: -8,
                            top: 32
                        }],
                        water: vec![lod::Interval { bottom: 0, top: 17 }],
                    };
                    lod::TILE_COLUMNS
                ],
            }),
            ..Default::default()
        }));
    }
    let mut scene = scene::Scene::build([]);
    scene::volume::append(&mut scene, &[], &coarse);
    let queries = (0..256)
        .map(|i| {
            let direction =
                glam::Vec3::new(1.0, 0.0, if i % 2 == 0 { 0.05 } else { -0.01 }).normalize();
            [
                69.0 * 64.0 + 0.5 + (i % 16) as f32 * 0.03125,
                8.0,
                8.0,
                512.0,
                direction.x,
                direction.y,
                direction.z,
                0.0,
            ]
        })
        .collect::<Vec<_>>();
    compare(&scene, queries.clone());
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let (layout, group) = bindings(&device, &scene, &queries);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("old/indexed medium timing"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipelines = ["fs_old", "fs_new"].map(|entry| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    });
    let output = device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let timestamps = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: None,
        ty: wgpu::QueryType::Timestamp,
        count: 32,
    });
    // Separate command buffers bound each invocation batch; never one long fragment loop.
    for batch in 0..18u32 {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let writes = (batch >= 2).then(|| wgpu::RenderPassTimestampWrites {
                query_set: &timestamps,
                beginning_of_pass_write_index: Some((batch - 2) * 2),
                end_of_pass_write_index: Some((batch - 2) * 2 + 1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("256 fixed medium queries"),
                timestamp_writes: writes,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipelines[batch as usize % 2]);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.resolve_query_set(&timestamps, 0..32, &resolve, 0);
    encoder.copy_buffer_to_buffer(&resolve, 0, &read, 0, 256);
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = read.get_mapped_range(..).unwrap();
    let values: &[u64] = bytemuck::cast_slice(&bytes);
    let mut samples = [Vec::new(), Vec::new()];
    for (batch, pair) in values.chunks_exact(2).enumerate() {
        samples[batch % 2].push(
            pair[1].wrapping_sub(pair[0]) as f64 * f64::from(queue.get_timestamp_period()) / 1e6,
        );
    }
    for values in &mut samples {
        values.sort_by(f64::total_cmp);
        assert!(values[0] > 0.0);
    }
    let old = samples[0][4];
    let indexed = samples[1][4];
    println!(
        "77-tile fixed256-query GPU median: old={old:.6}ms indexed-known-only={indexed:.6}ms ratio={:.3}; old samples={:?}; indexed samples={:?}",
        old / indexed,
        samples[0],
        samples[1]
    );
}
