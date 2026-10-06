//! Real voxel pools, published water interfaces, immutable BVH and production transport.
use super::*;
use crate::world::{AIR, CHUNK_VOLUME, ChunkKey, STONE, WATER};

mod lobes;

const PATHS: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let row=u32(pixel.x);ray_rng=row*1973u+991u;
 if (ray_frame.counts.y>=56u&&ray_frame.counts.y<=59u)||ray_frame.counts.y==62u||ray_frame.counts.y==63u {
  let phase_family=ray_frame.counts.y<=57u;let diffuse_family=ray_frame.counts.y>=62u;
  let enabled=(ray_frame.counts.y&1u)==1u;
  let n=vec3f(0.0,1.0,0.0);
  var origin=vec3f(8.0,1.5,8.0);var normal=n;var diffuse_allocation=vec3f(1.0);var pbr=bg_decode_pbr(vec4f(0.1,0.0,0.0,0.0),vec3f(1.0),false,true);
  if !phase_family {
   let hit=ray_cast(origin,-n,512.0);origin+=-n*hit.distance;normal=hit.normal;
   let surface=ray_surface(hit,origin);pbr=surface.pbr;
   let fresnel=select(vec3f(0.0),bg_pbr_fresnel(max(dot(normal,n),0.0),pbr),pbr.present);
   diffuse_allocation=surface.reflection*(1.0-pbr.metal)*(vec3f(1.0)-fresnel);
   origin+=normal*0.006;
  }
  var guide=RayWaterGuide(normal,1.0,false);if enabled {guide=ray_water_sun_guide(origin,normal);}
  var sum=vec3f(0.0);
  for(var i=0u;i<32u;i++) {
   let xi=vec3f(random(),random(),random());
   if phase_family {let sampled=ray_water_guided_phase(-n,guide,xi);sum+=sampled.weight*transport(origin,sampled.direction,1.0);}
   else if diffuse_family {let sampled=ray_water_guide_sample(normal,guide,xi);sum+=diffuse_allocation*sampled.weight*transport(origin,sampled.direction,1.0);}
   else {let sampled=ray_water_guided_ggx(normal,n,pbr,guide,xi);sum+=sampled.weight*transport(origin,sampled.direction,1.0);}
  }
  return vec4f(sum/32.0,select(0.0,1.0,guide.enabled));
 }
 if ray_frame.counts.y==38u {let guide=ray_water_diffuse_guide(vec3f(8.0,1.0,8.0),vec3f(0.0,1.0,0.0));return vec4f(select(0.0,1.0,guide.enabled),guide.axis);}
 if ray_frame.counts.y==30u {
  if row<=2u {
   let origins=array<vec3f,3>(vec3f(8.0,8.0,8.0),vec3f(8.0,4.0,8.0),vec3f(8.0,4.0,8.0));
   let directions=array<vec3f,3>(vec3f(0.0,-1.0,0.0),vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0));
   let hit=ray_cast(origins[row],directions[row],512.0);
   return vec4f(hit.distance,select(0.0,1.0,ray_water_is(hit)),ray_triangle_at(hit.triangle).normal.y,hit.normal.y);
  }
  return vec4f(f32(ray_water_at(vec3f(8.0,4.0,8.0))),f32(ray_water_at(vec3f(2.0,4.0,8.0))),
   f32(ray_water_at(vec3f(8.0,24.0,8.0))),ray_water_known_distance(vec3f(8.0,4.0,8.0),vec3f(1.0,0.0,0.0),512.0));
 }
 var sum=vec3f(0.0);
 let origin=select(vec3f(8.0,8.0,8.0),vec3f(8.0,4.0,8.0),(row&1u)==1u);
 let direction=select(vec3f(0.0,-1.0,0.0),vec3f(0.0,1.0,0.0),ray_frame.counts.y==32u);
 for(var i=0u;i<32u;i++) {sum+=transport(origin,direction,1.0);}
 return vec4f(sum/32.0,1.0);
}
"#;

