use super::*;
use crate::render::trace::scene::{Chunk, Triangle, surface};
use bytemuck::Zeroable;
use std::sync::Arc;
#[path = "tests/root_order.rs"]
mod root_order;

fn item(source: &str, prefix: &str) -> String {
    let start = source
        .find(prefix)
        .unwrap_or_else(|| panic!("missing production item {prefix}"));
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, c) in source[open..].char_indices() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                let end = open + offset + 1;
                return source[start..end + usize::from(source[end..].starts_with(';'))].to_owned();
            }
        }
    }
    panic!("unterminated production item {prefix}")
}
fn source() -> String {
    let intersection = include_str!("../../intersection.wgsl");
    let test = item(intersection, "fn ray_test_triangle(");
    let ordered_shader = format!(
        "{}\n{}\n{}",
        item(intersection, "struct RayPending "),
        item(intersection, "fn ray_box_near("),
        SHADER.replace(
            "const RAY_LOD_ROOT_ORDER:bool=false;",
            "const RAY_LOD_ROOT_ORDER:bool=true;"
        )
    );
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        item(intersection, "struct RayNode "),
        item(intersection, "struct RayTriangle "),
        item(intersection, "struct RayHit "),
        item(intersection, "struct RayMaterialMetadata "),
        item(include_str!("../../transport.wgsl"), "struct RayFrame "),
        include_str!("../../../material/foliage.wgsl"),
        item(intersection, "fn ray_wind("),
        item(intersection, "fn ray_box("),
        include_str!("../../coverage.wgsl"),
        test,
        FIXTURE_INPUTS,
        ordered_shader,
        FIXTURE_ENTRY
    )
}
const FIXTURE_INPUTS: &str = r#"
const RAY_MATERIAL_FAST=true;
const RAY_LOD_TIERED=true;
var<private> ray_frame:RayFrame;
@group(1) @binding(0) var ray_albedo:texture_2d_array<f32>;
@group(1) @binding(1) var ray_sampler:sampler;
@group(1) @binding(5) var<storage,read> ray_materials:array<RayMaterialMetadata>;
@group(0) @binding(18) var<storage,read_write> results:array<vec4u>;
"#;
const FIXTURE_ENTRY: &str = r#"
fn full_cast(origin:vec3f,direction:vec3f,limit:f32,current:RayHit)->RayHit {
    var hit=current;
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    for(var page=0u;page<4u;page++) {
        var index=0u;
        loop {
            if index>=ray_lod_word(page,0u) {break;}
            let node=ray_lod_node(page,index);
            if !ray_box(origin,inverse,node,hit.distance) {index=node.escape;continue;}
            if node.count==0u {index++;continue;}
            for(var i=node.first;i<node.first+node.count;i++) {
                let encoded=((page+1u)<<28u)|i;
                hit=ray_test_triangle(origin,direction,limit,encoded,hit,ray_lod_triangle(encoded));
            }
            index=node.escape;
        }
    }
    return hit;
}
fn equal_hit(a:RayHit,b:RayHit)->bool {
    return a.triangle==b.triangle&&a.distance==b.distance&&all(a.uv==b.uv)&&all(a.normal==b.normal);
}

