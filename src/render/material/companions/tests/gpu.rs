//! Execute production WGSL math, including mirrored faces and darkness gates.
use wgpu::util::DeviceExt;
const CASES: u32 = 34;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    if id.x < 8u {
        var axis = id.x / 2u;
        var side = select(1.0, -1.0, id.x % 2u == 1u);
        if id.x == 6u { axis = 1u; side = 1.0; }
        if id.x == 7u { axis = 0u; side = 1.0; }
        var n = vec3f(side, 0.0, 0.0);
        var t = vec3f(0.0, 0.0, 1.0);
        var b = vec3f(0.0, -1.0, 0.0);
        if axis == 1u { n = vec3f(0.0, side, 0.0); b = vec3f(1.0, 0.0, 0.0); }
        if axis == 2u { n = vec3f(0.0, 0.0, side); t = vec3f(1.0, 0.0, 0.0); }
        var ux = vec2f(1.0, 0.0);
        var uy = vec2f(0.0, 1.0);
        if id.x == 6u { ux.x = -1.0; }
        if id.x == 7u { ux = vec2f(0.0, 1.0); uy = vec2f(1.0, 0.0); }
        result[id.x] = vec4f(bg_normal_frame(n, t, b, ux, uy,
            normalize(vec3f(0.3, 0.4, 0.8660254))), 1.0);
    } else if id.x < 12u {
        let sky = select(1.0, 0.0, id.x == 8u);
        let visibility = select(1.0, 0.0, id.x == 9u);
        let enabled = select(1.0, 0.0, id.x == 10u);
        result[id.x] = vec4f(bg_specular_light(vec3f(0.0, 1.0, 0.0),
            vec3f(0.0, 1.0, 0.0), vec4f(0.0, 1.0, 0.0, 1.0), sky, visibility,
            vec3f(0.5), vec4f(0.8, 0.0, 0.0, enabled), vec3f(0.72,0.67,0.56)), 1.0);
    } else if id.x < 20u {
        let up = vec3f(0.0, 1.0, 0.0);
        var plane = vec3f(0.0, 0.0, 1.0);
        var view = vec3f(0.0, 0.0, 1.0);
        var transmission = 0.5;
        var sky = 1.0;
        if id.x == 13u { plane = -plane; }
        if id.x == 14u { view = -view; }
        if id.x == 15u { transmission = 0.0; }
        if id.x == 16u { sky = 0.0; }
        if id.x == 17u { plane = up; }
        if id.x == 18u { plane = vec3f(0.0); }
        if id.x == 19u { transmission = 1.0; }
        let thin = bg_thin_transmission_normal(up, up, plane, view);
        result[id.x] = vec4f(bg_thin_direct(up, thin,
            vec4f(normalize(vec3f(0.0, 0.6, -0.8)), 1.0), sky, 0.35, transmission, vec3f(0.72,0.67,0.56)), 1.0);
    } else if id.x < 28u {
        let up = vec3f(0.0,1.0,0.0);
        var specular = vec4f(0.8,1.0,0.0,1.0);
        var sky = 1.0;
        var local_visibility = 1.0;
        var local = vec3f(0.0);
        if id.x == 21u { sky = 0.0; }
        if id.x == 22u { local_visibility = 0.0; }
        if id.x == 23u { specular.a = 0.0; }
        if id.x == 24u { specular.r = 0.0; }
        if id.x == 25u { sky = 0.0; local = vec3f(1.0,0.2,0.0); }
        if id.x == 26u { specular.g = 0.0; }
        if id.x == 27u { sky = 0.0; local = vec3f(1.0); local_visibility = 0.0; }
        result[id.x] = vec4f(bg_environment_specular(up,up,vec3f(0.5),specular,
            vec3f(1.0),sky*local_visibility,local,local_visibility),
            bg_material_diffuse_weight(specular,1.0));
    } else if id.x < 32u {
        let daylight = select(0.0,1.0,id.x % 2u == 1u);
        let glow = select(0.0,1.0,id.x >= 30u);
        result[id.x] = vec4f(bg_local_material_radiance(glow*glow*vec3f(1.0,0.57,0.23),vec3f(0.0),vec3f(0.0),daylight),1.0);
    } else {
        let up = vec3f(0.0,1.0,0.0);
        let px = select(vec3f(1.0,0.0,0.0),vec3f(0.0),id.x==33u);
        result[id.x] = vec4f(bg_normal_frame(up,px,up,vec2f(1.0,0.0),vec2f(0.0,1.0),
            normalize(vec3f(0.3,0.4,0.8660254))),1.0);
    }
}
"#;

