use super::*;

const GREEN: &str = "fn custom_albedo(albedo: vec3f, uv: vec2f, world_position: vec3f) -> vec3f { return vec3f(0.0, 1.0, 0.0); }";

#[test]
fn helpers_cannot_hide_exponential_shader_work() {
    let mut source = "fn h0(v: f32) -> f32 { return sin(v); }\n".to_owned();
    for i in 1..7 {
        source.push_str(&format!(
            "fn h{i}(v:f32)->f32 {{ return h{p}(v)+h{p}(v)+h{p}(v)+h{p}(v); }}\n",
            p = i - 1
        ));
    }
    source.push_str("fn material_fragment(input:BgSurface)->BgSurface { var result=input; result.albedo.x=h6(input.uv.x); return result; }");
    assert!(
        shader::validate(&source)
            .unwrap_err()
            .contains("finite work")
    );
}

#[test]
fn rejects_foreign_resources_unbounded_work_and_invalid_layers_with_owner() {
    prepare("example:stone", GREEN.as_bytes(), 3, 20).unwrap();
    for invalid in [
        GREEN.replace("custom_albedo", "vs_main"),
        GREEN.replace("return vec3f", "loop {} return vec3f"),
        GREEN.replace("return vec3f", "discard; return vec3f"),
        format!("{GREEN}\n@group(0) @binding(0) var image: texture_2d<f32>;"),
        format!("{GREEN}\n@compute @workgroup_size(1) fn compute() {{}}"),
        GREEN.replace("vec3f(0.0", "array<vec3f, 1000000>(); vec3f(0.0"),
    ] {
        assert!(
            prepare("example:stone", invalid.as_bytes(), 3, 20)
                .unwrap_err()
                .contains("example:stone"),
            "{invalid}"
        );
    }
    assert!(
        prepare("example:stone", GREEN.as_bytes(), 20, 20)
            .unwrap_err()
            .contains("example:stone")
    );
    assert!(prepare("example:stone", &vec![b' '; MAX_SHADER_BYTES + 1], 3, 20).is_err());
}

#[test]
fn gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry() {
    gpu_preview(false);
}

#[test]
fn gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters() {
    gpu_preview(true);
}