pub(in crate::render::trace) fn catalog() -> (Catalog, crate::world::BlockId) {
    use crate::content::{BlockTextures, BlockTypeId, TextureDef};
    use std::borrow::Cow;
    let mut catalog = Catalog::builtins();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255; 4])
            .unwrap();
    }
    let texture = catalog
        .register_texture(TextureDef {
            key: Cow::Borrowed("fixture:water_source"),
            png: Cow::Owned(png),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
            emission_strength: 1.0,
            foliage: Default::default(),
        })
        .unwrap();
    let mut light = catalog.block(crate::world::GLOWSTONE).unwrap().clone();
    light.id = BlockTypeId::new(3999);
    light.key = Cow::Borrowed("fixture:water_source");
    light.name = Cow::Borrowed("Uniform water test source");
    light.textures = BlockTextures {
        top: texture,
        side: texture,
        bottom: texture,
    };
    catalog.register_block(light).unwrap();
    let state = crate::world::BlockId::new(3999);
    catalog
        .register_state(state, BlockTypeId::new(3999), vec![], None)
        .unwrap();
    (catalog, state)
}

pub(in crate::render::trace) fn pool(
    catalog: &Catalog,
    emitter: crate::world::BlockId,
    wet: bool,
    blocked: bool,
    roof: bool,
) -> Scene {
    pool_with_bottom(catalog, emitter, wet, blocked, roof, 3)
}
fn pool_with_bottom(
    catalog: &Catalog,
    emitter: crate::world::BlockId,
    wet: bool,
    blocked: bool,
    roof: bool,
    water_bottom: usize,
) -> Scene {
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let mut source = crate::world::Chunk::from_blocks(key, 1, vec![AIR; CHUNK_VOLUME]);
    for z in 0..16 {
        for x in 0..16 {
            source
                .blocks
                .set(crate::world::Chunk::index([x, 0, z]).unwrap(), emitter);
            if blocked {
                source
                    .blocks
                    .set(crate::world::Chunk::index([x, 2, z]).unwrap(), STONE);
            }
            if roof {
                source
                    .blocks
                    .set(crate::world::Chunk::index([x, 12, z]).unwrap(), STONE);
            }
        }
    }
    if roof {
        // A genuine six-sided enclosure, not an overhead card with open sides.
        for y in 1..=12 {
            for z in 0..16 {
                for x in 0..16 {
                    if x == 0 || x == 15 || z == 0 || z == 15 {
                        source
                            .blocks
                            .set(crate::world::Chunk::index([x, y, z]).unwrap(), STONE);
                    }
                }
            }
        }
    }
    if wet {
        for y in water_bottom..6 {
            if blocked && y == 2 {
                continue;
            }
            for z in 4..12 {
                for x in 4..12 {
                    source
                        .blocks
                        .set(crate::world::Chunk::index([x, y, z]).unwrap(), WATER);
                }
            }
        }
    }
    let known = std::collections::HashMap::from([(key, Arc::new(source.clone()))]);
    let light = crate::lighting::LightField::build_with_catalog(key, &known, 1, catalog);
    let mesh = render::mesh::mesh_chunk_lit_with_neighbors(&source, &light, 1, catalog, &known);
    let mut published = Chunk::from_world_mesh(&mesh, catalog, &source);
    // Both roof variants retain identical sky metadata: actual occlusion,
    // not a cached voxel sky gate, must distinguish sealed/open solar paths.
    for triangle in &mut published.triangles {
        triangle.b[3] = 1.0;
    }
    let published = Arc::new(published);
    let mut scene = Scene::build([published.clone()]);
    super::super::super::scene::volume::append(&mut scene, &[published], &[]);
    scene
}

fn run(f: &Fixture, scene: &Scene, mode: u32, sun: bool, width: u32) -> Vec<[f32; 4]> {
    run_shader(f, scene, mode, sun, width, PATHS)
}
fn run_shader(
    f: &Fixture,
    scene: &Scene,
    mode: u32,
    sun: bool,
    width: u32,
    probe: &str,
) -> Vec<[f32; 4]> {
    let mut frame = vec![0.0f32; 76];
    frame[36..40].copy_from_slice(&[0.0, 1.0, 0.0, 1.0]);
    if sun {
        frame[40..43].fill(1.0);
    }
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    frame[69] = f32::from_bits(mode);
    frame[73] = f32::from_bits(scene.water_offset);
    let buffer = |data: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("actual pool transport fixture"),
                contents: if data.is_empty() { &[0u8; 96] } else { data },
                usage,
            })
    };
    let buffers = [
        buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM),
        buffer(
            bytemuck::cast_slice(&scene.nodes),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&scene.triangles),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&scene.coverage),
            wgpu::BufferUsages::STORAGE,
        ),
    ];
    let mut entries = buffers
        .iter()
        .enumerate()
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding: if binding == 3 { 10 } else { binding as u32 },
            resource: buffer.as_entire_binding(),
        })
        .collect::<Vec<_>>();
    f.append_empty_pages(&mut entries);
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &f.layout,
        entries: &entries,
    });
    let source = source(12).replace(FIXTURE, probe).replace(
        "const RAY_CAUSTIC_GUIDE:bool=true;",
        if mode == 36 {
            "const RAY_CAUSTIC_GUIDE:bool=false;"
        } else {
            "const RAY_CAUSTIC_GUIDE:bool=true;"
        },
    );
    let source = source.replace(
        "const RAY_WATER_COMPONENT:u32=0u;",
        match mode {
            50 => "const RAY_WATER_COMPONENT:u32=1u;",
            51 => "const RAY_WATER_COMPONENT:u32=2u;",
            _ => "const RAY_WATER_COMPONENT:u32=0u;",
        },
    );
    super::super::denoise::draw(
        &f.device,
        &f.queue,
        &source,
        width,
        1,
        &[&group, &f.materials, &f.dynamic.group],
        &[
            Some(&f.layout),
            Some(&f.material_layout),
            Some(&f.dynamic.layout),
        ],
    )
}