fn source() -> String {
    format!("{}\n{FIXTURE}", include_str!("../../relief.wgsl"))
}

#[test]
fn relief_math_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_normal_frames_and_specular_preserve_mirrors_caves_shadows_and_missing_maps() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("material relief GPU test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("material relief regression"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; CASES as usize * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(CASES * 16),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(CASES, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, u64::from(CASES * 16));
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert_eq!(
        rows[32],
        [0.0, 1.0, 0.0, 1.0],
        "crossed cards retain upward shading without a collapsed frame"
    );
    assert_eq!(
        rows[33],
        [0.0, 1.0, 0.0, 1.0],
        "degenerate geometry retains its finite shading normal"
    );
    use glam::Vec3;
    for (i, row) in rows[..8].iter().enumerate() {
        let axis = if i == 6 {
            1
        } else if i == 7 {
            0
        } else {
            i / 2
        };
        let side = if i < 6 && i % 2 == 1 { -1.0 } else { 1.0 };
        let mut n = Vec3::ZERO;
        n[axis] = side;
        let mut t = if axis == 2 { Vec3::X } else { Vec3::Z };
        let mut b = if axis == 1 { Vec3::X } else { -Vec3::Y };
        if i == 6 {
            t = -t;
        }
        if i == 7 {
            std::mem::swap(&mut t, &mut b);
        }
        let expected = (t * 0.3 + b * 0.4 + n * 0.8660254).normalize();
        let actual = Vec3::new(row[0], row[1], row[2]);
        assert!(
            actual.distance(expected) < 1e-5,
            "normal frame {i}: {actual:?} vs {expected:?}"
        );
    }
    assert!(rows.iter().flatten().all(|v| v.is_finite()));
    assert_eq!(&rows[28][..3], &[0.0; 3]);
    assert_eq!(
        rows[28], rows[29],
        "sunlit/curved normals cannot become local radiance"
    );
    assert_eq!(
        rows[30], rows[31],
        "local glow stays independent of daylight"
    );
    assert_eq!(&rows[30][..3], &[1.0, 0.57, 0.23]);
    for index in [21, 22, 23, 27] {
        assert_eq!(
            &rows[index][..3],
            &[0.0; 3],
            "environment darkness gate {index}"
        );
    }
    assert!(
        rows[20][0] > rows[24][0],
        "roughness broadens and reduces the lobe"
    );
    assert!(
        rows[20][0] > rows[26][0],
        "metal uses base-colored reflection"
    );
    assert_eq!(rows[20][3], 0.0, "metal has no doubled diffuse energy");
    assert_eq!(rows[23][3], 1.0, "missing map keeps legacy diffuse");
    assert!((rows[26][3] - 0.96).abs() < 1e-6);
    assert!(rows[25][0] > rows[25][1] && rows[25][1] > 0.0);
    assert_eq!(rows[25][2], 0.0, "local warm glow has no injected blue sky");
    for row in &rows[8..11] {
        assert_eq!(&row[..3], &[0.0; 3]);
    }
    assert!(rows[11][..3].iter().all(|v| *v > 0.0));
    // Daytime upward diffuse normal still receives a backlit card transmission
    // lobe. Reversed winding is identical; looking from the lit side removes it.
    assert!(rows[12][0] > rows[15][0] + 0.1);
    assert_eq!(rows[12], rows[13]);
    assert_eq!(rows[14], rows[15]);
    assert_eq!(&rows[16][..3], &[0.0; 3]);
    assert_eq!(
        rows[17], rows[15],
        "cube leaves keep their surface response"
    );
    assert_eq!(rows[18], rows[15], "degenerate derivatives remain finite");
    for (actual, bound) in rows[19][..3].iter().zip([0.72, 0.67, 0.56]) {
        assert!(
            *actual <= bound + 0.00001,
            "direct energy must stay bounded"
        );
    }
}

#[test]
fn explicit_local_transport_validates_with_custom_vertex_normals() {
    let source = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        crate::render::custom::TYPES,
        include_str!("../../relief.wgsl"),
        include_str!("../../pbr.wgsl"),
        include_str!("../../parallax.wgsl"),
        include_str!("../../companions.wgsl"),
        r#"fn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {
            var output = input;
            output.normal = normalize(vec3f(input.position.x, 0.5, 1.0));
            return output;
        }
        fn bg_surface(input: BgSurface, layer: u32) -> BgSurface { return input; }"#,
        include_str!("../../../pipeline.wgsl"),
    );
    let source = crate::render::daylight::shader(&source);
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
