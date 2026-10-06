use crate::render::avatars;
#[path = "oracle.rs"]
mod oracle;
#[test]
fn reference_actor_actual_shaders_validate_in_all_modes() {
    let catalog = crate::content::Catalog::builtins();
    for source in [
        avatars::appearance::shader(&catalog),
        avatars::character_shader(&catalog),
        avatars::motion::shader(
            crate::render::daylight::shader(&format!(
                "{}\n{}\n{}",
                crate::render::trace::dynamic::DEFORMATION_SHADER,
                avatars::SHADING_SHADER,
                include_str!("../authored.wgsl")
            )),
            3,
        ),
    ] {
        for (reference, advanced) in [(false, false), (true, false), (false, true)] {
            let source = source
                .replace(
                    &format!(
                        "const BG_BSL_REFERENCE: bool = {};",
                        crate::render::bsl_reference::default_materials()
                    ),
                    &format!("const BG_BSL_REFERENCE: bool = {reference};"),
                )
                .replace(
                    &format!(
                        "const BG_BSL_ADVANCED_REFERENCE: bool = {};",
                        crate::render::bsl_reference::advanced_materials()
                    ),
                    &format!("const BG_BSL_ADVANCED_REFERENCE: bool = {advanced};"),
                );
            let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}

#[test]
fn gpu_actual_all_actor_reference_entity_fragments_match_source_equations() {
    use wgpu::util::DeviceExt;
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let production = include_str!("../shader.wgsl");
    let structure = &production[production.find("struct VertexOutput").unwrap()
        ..production.find("// REGISTERED_PALETTES").unwrap()];
    let body = &production[production.find("@fragment fn fs_main").unwrap()
        ..production.find("struct MotionOutput").unwrap()];
    let native = include_str!("../character.wgsl");
    let native_types = native
        [native.find("struct Output {").unwrap()..native.find("fn character_vertex(").unwrap()]
        .replace("input: Output", "input: NativeOutput")
        .replace("struct Output", "struct NativeOutput");
    let native_body = native[native.find("@fragment fn fs_main").unwrap()
        ..native.find("@fragment fn fs_shadow").unwrap()]
        .replace("@fragment fn fs_main", "fn native_fragment")
        .replace("input: Output", "input: NativeOutput")
        .replace("struct Output", "struct NativeOutput");
    let authored = include_str!("../authored.wgsl");
    let authored_types = authored
        [authored.find("struct Output {").unwrap()..authored.find("fn vertex(").unwrap()]
        .replace("input: Output", "input: AuthoredOutput")
        .replace("struct Output", "struct AuthoredOutput");
    let authored_body = authored[authored.find("@fragment fn fs_main").unwrap()
        ..authored.find("@fragment fn fs_shadow").unwrap()]
        .replace("@fragment fn fs_main", "fn authored_fragment")
        .replace("@builtin(front_facing) ", "")
        .replace("input: Output", "input: AuthoredOutput")
        .replace("struct Output", "struct AuthoredOutput");
    let extra = format!(
        "{native_types}\n{native_body}\n{authored_types}\n{authored_body}\n{}",
        r#"
struct Part {flags:vec4u};const parts=array<Part,1>(Part(vec4u(0u)));
fn character_albedo(input:NativeOutput)->vec4f {return vec4f(bg_actor_reference_encoded(vec3f(0.2,0.4,0.1)),1.0);}
fn color(input:AuthoredOutput)->vec4f {return vec4f(bg_actor_reference_encoded(vec3f(0.2,0.4,0.1)),1.0);}
fn bg_surface_light(n:vec3f,s:vec4f,sky:f32,l:vec3f,b:vec3f,g:vec3f,v:f32)->vec3f{return vec3f(1000);}
fn bg_direct_light(n:vec3f,s:vec4f,sky:f32)->vec3f{return vec3f(1000);}
fn bg_indirect_light(n:vec3f,s:vec4f,sky:f32,b:vec3f,g:vec3f)->vec3f{return vec3f(1000);}
"#
    );
    let actor_bodies = format!("{body}\n{extra}");
    let source=format!("{}\n{}\n{}\n{}\n{}\n{}",crate::render::bsl_reference::LIGHTING_SHADER,structure,r#"
const BG_BSL_REFERENCE:bool=true;
fn bg_bsl_reference_lightmap(lm:vec2f,world:vec3f)->vec2f {return lm;}
struct Camera {eye:vec4f,sun:vec4f};const camera=Camera(vec4f(0,0,0,1),vec4f(0,1,0,1));
struct BgSceneOutput {@location(0) color:vec4f};
fn bg_scene_output(color:vec3f,indirect:vec3f,world:vec3f,sky:f32,history:f32)->BgSceneOutput {return BgSceneOutput(vec4f(color,1));}
fn bg_scene_reflection(output:BgSceneOutput,n:vec3f,r:f32,d:f32,response:vec3f,sky:f32)->BgSceneOutput{return output;}
fn bg_shadow_receiver(world:vec3f)->f32{return 1.0;}
fn bg_bsl_reference_sun_visibility(receiver:f32,n:vec3f,s:vec3f,basic:f32,sky:f32)->f32 {return receiver_shadow;}
fn bg_shadowed_local_light(world:vec3f,n:vec3f,r:vec3f,d:vec3f)->vec3f{return vec3f(1000.0);}
fn bg_sun_visibility_material(receiver:f32,n:vec3f,sky:f32,basic:f32)->f32{return 0.0;}
fn bg_primary_sun_transmittance(world:vec3f)->f32{return 0.0;}
fn bg_local_history_sign(world:vec3f,n:vec3f,r:vec3f,d:vec3f)->f32{return 1.0;}
var<private> receiver_shadow:f32;
var<private> current_frame:BgBslReferenceFrame;
fn bg_bsl_reference_frame()->BgBslReferenceFrame{return current_frame;}
"#,avatars::SHADING_SHADER,actor_bodies,r#"
@vertex fn fixture_vertex(@builtin(vertex_index) i:u32)->@builtin(position)vec4f {let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);}
@fragment fn fixture_fragment(@builtin(position) p:vec4f)->BgSceneOutput {
 let i=u32(p.x);let family=u32(p.y);let sky=select(1.0,0.0,i==3u||i==4u);
 let glow=select(0.0,1.0,i==4u);receiver_shadow=select(1.0,0.0,i==1u);
 let rain=select(0.0,1.0,i==5u);let visible=select(1.0,0.0,i==6u);
 var normal=vec3f(0,1,0);if i==2u {normal=vec3f(1,0,0);}
 let light=pow(vec3f(196,220,255)*(1.4/255.0),vec3f(2));let ambient=pow(vec3f(120,172,255)*(0.6/255.0),vec3f(2));
 current_frame=BgBslReferenceFrame(light,ambient,vec3f(0,1,0),visible,rain,1,1);
 var input:VertexOutput;input.normal=normal;input.sky=sky;input.block_level=glow;input.world_position=vec3f(0,0,-3);
 input.surface_color=bg_actor_reference_encoded(vec3f(0.2,0.4,0.1))*bg_actor_reference_encoded(vec3f(0.5,0.75,0.25));
 input.color=vec3f(1000);input.direct=vec3f(1000);input.indirect=vec3f(1000);input.local_radiance=vec3f(1000);
 if family==1u {
  var native:NativeOutput;native.normal_sky=vec4f(normal,sky);native.world_position=vec4f(0,0,-3,0.25);
  native.light.w=0.5;native.indirect.w=0.75;native.direct.x=glow;
  return native_fragment(native);
 }
 if family==2u {
  var authored:AuthoredOutput;authored.normal=normal;authored.world=vec3f(0,0,-3);authored.tint=vec3f(0.5,0.75,0.25);
  authored.light_levels=vec2u(u32(sky*15.0)|(u32(glow*15.0)<<8u),0u);
  return authored_fragment(authored,true);
 }
 return fs_main(input);
}
"#).replace("@fragment fn fs_main", "fn fs_main");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual primitive reference entity fragment"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("fixture_vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fixture_fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 7,
            height: 3,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let read = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 768],
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(3),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&data);
    for family in 0..3 {
        for (i, row) in rows[family * 16..family * 16 + 7].iter().enumerate() {
            let normal = if i == 2 {
                glam::DVec3::X
            } else {
                glam::DVec3::Y
            };
            let sky = if i == 3 || i == 4 { 0.0 } else { 1.0 };
            let encoded = oracle::map(glam::DVec3::new(0.2, 0.4, 0.1), oracle::encoded);
            let tint = glam::DVec3::new(0.5, 0.75, 0.25);
            let expected = oracle::entity(
                encoded,
                tint,
                normal,
                [
                    if i == 4 { 1.0 } else { 0.0 },
                    sky,
                    if i != 1 { 1.0 } else { 0.0 },
                    if i == 5 { 1.0 } else { 0.0 },
                    if i != 6 { 1.0 } else { 0.0 },
                ],
            );
            for (c, expected) in expected.to_array().into_iter().enumerate() {
                assert!(
                    (f64::from(row[c]) - expected).abs() < 3e-6,
                    "actor family{family} row{i} channel{c}:{} vs{expected}",
                    row[c]
                );
            }
        }
    }
}
