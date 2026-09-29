use super::*;

struct PerfGpuMesh {
    opaque: Option<PerfGpuSubmesh>,
    cutout: Option<PerfGpuSubmesh>,
}

struct PerfGpuSubmesh {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    indices: u32,
}

#[derive(Clone, Copy)]
enum PerfPhase {
    Upload,
    Steady,
}

struct PerfSample {
    cpu_ms: f64,
    gpu_ms: Option<f64>,
    uploaded_chunks: usize,
    pending_chunks: usize,
    visible_chunks: usize,
    triangles: usize,
    phase: PerfPhase,
}

pub(super) async fn run_perf_benchmark_async(
    steady_frames: usize,
    radius: u8,
    bounced: bool,
) -> Result<(), Box<dyn Error>> {
    if steady_frames == 0 {
        return Err("steady frame count must be at least 1".into());
    }
    if !(1..=6).contains(&radius) {
        return Err("view radius must be in 1..=6".into());
    }

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await?;
    let adapter_info = adapter.get_info();
    let timestamp_supported = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let grid_width = u32::from(radius) * 2 + 1;
    let requested_chunks = grid_width * grid_width * 3;
    let max_frames = usize::try_from(requested_chunks)?
        .checked_add(steady_frames)
        .ok_or("frame count overflow")?;
    let max_queries = u32::try_from(max_frames.checked_mul(2).ok_or("query count overflow")?)?;
    if timestamp_supported && max_queries > wgpu::QUERY_SET_MAX_QUERIES {
        return Err(format!(
            "requested sample count needs {max_queries} GPU timestamp slots; adapter limit is {} (reduce steady frames or view radius)",
            wgpu::QUERY_SET_MAX_QUERIES
        )
        .into());
    }
    let requested_features = if timestamp_supported {
        wgpu::Features::TIMESTAMP_QUERY
    } else {
        wgpu::Features::empty()
    };
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_features: requested_features,
            ..Default::default()
        })
        .await?;

    let query_set = timestamp_supported.then(|| {
        device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("headless perf frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: max_queries,
        })
    });

    let center_surface = surface_height(0, 0);
    let center_y = world::world_to_chunk(0, center_surface + 1, 0).0.y;
    let generation_started = Instant::now();
    let mut precomputed_meshes = VecDeque::with_capacity(requested_chunks as usize);
    let mut chunk_order = Vec::with_capacity(requested_chunks as usize);
    let mut chunks = HashMap::with_capacity(requested_chunks as usize);
    let radius_i32 = i32::from(radius);
    // Match the server's near-first Manhattan ordering so the upload ramp starts
    // with the same chunk neighborhood clients receive first.
    for distance in 0..=radius_i32 * 2 + 1 {
        for dy in -1i32..=1 {
            for dz in -radius_i32..=radius_i32 {
                for dx in -radius_i32..=radius_i32 {
                    if dx.abs() + dy.abs() + dz.abs() != distance {
                        continue;
                    }
                    let key = ChunkKey {
                        x: dx,
                        y: center_y + dy,
                        z: dz,
                    };
                    chunk_order.push(key);
                    chunks.insert(key, Arc::new(world::generate_chunk(key, SEED)));
                }
            }
        }
    }
    for key in chunk_order {
        let light = LightField::build_with_bounce(key, &chunks, SEED, bounced);
        precomputed_meshes.push_back(render::mesh_chunk_lit(&chunks[&key], &light, 0));
    }
    let generation_ms = generation_started.elapsed().as_secs_f64() * 1_000.0;
    let nonempty_meshes = precomputed_meshes
        .iter()
        .filter(|mesh| !mesh.indices.is_empty() || !mesh.cutout_indices.is_empty())
        .count();
    let (mesh_bytes, vertex_count, index_count) = precomputed_meshes.iter().fold(
        (0usize, 0usize, 0usize),
        |(bytes, vertices, indices), mesh| {
            (
                bytes + mesh.byte_len(),
                vertices
                    + (mesh.vertices.len() + mesh.cutout_vertices.len()) / render::VERTEX_FLOATS,
                indices + mesh.indices.len() + mesh.cutout_indices.len(),
            )
        },
    );
    let cutout_vertex_count = precomputed_meshes
        .iter()
        .map(|mesh| mesh.cutout_vertices.len() / render::VERTEX_FLOATS)
        .sum::<usize>();
    let cutout_index_count = precomputed_meshes
        .iter()
        .map(|mesh| mesh.cutout_indices.len())
        .sum::<usize>();

    let post = render::post::PostProcess::new(&device, PERF_WIDTH, PERF_HEIGHT, FORMAT);
    let (sky_pipeline, sky_buffer, sky_group) =
        render::create_sky_pipeline(&device, render::post::HDR_FORMAT);
    let (pipeline, cutout_pipeline, camera_buffer, camera_group, texture_group) =
        render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT);
    let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
        render::create_target_pipeline(&device, FORMAT);
    let mut ui_renderer = ui::UiRenderer::new(&device, &queue, FORMAT);
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless perf color"),
        size: wgpu::Extent3d {
            width: PERF_WIDTH,
            height: PERF_HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless perf depth"),
        size: wgpu::Extent3d {
            width: PERF_WIDTH,
            height: PERF_HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());

    let target_x = 8;
    let target_z = 8;
    let target_block = [target_x, surface_height(target_x, target_z), target_z];
    let target_point = Vec3::new(
        target_x as f32 + 0.5,
        target_block[1] as f32 + 0.5,
        target_z as f32 + 0.5,
    );
    let camera_position = Vec3::new(0.5, center_surface as f32 + 18.0, -24.0);
    let direction = (target_point - camera_position).normalize();
    let camera = Camera {
        position: camera_position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70.0f32.to_radians(),
    };
    let matrix = render::view_projection(camera, PERF_WIDTH, PERF_HEIGHT);

    let mut precomputed_source = precomputed_meshes;
    let mut mesher_results = VecDeque::<ChunkMesh>::with_capacity(MESHER_RESULT_CAPACITY);
    let mut pending_upload = VecDeque::<ChunkMesh>::with_capacity(CLIENT_PENDING_UPLOADS);
    let mut pending_render = VecDeque::<ChunkMesh>::with_capacity(render::MAX_PENDING_MESHES);
    let mut gpu_meshes = HashMap::<ChunkKey, PerfGpuMesh>::with_capacity(requested_chunks as usize);
    let mut samples = Vec::with_capacity(max_frames);
    let mut total_uploaded = 0usize;
    let mut total_upload_bytes = 0usize;
    let mut max_upload_bytes_frame = 0usize;
    let mut final_triangles = 0usize;
    let mut final_visible = 0usize;
    let mut steady_done = 0usize;
    let mut last_submission = None;

    loop {
        let loading = !precomputed_source.is_empty()
            || !mesher_results.is_empty()
            || !pending_upload.is_empty()
            || !pending_render.is_empty();
        if !loading && steady_done >= steady_frames {
            break;
        }
        let phase = if loading {
            PerfPhase::Upload
        } else {
            PerfPhase::Steady
        };
        let start = Instant::now();

        // Fill the bounded worker-result channel with precomputed jobs, then mirror
        // ClientApp::poll_work: receive at most 64 meshes, retain at most 128 in
        // its retry queue, then fill Renderer up to its cap.
        while mesher_results.len() < MESHER_RESULT_CAPACITY {
            let Some(mesh) = precomputed_source.pop_front() else {
                break;
            };
            mesher_results.push_back(mesh);
        }
        let receive_room = CLIENT_PENDING_UPLOADS.saturating_sub(pending_upload.len());
        for _ in 0..receive_room.min(CLIENT_MESH_RESULT_BATCH) {
            let Some(mesh) = mesher_results.pop_front() else {
                break;
            };
            pending_upload.push_back(mesh);
        }
        while pending_render.len() < render::MAX_PENDING_MESHES {
            let Some(mesh) = pending_upload.pop_front() else {
                break;
            };
            pending_render.push_back(mesh);
        }

        let mut uploaded_chunks = 0usize;
        let mut uploaded_bytes = 0usize;
        while uploaded_chunks < render::UPLOAD_MESHES_PER_FRAME {
            let Some(mesh) = pending_render.front() else {
                break;
            };
            let mesh_bytes = mesh.byte_len();
            if uploaded_chunks > 0 && uploaded_bytes + mesh_bytes > render::UPLOAD_BYTES_PER_FRAME {
                break;
            }
            let mesh = pending_render.pop_front().unwrap();
            if mesh.indices.is_empty() && mesh.cutout_indices.is_empty() {
                gpu_meshes.remove(&mesh.key);
                continue;
            }
            let upload = |vertices: &[f32], indices: &[u32]| {
                if indices.is_empty() {
                    return None;
                }
                Some(PerfGpuSubmesh {
                    vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("perf chunk vertices"),
                        contents: bytemuck::cast_slice(vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("perf chunk indices"),
                        contents: bytemuck::cast_slice(indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    indices: indices.len() as u32,
                })
            };
            gpu_meshes.insert(
                mesh.key,
                PerfGpuMesh {
                    opaque: upload(&mesh.vertices, &mesh.indices),
                    cutout: upload(&mesh.cutout_vertices, &mesh.cutout_indices),
                },
            );
            uploaded_chunks += 1;
            uploaded_bytes += mesh_bytes;
        }

        let ui_frame = UiFrame {
            package_ui: None,
            join_address: None,
            join_progress: None,
            inventory_search: "",
            action_panel: None,
            container_screen: None,
            screen: UiScreen::Playing,
            selected_slot: 1,
            inventory: sample_inventory(),
            inventory_source: None,
            kiln: None,
            kiln_source: None,
            admin_enabled: false,
            admin_page: 0,
            admin_input: "",
            target: Some(target_block),
            status: None,
            debug: Some(ui::UiDebug {
                position: camera_position.to_array(),
                fps: 60.0,
                frame_ms: 16.667,
                visible_chunks: final_visible,
                cached_chunks: gpu_meshes.len(),
                latency_ms: Some(24),
            }),
            settings: UiSettings {
                view_distance: radius,
                ..UiSettings::default()
            },
            hovered: None,
        };
        ui_renderer.prepare(&queue, PERF_WIDTH, PERF_HEIGHT, &ui_frame);
        queue.write_buffer(
            &sky_buffer,
            0,
            bytemuck::cast_slice(&render::sky_camera_data(camera, PERF_WIDTH, PERF_HEIGHT)),
        );
        queue.write_buffer(
            &camera_buffer,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        queue.write_buffer(
            &target_camera_buffer,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        queue.write_buffer(
            &target_vertices,
            0,
            bytemuck::cast_slice(&render::target_outline_vertices(target_block)),
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless perf frame"),
        });
        let first_query = u32::try_from(samples.len() * 2)?;
        let frame_query_set = query_set.as_ref();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque chunks"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &post.scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(render::SKY_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: frame_query_set.map(|set| wgpu::RenderPassTimestampWrites {
                    query_set: set,
                    beginning_of_pass_write_index: Some(first_query),
                    end_of_pass_write_index: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&sky_pipeline);
            pass.set_bind_group(0, &sky_group, &[]);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &camera_group, &[]);
            pass.set_bind_group(1, &texture_group, &[]);
            final_visible = 0;
            final_triangles = 0;
            for (key, mesh) in &gpu_meshes {
                if !render::chunk_visible(matrix, *key) {
                    continue;
                }
                if let Some(opaque) = &mesh.opaque {
                    pass.set_vertex_buffer(0, opaque.vertex.slice(..));
                    pass.set_index_buffer(opaque.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..opaque.indices, 0, 0..1);
                    final_triangles += opaque.indices as usize / 3;
                }
                final_visible += 1;
            }
            pass.set_pipeline(&cutout_pipeline);
            for (key, mesh) in &gpu_meshes {
                if !render::chunk_visible(matrix, *key) {
                    continue;
                }
                if let Some(cutout) = &mesh.cutout {
                    pass.set_vertex_buffer(0, cutout.vertex.slice(..));
                    pass.set_index_buffer(cutout.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..cutout.indices, 0, 0..1);
                    final_triangles += cutout.indices as usize / 3;
                }
            }
        }
        post.encode(&device, &queue, &mut encoder, &color_view);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("target block outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&target_pipeline);
            pass.set_bind_group(0, &target_camera_group, &[]);
            pass.set_vertex_buffer(0, target_vertices.slice(..));
            pass.draw(0..24, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen-space playing HUD"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: frame_query_set.map(|set| wgpu::RenderPassTimestampWrites {
                    query_set: set,
                    beginning_of_pass_write_index: None,
                    end_of_pass_write_index: Some(first_query + 1),
                }),
                ..Default::default()
            });
            ui_renderer.encode(&mut pass);
        }
        last_submission = Some(queue.submit(Some(encoder.finish())));
        let cpu_ms = start.elapsed().as_secs_f64() * 1_000.0;
        total_uploaded += uploaded_chunks;
        total_upload_bytes += uploaded_bytes;
        max_upload_bytes_frame = max_upload_bytes_frame.max(uploaded_bytes);
        samples.push(PerfSample {
            cpu_ms,
            gpu_ms: None,
            uploaded_chunks,
            pending_chunks: pending_render.len() + pending_upload.len() + mesher_results.len(),
            visible_chunks: final_visible,
            triangles: final_triangles,
            phase,
        });
        if matches!(phase, PerfPhase::Steady) {
            steady_done += 1;
        }
    }

    let gpu_timestamp_period_ns = queue.get_timestamp_period();
    let gpu_timing = if let Some(query_set) = query_set {
        let query_count =
            u32::try_from(samples.len().checked_mul(2).ok_or("query count overflow")?)?;
        let buffer_size = u64::from(query_count) * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless perf timestamp resolve"),
            size: buffer_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless perf timestamp readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless perf timestamp resolve commands"),
        });
        encoder.resolve_query_set(&query_set, 0..query_count, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, buffer_size);
        let submission = queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission.clone()),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        receiver.recv()??;
        let mapped = readback.get_mapped_range(..)?;
        let timestamps = mapped
            .chunks_exact(8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        drop(mapped);
        readback.unmap();
        if gpu_timestamp_period_ns > 0.0 {
            for (index, sample) in samples.iter_mut().enumerate() {
                let start = timestamps[index * 2];
                let end = timestamps[index * 2 + 1];
                let elapsed_ns =
                    end.wrapping_sub(start) as f64 * f64::from(gpu_timestamp_period_ns);
                sample.gpu_ms = Some(elapsed_ns / 1_000_000.0);
            }
            Some(())
        } else {
            None
        }
    } else {
        if let Some(submission) = last_submission {
            device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })?;
        }
        None
    };

    let (upload_samples, steady_samples): (Vec<_>, Vec<_>) = samples
        .iter()
        .partition(|sample| matches!(sample.phase, PerfPhase::Upload));
    let cpu_all = samples
        .iter()
        .map(|sample| sample.cpu_ms)
        .collect::<Vec<_>>();
    print_percentiles("CPU frame", &cpu_all);
    print_percentiles(
        "CPU upload-ramp frame",
        &upload_samples
            .iter()
            .map(|sample| sample.cpu_ms)
            .collect::<Vec<_>>(),
    );
    print_percentiles(
        "CPU steady frame",
        &steady_samples
            .iter()
            .map(|sample| sample.cpu_ms)
            .collect::<Vec<_>>(),
    );
    if gpu_timing.is_some() {
        print_percentiles(
            "GPU render passes",
            &samples
                .iter()
                .filter_map(|sample| sample.gpu_ms)
                .collect::<Vec<_>>(),
        );
        print_percentiles(
            "GPU upload-ramp passes",
            &upload_samples
                .iter()
                .filter_map(|sample| sample.gpu_ms)
                .collect::<Vec<_>>(),
        );
        print_percentiles(
            "GPU steady passes",
            &steady_samples
                .iter()
                .filter_map(|sample| sample.gpu_ms)
                .collect::<Vec<_>>(),
        );
    }

    let upload_frame_count = upload_samples.len();
    let peak_pending_chunks = samples
        .iter()
        .map(|sample| sample.pending_chunks)
        .max()
        .unwrap_or_default();
    let max_uploaded_frame = samples
        .iter()
        .map(|sample| sample.uploaded_chunks)
        .max()
        .unwrap_or_default();
    let max_visible = samples
        .iter()
        .map(|sample| sample.visible_chunks)
        .max()
        .unwrap_or_default();
    let max_triangles = samples
        .iter()
        .map(|sample| sample.triangles)
        .max()
        .unwrap_or_default();
    eprintln!(
        "headless scene: seed=0x{SEED:016x}, radius={radius}, requested={requested_chunks}, nonempty={nonempty_meshes}, gpu-resident={} ({} uploaded), final-visible={final_visible} / max-visible={max_visible}, final-triangles={final_triangles} / max-triangles={max_triangles}, mesh-bytes={mesh_bytes}, upload-bytes={total_upload_bytes}, max-upload/frame={max_uploaded_frame} chunks / {max_upload_bytes_frame} bytes (budget {} bytes), peak-queued={peak_pending_chunks}",
        gpu_meshes.len(),
        total_uploaded,
        render::UPLOAD_BYTES_PER_FRAME,
    );
    eprintln!(
        "adapter: {} ({:?}, {:?}), driver={} {}, timestamps={}",
        adapter_info.name,
        adapter_info.backend,
        adapter_info.device_type,
        adapter_info.driver,
        adapter_info.driver_info,
        if gpu_timing.is_some() {
            format!("supported, period={gpu_timestamp_period_ns:.3} ns/tick")
        } else if timestamp_supported {
            "reported but unusable (zero timestamp period)".to_string()
        } else {
            "unsupported; GPU percentiles omitted".to_string()
        },
    );
    eprintln!(
        "scene setup: {} mesh vertices ({} cutout), {} indices ({} cutout), generated in {:.1} ms (excluded from frame samples)",
        vertex_count, cutout_vertex_count, index_count, cutout_index_count, generation_ms
    );
    eprintln!(
        "measurement: offscreen {}x{}, {} upload-ramp + {} steady frames, no vsync; CPU is submit-side work (staging + GPU buffer creation + UI preparation + encode + queue submit), excludes GPU completion and present; GPU timestamps cover world pass including cutout foliage through final HUD pass end, excluding CPU staging and buffer-upload copies; samples do not wait per frame, one final GPU wait is used for timestamp readback",
        PERF_WIDTH, PERF_HEIGHT, upload_frame_count, steady_done,
    );
    Ok(())
}

fn print_percentiles(label: &str, values: &[f64]) {
    if values.is_empty() {
        eprintln!("{label}: no samples");
        return;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: usize| sorted[(sorted.len() - 1) * p / 100];
    eprintln!(
        "{label} (n={}): p50={:.3} ms, p95={:.3} ms, p99={:.3} ms",
        sorted.len(),
        percentile(50),
        percentile(95),
        percentile(99),
    );
}
