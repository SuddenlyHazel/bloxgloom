use super::*;

#[test]
fn default_emission_uses_modern_source_classes_and_explicit_state_proxies() {
    let catalog = Catalog::builtins();
    for (key, expected) in [
        ("glowstone", 151),
        ("jg_magma", 151),
        ("jg_crying_obsidian", 150),
        ("jg_crimson_stem", 150),
        ("jg_crimson_stem_top", 150),
        ("jg_warped_stem", 150),
        ("jg_warped_stem_top", 150),
        ("jg_glow_lichen", 150),
        ("jg_sea_lantern", 151),
        ("jg_shroomlight", 151),
        ("jg_amethyst_cluster", 150),
        ("jg_small_amethyst_bud", 150),
        ("jg_cave_vines_head_berries", 157),
        ("jg_cave_vines", 0),
        ("jg_stripped_crimson_stem", 0),
        ("jg_stripped_crimson_stem_top", 0),
        ("jg_stripped_warped_stem", 0),
        ("jg_torchflower", 0),
        ("jg_diamond_ore", 0),
        ("jg_amethyst_block", 0),
        ("stone", 0),
    ] {
        let texture = catalog
            .textures()
            .iter()
            .find(|t| t.key.as_ref() == format!("bloxgloom:{key}"))
            .unwrap();
        assert_eq!(texture_class(texture), expected, "{key}");
    }
    if let Some(settings) = crate::render::bsl_reference::audit_source("lib/settings.glsl") {
        assert!(
            settings
                .lines()
                .any(|l| l.trim().starts_with("#define EMISSIVE_HARDCODED 0 //"))
        );
        assert!(
            settings
                .lines()
                .any(|l| l.trim() == "//#define GLOWING_ORES")
        );
    }
}

// Extract the actual consumer assignments, retaining their default-only guard.
// Unrelated lighting is omitted; advanced input must remain the old catalog/PBR
// expression and an untyped LOD face must never inherit layer-zero emission.
fn consumers() -> String {
    let near = include_str!("../../pipeline.wgsl")
        .split("var emission=max(material_emission[u32(input.layer)],specular.emission);")
        .nth(1)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap();
    let lod = include_str!("../../lod/shader.wgsl")
        .split("emission=material_emission[encoded_layer-1u];")
        .nth(1)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap();
    let mut source = String::from(
        "struct Input{layer:f32};struct Surface{albedo:vec4f};struct Specular{emission:f32};\n",
    );
    for (mode, default) in [("default", true), ("advanced", false)] {
        writeln!(source,"fn near_{mode}(layer:u32,albedo:vec3f)->f32{{let input=Input(f32(layer));let surface=Surface(vec4f(albedo,1.0));let specular=Specular(0.8);var emission=max(material_emission[u32(input.layer)],specular.emission);{} return emission;}}",near.replace("BG_BSL_REFERENCE",&default.to_string())).unwrap();
        writeln!(source,"fn lod_{mode}(encoded_layer:u32,source_albedo:vec3f)->f32{{var emission=0.0;if encoded_layer!=0u{{emission=material_emission[encoded_layer-1u];{}}}return emission;}}",lod.replace("BG_BSL_REFERENCE",&default.to_string())).unwrap();
    }
    source
}

#[test]
fn gpu_actual_near_and_lod_default_emission_match_encoded_color_and_preserve_advanced() {
    use wgpu::util::DeviceExt;
    let catalog = Catalog::builtins();
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for key in [
        "glowstone",
        "jg_crimson_stem_top",
        "jg_stripped_crimson_stem",
        "jg_cave_vines_head_berries",
        "jg_cave_vines",
        "jg_diamond_ore",
    ] {
        let layer = catalog
            .textures()
            .iter()
            .position(|t| t.key.as_ref() == format!("bloxgloom:{key}"))
            .unwrap() as u32;
        for encoded in [
            [0.0f32; 3],
            [0.5; 3],
            [1.0; 3],
            [0.8, 0.1, 0.2],
            [0.9, 0.89, 0.88],
            [0.02, 0.01, 0.0],
        ] {
            let value = encoded.into_iter().fold(0.0f32, f32::max);
            let minimum = encoded.into_iter().fold(1.0f32, f32::min);
            let saturation = (value - minimum) / (value + 1.0e-10);
            let emission = (((saturation * 3.125 - 0.125).clamp(0.0, 1.0) * value.powi(4))
                .max((value * 7.0 - 6.0).clamp(0.0, 1.0)))
                * 0.5;
            expected.push(if texture_class(&catalog.textures()[layer as usize]) != 0 {
                emission
            } else {
                0.0
            });
            cases.push([
                encoded[0].powf(2.2),
                encoded[1].powf(2.2),
                encoded[2].powf(2.2),
                f32::from_bits(layer),
            ]);
        }
    }
    let source = format!(
        "{}\n{}\n{}",
        shader(&catalog),
        r#"
@group(0) @binding(0) var<storage,read> cases:array<vec4f>;
@group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
@group(0) @binding(2) var<storage,read> material_emission:array<f32>;
"#,
        consumers()
    ) + r#"
@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3u) {
 let c=cases[id.x];let layer=bitcast<u32>(c.w);
 result[id.x*2u]=vec4f(near_default(layer,c.rgb),lod_default(layer+1u,c.rgb),near_advanced(layer,c.rgb),lod_advanced(layer+1u,c.rgb));
 result[id.x*2u+1u]=vec4f(lod_default(0u,c.rgb),lod_advanced(0u,c.rgb),0.0,0.0);
}"#;
    let parsed = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&parsed)
    .unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source default emission"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&cases),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let catalog_emission = vec![1.7f32; catalog.textures().len()];
    let materials = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&catalog_emission),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = (cases.len() * 2 * 16) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: materials.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(cases.len() as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, bytes);
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.map_async(wgpu::MapMode::Read, .., move |r| {
        tx.send(r).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = read.get_mapped_range(..).unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    for (i, (rows, expected)) in rows.chunks_exact(2).zip(expected).enumerate() {
        for channel in 0..2 {
            assert!(
                (rows[0][channel] - expected).abs() < 2.0e-6,
                "case{i}: {:?} !={expected}",
                rows[0]
            );
        }
        assert_eq!(&rows[0][2..], &[1.7, 1.7]);
        assert_eq!(rows[1], [0.0; 4]);
    }
}

#[test]
fn portable_builtin_classification_preserves_audited_shader_bytes() {
    use sha2::{Digest, Sha256};
    // Captured from the previous source-driven implementation across every
    // builtin texture, including zero/default classifications.
    let source = shader(&Catalog::builtins());
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        "c93435e5f71f93475351a380f1bd029820240479420f677d82be75e326957ca1"
    );
}
