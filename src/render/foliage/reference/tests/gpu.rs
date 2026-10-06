//! Independent portable GPU oracle, with an optional live GLSL comparison.
use super::*;
use wgpu::util::DeviceExt;

#[test]
fn gpu_reference_botanical_wind_matches_independent_oracle_class_top_and_eye_bend() {
    let catalog = Catalog::builtins();
    let keys = [
        "leaves",
        "jg_short_grass",
        "jg_tall_grass_bottom",
        "jg_tall_grass_top",
        "flower_red",
        "jg_vine",
        "jg_pink_petals",
        "jg_dead_bush",
        "jg_torchflower",
        "jg_lily_pad",
    ];
    let layers: Vec<_> = keys
        .iter()
        .map(|key| {
            catalog
                .textures()
                .iter()
                .position(|t| t.key.as_ref() == format!("bloxgloom:{key}"))
                .unwrap() as u32
        })
        .collect();
    let classes: Vec<_> = layers
        .iter()
        .map(|&id| wind_class(&catalog.textures()[id as usize]))
        .collect();
    let mut cases = Vec::new();
    for (&layer, &class) in layers.iter().zip(&classes) {
        for seconds in [0.0, 7.25, 128.0, 256.0] {
            for y in [33.0, 33.45] {
                for uv_y in [0.0, 1.0] {
                    for offset in [[0.0; 3], [-0.35, 0.25, 0.4], [2.0, 0.5, -3.0]] {
                        // world/time, camera/uvY, texture/class, source relativeEyePosition.
                        cases.push([2.5, y, -7.75, seconds]);
                        cases.push([2.45, 33.0, -7.85, uv_y]);
                        cases.push([f32::from_bits(layer), f32::from_bits(class), 0.0, 0.0]);
                        cases.push([offset[0], offset[1], offset[2], 0.0]);
                    }
                }
            }
        }
    }
    let count = (cases.len() / 4) as u32;
    let source = format!(
        "{}\n{}\n{}\n{}\n{}",
        shader(&catalog),
        include_str!("../../../material/foliage.wgsl"),
        super::oracle::source(),
        super::callers::source(),
        r#"
@group(0) @binding(0) var<storage,read> cases:array<vec4f>;
@group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3u) {
 let p=cases[id.x*4u];let eye=cases[id.x*4u+1u];let metadata=bitcast<vec4u>(cases[id.x*4u+2u]);let offset=cases[id.x*4u+3u].xyz;
 source_time=p.w;cameraPosition=eye.xyz;relativeEyePosition=offset;
 let actual=bg_bsl_foliage_wind(p.xyz,vec2f(0.25,eye.w),p.w,eye.xyz,offset,metadata.x);
 let expected=WavingBlocks(p.xyz-eye.xyz,metadata.y,select(0.0,1.0,eye.w<0.5))+eye.xyz;
 let normal=select(vec3f(0.0,1.0,0.0),normalize(vec3f(1.0,0.2,0.0)),metadata.y==105u);
 var wind_uv=vec2f(0.25,eye.w);let flags=material_map_flags[metadata.x].flags;
 if (flags&8u)!=0u {wind_uv.y=wind_uv.y*0.5+select(0.5,0.0,(flags&128u)!=0u);}
 camera.ambient_sh[5]=vec4f(offset,0.0);camera.eye=vec4f(eye.xyz,0.0);camera.fog_range=vec4f(0.0,0.0,p.w,0.0);
 var input=VertexInput(p.xyz,normal,vec2f(0.25,eye.w),f32(metadata.x),vec2f(1.0,0.0),0.0,0.0,0.0,0.0);
 let reference=reference_vertex(input);let enhanced=enhanced_vertex(input);
 input.glow_bounce_packed=-1.0;let dropped=reference_vertex(input);
 result[id.x*6u]=vec4f(actual,1.0);result[id.x*6u+1u]=vec4f(expected,1.0);
 result[id.x*6u+2u]=vec4f(bg_foliage_wind(p.xyz,normal,wind_uv,p.w),1.0);
 result[id.x*6u+3u]=vec4f(reference.world_position,1.0);result[id.x*6u+4u]=vec4f(enhanced.world_position,1.0);
 result[id.x*6u+5u]=vec4f(dropped.world_position,1.0);
}"#
    );
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source botanical wind comparison"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&cases),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let mut flags = vec![[0u32; 2]; catalog.textures().len()];
    for (&layer, &class) in layers.iter().zip(&classes) {
        flags[layer as usize] = [
            64 | if class == 102 {
                8
            } else if class == 103 {
                8 | 128
            } else {
                0
            },
            layer,
        ];
    }
    let metadata = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("actual botanical flags for production callers"),
        contents: bytemuck::cast_slice(&flags),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = u64::from(count) * 6 * 16;
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
    let entries = [
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
            resource: metadata.as_entire_binding(),
        },
    ];
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(count, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, bytes);
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
    let mapped = read.get_mapped_range(..).unwrap();
    let result: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    let mut distinct = 0;
    let mut offset_changes = 0;
    for cases in result.chunks_exact(18) {
        if (0..3).any(|axis| (cases[0][axis] - cases[6][axis]).abs() > 0.0001) {
            offset_changes += 1;
        }
        assert_eq!(
            cases[2], cases[8],
            "enhanced sway must ignore reference eye payload"
        );
    }
    assert!(
        offset_changes > 0,
        "nonzero source eye offsets must affect bending plants"
    );
    for (index, row) in result.chunks_exact(6).enumerate() {
        for axis in 0..3 {
            assert!(
                (row[0][axis] - row[1][axis]).abs() < 0.00001,
                "case{index} axis{axis}: port{:?} source{:?}",
                row[0],
                row[1]
            );
            // Metal can specialize/in-line a helper and its struct-based caller
            // differently (observed one float32 ULP). Independently compare BOTH
            // against the checked-in GLSL oracle at the original source bound.
            assert!(
                (row[3][axis] - row[1][axis]).abs() < 0.00001,
                "reference caller case{index} axis{axis}: caller{:?} source{:?}",
                row[3],
                row[1]
            );
            assert_eq!(
                row[2][axis].to_bits(),
                row[4][axis].to_bits(),
                "enhanced caller changed case{index} axis{axis}"
            );
            assert_eq!(
                row[5][axis].to_bits(),
                cases[index * 4][axis].to_bits(),
                "pickup must not receive terrain wind case{index}"
            );
        }
        if row[0][..3]
            .iter()
            .zip(&row[2][..3])
            .any(|(a, b)| (a - b).abs() > 0.001)
        {
            distinct += 1;
        }
    }
    assert!(
        distinct > count as usize / 4,
        "reference must differ materially from enhanced wind"
    );
    drop(mapped);
    read.unmap();
}
