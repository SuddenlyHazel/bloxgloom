//! Both new proposal-query families participate in current/static dependency replay.
use super::*;
#[test]
fn gpu_phase_and_ggx_water_guidance_tracks_native_blocker_replay_and_removal() {
    let catalog = crate::content::Catalog::builtins();
    let f = Fixture::new(&catalog);
    let key = crate::world::ChunkKey { x: 0, y: 0, z: 0 };
    let mut voxels = crate::world::Chunk::from_blocks(
        key,
        1,
        vec![crate::world::AIR; crate::world::CHUNK_VOLUME],
    );
    for z in 0..16 {
        for x in 0..16 {
            for y in 1..7 {
                voxels.blocks.set(
                    crate::world::Chunk::index([x, y, z]).unwrap(),
                    crate::world::WATER,
                );
            }
        }
    }
    let mesh = crate::render::mesh::mesh_chunk(&voxels);
    let near = [Arc::new(Chunk::from_world_mesh(&mesh, &catalog, &voxels))];
    let mut scene = Scene::build(near.iter().cloned());
    crate::render::trace::scene::volume::append(&mut scene, &near, &[]);
    let size = wgpu::Extent3d {
        width: 2,
        height: 1,
        depth_or_array_layers: 1,
    };
    let mut gpu = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    let mut frame = [0.0f32; 76];
    frame[37] = 1.0;
    frame[40..43].fill(1.0);
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    frame[73] = f32::from_bits(scene.water_offset);
    f.queue
        .write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&frame));
    let depth = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let blank = texture(&f, 2, 1, &[[0.0; 4]; 2]);
    let source = format!("{}\n{}", shaders::transport(), PROBE);
    let probe = |gpu: &Gpu| draw_probe(&f, gpu, [&depth, &blank, &blank, &blank], &source, 2);
    let mut targets = DynamicTargets::default();
    admitted(&f, &mut gpu, &targets);
    let clear = probe(&gpu);
    assert_eq!(clear, vec![[1.0, 1.0, 0.0, 1.0]; 2]);
    let layer = catalog
        .textures()
        .iter()
        .position(|t| t.key.ends_with(":jg_cherry_log"))
        .unwrap() as u32;
    let mut blocker = drop_target(layer);
    blocker.world = Mat4::from_scale_rotation_translation(
        Vec3::splat(3.0),
        glam::Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
        Vec3::new(8.0, 4.0, 8.0),
    );
    targets.instances.push(blocker);
    admitted(&f, &mut gpu, &targets);
    let blocked = probe(&gpu);
    assert_eq!(
        blocked,
        vec![[0.0, 1.0, 1.0, 1.0]; 2],
        "current real native first-hit disables both guides; static replay must recover them and mark dependency"
    );
    targets.instances.clear();
    admitted(&f, &mut gpu, &targets);
    assert_eq!(
        probe(&gpu),
        clear,
        "removed native blocker cannot leave a stale guide or dependency"
    );
}
const PROBE: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let phase=u32(pixel.x)==1u;
 ray_dynamic_disabled=false;ray_dynamic_touched=false;
 var current=RayWaterGuide(vec3f(0.0,1.0,0.0),1.0,false);
 if phase {current=ray_water_sun_guide(vec3f(8.0,2.5,8.0),vec3f(0.0,-1.0,0.0));}
 else {current=ray_water_diffuse_guide(vec3f(8.0,1.0,8.0),vec3f(0.0,1.0,0.0));}
 let touched=ray_dynamic_touched;ray_dynamic_disabled=true;
 var reference=RayWaterGuide(vec3f(0.0,1.0,0.0),1.0,false);
 if phase {reference=ray_water_sun_guide(vec3f(8.0,2.5,8.0),vec3f(0.0,-1.0,0.0));}
 else {reference=ray_water_diffuse_guide(vec3f(8.0,1.0,8.0),vec3f(0.0,1.0,0.0));}
 return vec4f(select(0.0,1.0,current.enabled),select(0.0,1.0,reference.enabled),select(0.0,1.0,touched),1.0);
}
"#;

fn admitted(f: &Fixture, gpu: &mut Gpu, targets: &DynamicTargets) {
    ready(f, gpu, targets);
    let mut encoder = f.device.create_command_encoder(&Default::default());
    gpu.dynamic.encode(&mut encoder);
    f.queue.submit([encoder.finish()]);
}