fn energies(f: &Fixture, scene: &Scene, mode: u32, sun: bool) -> [[f32; 4]; 2] {
    let pixels = run(f, scene, mode, sun, 256);
    let mut means = [[0.0f32; 4]; 2];
    for (i, pixel) in pixels.iter().enumerate() {
        assert_eq!(
            pixel[3], 1.0,
            "real path fragment must finish: {i}: {pixel:?}"
        );
        for (sum, value) in means[i % 2].iter_mut().zip(pixel) {
            *sum += value / 128.0;
        }
    }
    means
}

#[test]
fn gpu_actual_pool_interfaces_refraction_beer_blockers_and_solar_escape() {
    let (catalog, emitter) = catalog();
    let f = Fixture::new(&catalog);
    let wet = pool(&catalog, emitter, true, false, false);
    let inverse = glam::Mat4::from_cols(
        glam::Vec4::X,
        glam::Vec4::Z,
        -glam::Vec4::Y,
        glam::Vec4::new(8.0, 8.0, 8.0, 1.0),
    );
    let complete = super::primary::run_scene(&f, &wet, -2.0, 0.0, 0, inverse, false);
    assert!(
        complete.iter().all(|p| p[0] < -3.0
            && p[1] == 1.0
            && p[2] >= 0.0
            && p[3] == 1.0
            && p.iter().all(|v| v.is_finite())),
        "actual nearest water interface must select whole-path primary radiance, not reflection-only: {complete:?}"
    );
    let dry_scene = pool(&catalog, emitter, false, false, false);
    let marker_without_geometry =
        super::primary::run_scene(&f, &dry_scene, -2.0, 0.0, 0, inverse, false);
    assert!(
        marker_without_geometry
            .iter()
            .all(|p| p[0] > -2.0 && p[0] < 0.0 && p[1] < 0.0 && p[3] < 0.0),
        "water raster marker alone does not invent an interface: {marker_without_geometry:?}"
    );

    let eye = glam::Vec3::splat(8.0);
    let camera =
        glam::camera::rh::proj::directx::perspective(30.0f32.to_radians(), 2.0, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(eye, eye - glam::Vec3::Y, glam::Vec3::Z);
    let camera_air = super::primary::run_scene(
        &f,
        &wet,
        -2.0,
        0.0,
        44,
        super::primary::PrimaryCamera {
            inverse: camera.inverse(),
            eye,
        },
        false,
    );
    assert!(
        camera_air
            .iter()
            .all(|p| p[0] == 1.0 && p[1] == 2.0 && p[2].is_finite()),
        "camera air is integrated exactly once while both water lobes continue: {camera_air:?}"
    );
    let stable = super::primary::run_scene(
        &f,
        &wet,
        -2.0,
        0.0,
        40,
        super::primary::PrimaryCamera {
            inverse: camera.inverse(),
            eye,
        },
        false,
    );
    assert!(
        stable.iter().all(|p| p[3] == 9.0),
        "actual complete-water primary history must accumulate: {stable:?}"
    );
    for invalid in [41, 42, 43] {
        let rejected = super::primary::run_scene(
            &f,
            &wet,
            -2.0,
            0.0,
            invalid,
            super::primary::PrimaryCamera {
                inverse: camera.inverse(),
                eye,
            },
            false,
        );
        assert!(
            rejected.iter().all(|p| p[3] == 1.0),
            "depth/class/normal disocclusion still rejects stale water: mode{invalid}: {rejected:?}"
        );
    }
    assert!(wet.water_offset > 0);
    let boundaries = run(&f, &wet, 30, false, 4);
    assert!(
        (boundaries[0][0] - 2.0).abs() < 0.0001
            && boundaries[0][1] == 1.0
            && boundaries[0][2] == 1.0
    );
    assert!(
        (boundaries[1][0] - 2.0).abs() < 0.0001
            && boundaries[1][2] == 1.0
            && boundaries[1][3] == -1.0
    );
    assert!(
        (boundaries[2][0] - 1.0).abs() < 0.0001
            && boundaries[2][2] == -1.0
            && boundaries[2][3] == 1.0
    );
    assert_eq!(boundaries[3][..3], [1.0, 0.0, -1.0]);
    assert!(
        (boundaries[3][3] - 8.0).abs() < 0.0001,
        "known-distance stops before unloaded chunk, not after a sampled gap: {:?}",
        boundaries[3]
    );
    let dry = energies(&f, &pool(&catalog, emitter, false, false, false), 31, false)[0];
    let transmitted = energies(&f, &wet, 31, false)[0];
    for (actual, expected) in dry[..3].iter().zip([1.0; 3]) {
        assert!(
            (*actual - expected).abs() < 0.001,
            "uniform emitter fixture: {dry:?}"
        );
    }
    let expected = [0.34f32, 0.80, 0.92];
    for (actual, expected) in transmitted[..3].iter().zip(expected) {
        assert!(
            (*actual - expected).abs() < 0.10,
            "actual3m pool must transmit off-screen source with spectral Beer: {transmitted:?}"
        );
    }
    assert!(transmitted[0] < transmitted[1] && transmitted[1] < transmitted[2]);
    let blocked = energies(&f, &pool(&catalog, emitter, true, true, false), 31, false)[0];
    assert!(
        blocked[..3].iter().all(|v| v.abs() < 0.0001),
        "an opaque bottom blocks emitter behind water: {blocked:?}"
    );
    let open = energies(&f, &pool(&catalog, STONE, true, false, false), 32, true);
    let sealed = energies(&f, &pool(&catalog, STONE, true, false, true), 32, true);
    assert!(
        open[1][0] > 0.01,
        "underwater camera reaches real refracted solar source: {open:?}"
    );
    assert!(
        sealed
            .iter()
            .all(|p| p[..3].iter().all(|v| v.abs() < 0.0001)),
        "actual closed roof preserves dark water: {sealed:?}"
    );
}

#[test]
fn gpu_actual_submerged_floor_caustic_guide_preserves_energy_variance_and_occlusion() {
    let catalog = Catalog::builtins();
    let f = Fixture::new(&catalog);
    let open = pool_with_bottom(&catalog, STONE, true, false, false, 1);
    let summarize = |pixels: Vec<[f32; 4]>| {
        let samples = pixels
            .iter()
            .skip(1)
            .step_by(2)
            .map(|p| p[2])
            .collect::<Vec<_>>();
        assert!(samples.iter().all(|v| v.is_finite() && *v >= 0.0));
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let variance =
            samples.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / samples.len() as f32;
        (mean, variance, samples.len())
    };
    // Isolate the real decoded floor's diffuse component. Phase and GGX
    // downstream proposals are identical in both controls; initial selection
    // cannot let their rare bright paths masquerade as a diffuse-energy change.
    let old = summarize(run(&f, &open, 62, true, 2048));
    let guided = summarize(run(&f, &open, 63, true, 2048));
    println!("actual submerged caustic blue energy old={old:?},guided={guided:?}");
    let standard_error = ((old.1 + guided.1) / old.2 as f32).sqrt();
    assert!(old.0 > 0.0 && guided.0 > 0.0);
    assert!(
        (old.0 - guided.0).abs() < 5.0 * standard_error + 0.0002,
        "proposal changes preserve actual pool radiance: old={old:?},guided={guided:?}"
    );
    assert!(
        guided.1 < old.1 * 0.75,
        "guiding must reduce actual pool variance: old={old:?},guided={guided:?}"
    );
    let queried = run(&f, &open, 38, true, 1);
    assert_eq!(queried[0][0], 1.0, "actual water boundary enables proposal");
    let blocked = pool_with_bottom(&catalog, STONE, true, true, false, 1);
    assert_eq!(
        run(&f, &blocked, 38, true, 1)[0][0],
        0.0,
        "first opaque occluder disables guidance"
    );
    let mut unknown = pool_with_bottom(&catalog, STONE, true, false, false, 1);
    unknown.coverage[..8].fill(0);
    unknown.water_offset = 0;
    assert_eq!(
        run(&f, &unknown, 38, true, 1)[0][0],
        0.0,
        "unknown fluid state never fabricates guidance"
    );
    let sealed = pool_with_bottom(&catalog, STONE, true, false, true, 1);
    let darkness = run(&f, &sealed, 63, true, 128);
    assert!(
        darkness.iter().all(|p| p[..3].iter().all(|v| *v == 0.0)),
        "guided proposal cannot illuminate a real six-sided sealed underwater room: {darkness:?}"
    );
}

#[test]
fn gpu_water_phase_diagnostic_components_preserve_full_radiance_and_rng() {
    let catalog = Catalog::builtins();
    let f = Fixture::new(&catalog);
    let scene = pool_with_bottom(&catalog, STONE, true, false, false, 1);
    let full = run(&f, &scene, 49, true, 256);
    let unscattered = run(&f, &scene, 50, true, 256);
    let scattered = run(&f, &scene, 51, true, 256);
    let mut scattered_energy = 0.0;
    for ((all, plain), phase) in full.iter().zip(&unscattered).zip(&scattered) {
        for channel in 0..3 {
            assert!(
                all[channel].is_finite()
                    && plain[channel].is_finite()
                    && phase[channel].is_finite()
            );
            assert!(
                (all[channel] - plain[channel] - phase[channel]).abs()
                    < all[channel].abs() * 0.00002 + 0.00002,
                "unchanged RNG/queries: linear diagnostic components add to exact full path: all={all:?},plain={plain:?},phase={phase:?}"
            );
            scattered_energy += phase[channel];
        }
    }
    assert!(
        scattered_energy > 0.0,
        "real water-medium collisions must appear in their diagnostic component"
    );
}

#[test]
fn gpu_actual_pool_phase_and_ggx_guides_preserve_energy_and_coverage() {
    let catalog = Catalog::builtins();
    let f = Fixture::new(&catalog);
    let open = pool_with_bottom(&catalog, STONE, true, false, false, 1);
    for (family, baseline, guided) in [("phase", 56, 57), ("GGX", 58, 59)] {
        let summarize = |pixels: Vec<[f32; 4]>| {
            assert!(
                pixels
                    .iter()
                    .all(|p| p.iter().all(|v| v.is_finite()) && p[2] >= 0.0)
            );
            let mean = pixels.iter().map(|p| p[2] as f64).sum::<f64>() / pixels.len() as f64;
            let variance = pixels
                .iter()
                .map(|p| (p[2] as f64 - mean).powi(2))
                .sum::<f64>()
                / pixels.len() as f64;
            (mean, variance, pixels.len())
        };
        let old = summarize(run(&f, &open, baseline, true, 1024));
        let new = summarize(run(&f, &open, guided, true, 1024));
        let standard_error = ((old.1 + new.1) / old.2 as f64).sqrt();
        println!("actual pool guided {family} old={old:?} new={new:?} stderr={standard_error}");
        assert!(old.0 > 0.0 && new.0 > 0.0);
        assert!(
            (old.0 - new.0).abs() < 5.0 * standard_error + 0.00002,
            "actual unchanged path energy: {family} {old:?} {new:?}"
        );
        assert!(
            new.1 < old.1 * 0.75,
            "material actual variance improvement: {family} {old:?} {new:?}"
        );
        let queried = run(&f, &open, guided, true, 1);
        assert_eq!(queried[0][3], 1.0);
        let blocked = pool_with_bottom(&catalog, STONE, true, true, false, 1);
        assert_eq!(
            run(&f, &blocked, guided, true, 1)[0][3],
            0.0,
            "opaque first boundary disables {family} guidance"
        );
        let mut unknown = pool_with_bottom(&catalog, STONE, true, false, false, 1);
        unknown.coverage[..8].fill(0);
        unknown.water_offset = 0;
        assert_eq!(
            run(&f, &unknown, guided, true, 1)[0][3],
            0.0,
            "unknown fluid disables {family} guidance"
        );
        let sealed = pool_with_bottom(&catalog, STONE, true, false, true, 1);
        assert!(
            run(&f, &sealed, guided, true, 128)
                .iter()
                .all(|p| p[..3].iter().all(|v| *v == 0.0)),
            "guided {family} cannot illuminate a genuine closed underwater room"
        );
    }
}