fn gpu_preview(extended: bool) {
    use crate::render::{VERTEX_FLOATS, pipeline};
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        apply_limit_buckets: false,
        ..Default::default()
    }))
    .unwrap();
    let mut catalog = crate::content::Catalog::builtins();
    let layer = catalog
        .register_texture(crate::content::TextureDef {
            key: "jade:tile".into(),
            png: std::borrow::Cow::Borrowed(include_bytes!(
                "../../../fixtures/material-packages/jade/assets/textures/jade.png"
            )),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap()
        .get() as f32;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/material-packages");
    let bundle = crate::server::PackageSnapshot::discover(&root).unwrap();
    let mut shader = bundle
        .client_bundle()
        .material()
        .unwrap()
        .resolve(&catalog)
        .unwrap();
    if extended {
        shader.materials[0].version = 2;
        shader.materials[0].parameters =
            serde_json::from_str(r#"[{"name":"tint","kind":"color","default":[1,0,0,1]}]"#)
                .unwrap();
        shader.materials[0].textures.push(2);
        shader.materials[0].shader = r#"
fn tint() -> vec4f { return material_parameter(0u); }
fn material_vertex(input: BgVertex) -> BgVertex {
    var result = input;
    result.position.x += 0.02 * sin(material_time());
    return result;
}
fn material_fragment(input: BgSurface) -> BgSurface {
    var result = input;
    let detail = material_texture(input.uv, 1u);
    result.albedo = tint() + detail * 0.0;
    return result;
}"#
        .into();
        shader.materials[0].vertex_offset = 0.02;
        shader::validate(&shader.materials[0].shader).unwrap();
        shader.materials.push(Material {
            owner: "jade:dirt".into(),
            layers: vec![2],
            textures: vec![2],
            parameters: vec![],
            version: 2,
            vertex_offset: 0.0,
            shader: "fn material_fragment(input: BgSurface) -> BgSurface { return input; }".into(),
        });
    }
    let required_limits = crate::render::material_device_limits(
        adapter.limits(),
        crate::render::material_texture_layers(&catalog) as usize,
    )
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits,
        ..Default::default()
    }))
    .unwrap();
    let ((opaque, cutout, _, _, _), mut gpu) = pipeline::create_custom_voxel_pipeline(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &catalog,
        &shader,
    )
    .unwrap();
    if extended {
        assert!(
            gpu.set(
                &shader.materials[0].owner,
                "tint",
                &crate::render::parameters::Value::Vector(vec![0.0, 1.0, 0.0, 1.0])
            )
            .unwrap()
        );
    }
    gpu.update(&queue);
    // The live renderer retains its original groups when swapping pipelines.
    let (_, _, camera, camera_group, texture_group) = pipeline::create_voxel_pipeline_with_catalog(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &catalog,
    )
    .unwrap();
    queue.write_buffer(
        &camera,
        0,
        bytemuck::cast_slice(
            &crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
                .camera_data(glam::Mat4::IDENTITY, glam::Vec3::ZERO),
        ),
    );

    // Package-owned tile through the item/cutout pipeline in both sky and dark
    // light, and unselected opaque dirt. The production vertex layout is unchanged.
    // These XY quads face the eye along -Z; use their actual geometric normal.
    // An unrelated +Y normal makes this a grazing-Fresnel test instead of a
    // layer-isolation test once materials receive environment reflections.
    let mut vertices = Vec::<f32>::new();
    let mut indices = Vec::<u32>::new();
    for (x0, x1, y0, y1, layer, sky) in [
        (-1.0, 0.0, 0.0, 1.0, layer, 1.0),
        (-1.0, 0.0, -1.0, 0.0, layer, 0.0),
        (0.0, 1.0, -1.0, 1.0, 2.0, 1.0),
    ] {
        let base = (vertices.len() / VERTEX_FLOATS) as u32;
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            vertices.extend_from_slice(&[
                x, y, 0.5, 0.0, 0.0, -1.0, 0.25, 0.25, layer, sky, 0.0, 0.0, 0.0, 0.0, 0.0,
            ]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // Overlay two cutout flower patches in front of the lit custom stone. One
    // samples transparent alpha, the other opaque alpha; cutout must still
    // discard exactly as it did before customization.
    let pixels = crate::render::material::material_tiles_for(&catalog);
    let tile_size = crate::render::material::TEXTURE_SIZE as usize;
    let tile_bytes = tile_size * tile_size * 4;
    let leaf = &pixels[12 * tile_bytes..13 * tile_bytes];
    for patch in 0..2 {
        let texel = leaf
            .chunks_exact(4)
            .position(|p| if patch == 0 { p[3] == 0 } else { p[3] >= 128 })
            .unwrap();
        let uv = [
            ((texel % tile_size) as f32 + 0.5) / tile_size as f32,
            ((texel / tile_size) as f32 + 0.5) / tile_size as f32,
        ];
        let x0 = -1.0 + patch as f32 * 0.5;
        let x1 = x0 + 0.5;
        let base = (vertices.len() / VERTEX_FLOATS) as u32;
        for (x, y) in [(x0, 0.0), (x1, 0.0), (x1, 1.0), (x0, 1.0)] {
            vertices.extend_from_slice(&[
                x, y, 0.25, 0.0, 0.0, -1.0, uv[0], uv[1], 12.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 32;
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * u64::from(HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.create_view(&Default::default()),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&opaque);
        pass.set_bind_group(0, &camera_group, &[]);
        pass.set_bind_group(1, &texture_group, &[]);
        pass.set_bind_group(2, &gpu.group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(12..18, 0, 0..1);
        pass.set_pipeline(&cutout);
        pass.draw_indexed(0..12, 0, 0..1);
        pass.draw_indexed(18..30, 0, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let left = &bytes[(8 * 256 + 8 * 4)..][..4];
    let leaf = &bytes[(8 * 256 + 24 * 4)..][..4];
    let dark = &bytes[(24 * 256 + 16 * 4)..][..4];
    let right = &bytes[(16 * 256 + 48 * 4)..][..4];
    assert!(
        left[1] > 120 && left[0] < 10 && left[2] < 10,
        "selected custom layer: {left:?}"
    );
    assert!(
        leaf[0] > 0 && leaf[1] < left[1],
        "opaque cutout leaf must cover custom stone: {leaf:?}"
    );
    assert!(
        dark[1] > 0 && left[1] > dark[1] + 80,
        "light must remain renderer-owned: {left:?}, {dark:?}"
    );
    assert!(
        right[0] > right[1] && right[1] > right[2] && right[2] > 0,
        "default dirt layer: {right:?}"
    );
    if let Ok(path) = std::env::var("BLOXGLOOM_CUSTOM_PREVIEW") {
        let file = std::fs::File::create(path).unwrap();
        let mut png = png::Encoder::new(file, WIDTH, HEIGHT);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
    }
}

#[test]
fn gpu_custom_cutout_alpha_receives_matching_light_in_color_and_caster_passes() {
    let prepared = Prepared { environment_lighting: None, local_shadows: None, materials: vec![Material {
        owner: "test:lit_cutout".into(), layers: vec![2], textures: vec![2], parameters: vec![],
        version: 2, vertex_offset: 0.0,
        shader: "fn material_fragment(input: BgSurface) -> BgSurface { var result = input; result.albedo = vec4f(0.8,0.8,0.8,select(0.0,1.0,input.light.x > 0.7)); return result; }".into(),
    }] };
    shader::validate(&prepared.materials[0].shader).unwrap();
    crate::render::sun_shadow::gpu_tests::verify_custom_alpha(&prepared);
}