@compute @workgroup_size(1) fn check(){
    ray_frame.parameters.y=1.0;
    let empty=RayHit(1000.0,0xffffffffu,vec2f(0.0),vec3f(0.0));
    let nearest=ray_lod_cast(vec3f(0.0),vec3f(1.0,0.0,0.0),1000.0,empty);
    let translated=ray_lod_cast(vec3f(-100.0,0.0,0.0),vec3f(1.0,0.0,0.0),1000.0,empty);
    let middle=ray_lod_cast(vec3f(3.0,0.0,0.0),vec3f(1.0,0.0,0.0),1000.0,empty);
    let missed=ray_lod_cast(vec3f(0.0),vec3f(-1.0,0.0,0.0),1000.0,empty);
    results[0]=vec4u(bitcast<u32>(nearest.distance),nearest.triangle,bitcast<u32>(translated.distance),translated.triangle);
    results[1]=vec4u(bitcast<u32>(middle.distance),middle.triangle,missed.triangle,u32(ray_lod_any_opaque(vec3f(0.0),vec3f(1.0,0.0,0.0),1000.0)));
    let native=RayHit(2.0,7u,vec2f(0.0),vec3f(-1.0,0.0,0.0));
    let retained=ray_lod_cast(vec3f(0.0),vec3f(1.0,0.0,0.0),1000.0,native);
    let t=ray_lod_triangle(0x40000000u);
    results[2]=vec4u(t.surface_color,t.surface_flags,bitcast<u32>(t.uv_c.x),retained.triangle);
    var errors=0u;var accepted=0u;var alpha_rejected=0u;var alpha_accepted=0u;
    for(var page=0u;page<4u;page++) {
        for(var index=0u;index<ray_lod_word(page,1u);index++) {
            let id=((page+1u)<<28u)|index;let triangle=ray_lod_triangle(id);
            for(var probe=0u;probe<12u;probe++) {
                let u=select(0.1,0.8,(probe&1u)!=0u);let v=0.1;
                let point=triangle.a.xyz*(1.0-u-v)+triangle.b.xyz*u+triangle.c.xyz*v;
                let d=select(vec3f(1.0,0.0,0.0),vec3f(-1.0,0.0,0.0),(probe&2u)!=0u);
                let origin=point-d*10.0;
                let limit=select(11.0,9.0,probe>=8u);
                let initial=RayHit(select(1000.0,8.0,probe>=4u&&probe<8u),7u,vec2f(0.0),vec3f(0.0));
                let old=ray_test_triangle(origin,d,limit,id,initial,triangle);
                let candidate=ray_lod_candidate(origin,d,limit,id,initial);
                if !equal_hit(old,candidate) {errors++;}
                if candidate.triangle==id {accepted++;}
                if triangle.c.w>0.5&&probe<4u {
                    if candidate.triangle==id {alpha_accepted++;}else{alpha_rejected++;}
                }
                if !equal_hit(full_cast(origin,d,limit,initial),ray_lod_cast(origin,d,limit,initial)) {errors++;}
            }
        }
    }
    results[3]=vec4u(errors,accepted,alpha_rejected,alpha_accepted);
    let root=ray_lod_node(2u,0u);
    results[4]=vec4u(bitcast<u32>(root.low.x),bitcast<u32>(root.high.x),root.count,root.escape);
    let prior=full_cast(vec3f(3.0,0.0,0.0),vec3f(1.0,0.0,0.0),1000.0,empty);
    results[5]=vec4u(u32(ray_box(vec3f(3.0,0.0,0.0),vec3f(1.0,10000000.0,10000000.0),root,1000.0)),bitcast<u32>(prior.distance),prior.triangle,0u);
    let direct=ray_test_triangle(vec3f(3.0,0.0,0.0),vec3f(1.0,0.0,0.0),1000.0,0x30000000u,empty,ray_lod_triangle(0x30000000u));
    let cheap=ray_lod_candidate(vec3f(3.0,0.0,0.0),vec3f(1.0,0.0,0.0),1000.0,0x30000000u,empty);
    results[6]=vec4u(bitcast<u32>(direct.distance),direct.triangle,bitcast<u32>(cheap.distance),cheap.triangle);
}
"#;
fn plane(x: f32, id: u32) -> Triangle {
    let mut t = Triangle::zeroed();
    t.a = [x, -2.0, -2.0, id as f32];
    t.b = [x, 2.0, -2.0, 1.0];
    t.c = [x, 2.0, 2.0, 0.0];
    t.uv_ab = [0.0, 0.0, 1.0, 0.0];
    t.uv_c = [-3.5, 1.0];
    t.normal = [-1.0, 0.0, 0.0, -1.0];
    t.with_surface(
        [1.0; 4],
        surface::LOD | surface::NO_WIND | surface::COARSE_COLOR,
    )
}
fn scenes() -> Vec<Scene> {
    [8.0, 6.0, 4.0, 2.0]
        .into_iter()
        .enumerate()
        .map(|(page, x)| {
            let mut triangles = vec![plane(x, 0)];
            if page == 0 {
                // Force internal nodes and escape links, with far off-axis
                // leaves that the actual page walk must skip.
                for n in 0..10 {
                    let mut missed = plane(x, 0);
                    missed.a[2] += 100.0 + n as f32 * 4.0;
                    missed.b[2] += 100.0 + n as f32 * 4.0;
                    missed.c[2] += 100.0 + n as f32 * 4.0;
                    if n == 0 {
                        missed.c[3] = 1.0;
                        missed.surface_flags |= surface::TEXTURE;
                    }
                    triangles.push(missed);
                }
            }
            Scene::build([Arc::new(Chunk {
                key: None,
                triangles,
                ..Default::default()
            })])
        })
        .collect()
}
#[test]
fn bounded_page_shader_uses_production_intersection_and_validates() {
    let source = source();
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let entries = LodGeometry::layout_entries();
    for (n, entry) in entries.iter().enumerate() {
        assert_eq!(entry.binding, 11 + n as u32);
        assert!(entry.visibility.contains(wgpu::ShaderStages::COMPUTE));
    }
}
#[test]
fn gpu_four_static_pages_preserve_packed_bits_and_closest_hits() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let lod = LodGeometry::new(&device, &scenes());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production paged LOD traversal"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 112,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 112,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0u32; 12]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let metadata = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0u32; 4]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[255, 255, 255, 255, 255, 255, 255, 0],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    let albedo = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        ..Default::default()
    });
    let mut entries = LodGeometry::layout_entries().to_vec();
    for (binding, read_only) in [(10, true), (18, false)] {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });
    let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
        label: None,
        bind_group_layouts: &[Some(&layout), Some(&material_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("check"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut bindings = lod.entries().to_vec();
    bindings.extend([
        wgpu::BindGroupEntry {
            binding: 10,
            resource: coverage.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 18,
            resource: output.as_entire_binding(),
        },
    ]);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &bindings,
    });
    let materials = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: metadata.as_entire_binding(),
            },
        ],
    });
    let run = |group: &wgpu::BindGroup| {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_bind_group(1, &materials, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 112);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let words =
            bytemuck::cast_slice::<u8, u32>(&read.slice(..).get_mapped_range().unwrap()).to_vec();
        read.unmap();
        words
    };
    let words = run(&group);
    assert_eq!(f32::from_bits(words[0]), 2.0);
    assert_eq!(words[1], 0x40000000);
    assert_eq!(f32::from_bits(words[2]), 102.0);
    assert_eq!(words[3], 0x40000000);
    assert_eq!(f32::from_bits(words[4]), 1.0);
    assert_eq!(words[5], 0x30000000);
    assert_eq!(words[6], u32::MAX);
    assert_eq!(words[7], 1);
    assert_eq!(words[8], u32::MAX);
    assert_eq!(words[9], 21);
    assert_eq!(f32::from_bits(words[10]), -3.5);
    assert_eq!(words[11], 7);
    assert!(f32::from_bits(words[16]) < 4.0 && f32::from_bits(words[17]) > 4.0);
    assert_eq!(
        words[18..21],
        [1, 1, 1],
        "actual root count/escape/AABB acceptance"
    );
    assert_eq!(f32::from_bits(words[21]), 1.0);
    assert_eq!(words[22], 0x30000000);
    assert_eq!(f32::from_bits(words[24]), 1.0);
    assert_eq!(words[25], 0x30000000);
    assert_eq!(
        words[24..26],
        words[26..28],
        "shared/full and tiered geometric acceptance"
    );
    assert_eq!(
        words[12], 0,
        "tiered candidate/page ordering differs from shared full decoder"
    );
    assert!(words[13] > 0);
    assert!(
        words[14] > 0 && words[15] > 0,
        "actual LOD0 alpha holes and accepted artwork must be exercised"
    );
    // An actual known-empty near cell suppresses overlapping distant faces,
    // including an origin translated outside that cell. It does not erase
    // the admitted packed page or certify any new distant air.
    queue.write_buffer(
        &coverage,
        0,
        bytemuck::cast_slice(&[0u32, 0, 0, 113, 1, 1, 1, 1, 1, 0, 0, 0]),
    );
    let masked = run(&group);
    assert_eq!(masked[1], u32::MAX);
    assert_eq!(masked[3], u32::MAX);
    assert_eq!(masked[5], u32::MAX);
    assert_eq!(masked[7], 0);
    assert_eq!(
        masked[12], 0,
        "masked candidate/page ordering differs from reference"
    );
    queue.write_buffer(&coverage, 0, bytemuck::cast_slice(&[0u32; 12]));
    let empty = LodGeometry::new(&device, &[]);
    let mut empty_bindings = empty.entries().to_vec();
    empty_bindings.extend([
        wgpu::BindGroupEntry {
            binding: 10,
            resource: coverage.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 18,
            resource: output.as_entire_binding(),
        },
    ]);
    let empty_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &empty_bindings,
    });
    let empty_words = run(&empty_group);
    assert_eq!(empty_words[1], u32::MAX);
    assert_eq!(empty_words[7], 0);
    assert_eq!(
        empty_words[8..11],
        [0; 3],
        "empty page decode must stay bounded"
    );
    root_order::check(&device, &layout, &coverage, &output, run);
}
