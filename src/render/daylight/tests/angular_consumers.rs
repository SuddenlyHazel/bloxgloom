//! Invoke verbatim production receiver bodies with a minimal resource set.
//! This catches packet/normal/unit mismatches between terrain, actors and LOD;
//! source-convolution error and primary GI records have their separate oracle.
use super::super::*;
use wgpu::util::DeviceExt;
fn between<'a>(s: &'a str, start: &str, end: &str) -> &'a str {
    let a = s.find(start).unwrap();
    let b = s[a..].find(end).unwrap() + a;
    &s[a..b]
}
fn source() -> String {
    let terrain = include_str!("../../pipeline.wgsl");
    let actor = include_str!("../../avatars/shader.wgsl");
    let lod = include_str!("../../lod/shader.wgsl");
    let camera = terrain
        .lines()
        .find(|l| l.starts_with("struct Camera"))
        .unwrap();
    let terrain_types = between(terrain, "struct VertexInput {", "@group(1)");
    let terrain_body = between(terrain, "fn voxel_vertex(", "@vertex fn vs_main");
    let rename_actor = |s: &str| {
        s.replace("VertexInput", "ActorInput")
            .replace("VertexOutput", "ActorOutput")
    };
    let actor_types = rename_actor(between(
        actor,
        "struct VertexInput {",
        "// REGISTERED_PALETTES",
    ));
    let actor_body = rename_actor(between(actor, "fn avatar_vertex(", "@vertex fn vs_main"));
    let lod_types = between(lod, "struct In {", "@vertex fn vs_main");
    let lod_body = between(lod, "@vertex fn vs_main(", "fn bg_lod_coverage")
        .replace("@vertex fn vs_main(", "fn lod_vertex(");
    surface_shader(&format!(
        r#"{camera}
 @group(0) @binding(0) var<uniform> camera:Camera;
 @group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
 struct Tile {{relative:vec4f,origin:vec4i}};
 @group(1) @binding(0) var<uniform> tile:Tile;
 struct MaterialMetadata {{flags:u32,layer:u32}};
 @group(1) @binding(5) var<storage,read> material_map_flags:array<MaterialMetadata>;
 {}
 {}
 fn bg_vertex(v:BgVertex,layer:u32)->BgVertex {{return v;}}
 fn bg_shadow_project(world:vec3f)->vec4f {{return vec4f(world,1.0);}}
 const SKINS=array<vec3f,32>();const SHIRTS=array<vec3f,32>();const PANTS=array<vec3f,32>();
 {terrain_types}
 {terrain_body}
 {actor_types}
 {actor_body}
 {lod_types}
 {lod_body}
 {}
 @compute @workgroup_size(1) fn test_consumers(@builtin(global_invocation_id) id:vec3u) {{
 let face=id.x%6u;let normals=array<vec3f,6>(vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0),vec3f(-1.0,0.0,0.0),vec3f(1.0,0.0,0.0),vec3f(0.0,0.0,1.0),vec3f(0.0,0.0,-1.0));let n=normals[face];let sky=select(1.0,0.0,id.x>=6u);
 let main=voxel_vertex(VertexInput(vec3f(0.0),n,vec2f(0.5),0.0,vec2f(sky,0.0),0.0,0.0,0.0,0.0));
 let actor=avatar_vertex(ActorInput(vec3f(0.0),n,5u,vec3f(0.0),vec4u(0u),vec2u(u32(sky*15.0),0u),vec4u(0u),vec4f(0.0),vec3f(1.0),vec3f(1.0),vec4u(0u),vec4f(0.0,0.0,0.0,1.0)),false);
 let codes=array<u32,6>(3u,2u,0u,1u,5u,4u);let coarse=lod_vertex(In(vec3f(0.0),0xffffffffu,codes[face]|(u32(sky*15.0)<<3u)));
 result[id.x*10u]=vec4f(bg_surface_light(n,camera.sun,sky,vec3f(0.0),vec3f(0.0),vec3f(0.0),1.0),1.0);
 result[id.x*10u+1u]=vec4f(main.light,1.0);result[id.x*10u+2u]=vec4f(actor.color,1.0);result[id.x*10u+3u]=vec4f(coarse.light,1.0);
 result[id.x*10u+4u]=vec4f(actor.indirect,1.0);result[id.x*10u+5u]=vec4f(coarse.indirect,1.0);
 let old=bg_scene_output(vec3f(0.7,0.3,0.1),vec3f(0.1,0.04,0.02),vec3f(0.0,0.0,3.0),sky,-1.0);
 let receiver=bg_actor_scene_output(vec3f(0.7,0.3,0.1),vec3f(0.1,0.04,0.02),vec3f(0.0,0.0,3.0),n,sky,-1.0);
 result[id.x*10u+6u]=receiver.reflection_normal;result[id.x*10u+7u]=receiver.reflection_response;
 result[id.x*10u+8u]=receiver.color-old.color;result[id.x*10u+9u]=receiver.indirect-old.indirect;
 }}"#,
        crate::render::custom::TYPES,
        format_args!(
            "{}\n{}\n{}",
            include_str!("../../material/foliage.wgsl"),
            crate::render::trace::dynamic::DEFORMATION_SHADER,
            crate::render::trace::dynamic::MATERIAL_SHADER
        ),
        crate::render::avatars::SHADING_SHADER
    ))
}
#[test]
fn actual_terrain_actor_lod_angular_receiver_bodies_validate() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_actual_terrain_actor_lod_share_angular_sky_and_dark_shelter() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("verbatim production terrain actor LOD angular consumers"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("test_consumers"),
        compilation_options: Default::default(),
        cache: None,
    });
    let atmosphere = Atmosphere::at(crate::daylight::CYCLE_MS / 48);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO)),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let tile = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 32],
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let metadata = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 8],
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = 12 * 10 * 16;
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
    let group0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let group1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: tile.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: metadata.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group0, &[]);
        pass.set_bind_group(1, &group1, &[]);
        pass.dispatch_workgroups(12, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, bytes);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let values =
        bytemuck::cast_slice::<u8, [f32; 4]>(&read.slice(..).get_mapped_range().unwrap()).to_vec();
    for (case, rows) in values.chunks_exact(10).enumerate() {
        for row in &rows[1..4] {
            for (actual, expected) in row.iter().zip(rows[0]) {
                assert!((*actual - expected).abs() < 1e-6, "case{case}: {rows:?}");
            }
        }
        assert_eq!(rows[4], rows[5]);
        assert_eq!(rows[8], [0.0; 4], "native matte art/color/fog changed");
        assert_eq!(rows[9], [0.0; 4], "native indirect/history sign changed");
        if crate::render::bsl_reference::enabled() {
            assert_eq!(rows[6], [0.0; 4]);
            assert_eq!(rows[7], [0.0; 4]);
        } else {
            let encoded = [
                [0.0, 1.0],
                [0.0, -1.0],
                [-1.0, 0.0],
                [1.0, 0.0],
                [0.0, 0.0],
                [1.0, 1.0],
            ][case % 6];
            assert_eq!(rows[6], [encoded[0], encoded[1], 1.0, 3.0]);
            assert_eq!(rows[7], [0.0, 0.0, 0.0, if case >= 6 { 0.0 } else { 1.0 }]);
        }
        if case >= 6 {
            assert_eq!(rows[4], [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(rows[0], [0.012, 0.015, 0.022, 1.0]);
        } else {
            assert!(rows[4][..3].iter().any(|v| *v > 0.0) || case == 1);
        }
    }
}

#[test]
fn gpu_actual_drop_plant_pose_skips_terrain_wind_but_world_plant_moves() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        source(),
        r#"
@compute @workgroup_size(1) fn drop_pose(@builtin(global_invocation_id) id:vec3u) {
 let tagged=select(0.0,-1.0,id.x==0u);
 let v=voxel_vertex(VertexInput(vec3f(-7.0,8.0,11.0),vec3f(0.0,1.0,0.0),vec2f(0.5,0.0),0.0,vec2f(1.0,0.0),0.0,tagged,0.0,0.0));
 result[id.x]=vec4f(v.world_position,v.history_sign);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual pickup/world foliage vertex pose"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("drop_pose"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut atmosphere = Atmosphere::at(crate::daylight::CYCLE_MS / 4);
    atmosphere.wind_seconds = 17.25;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO)),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let metadata = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[64u32, 0]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 32,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 32,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let g0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let g1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 5,
            resource: metadata.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        pass.dispatch_workgroups(2, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 32);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let values =
        bytemuck::cast_slice::<u8, [f32; 4]>(&read.slice(..).get_mapped_range().unwrap()).to_vec();
    assert_eq!(
        values[0],
        [-7.0, 8.0, 11.0, -1.0],
        "presented pickup receives no additional terrain deformation"
    );
    assert_eq!(values[1][3], -1.0, "world wind remains temporally reactive");
    assert!(
        Vec3::from_array(values[1][..3].try_into().unwrap()).distance(Vec3::new(-7.0, 8.0, 11.0))
            > 0.001,
        "world vegetation still sways"
    );
}
