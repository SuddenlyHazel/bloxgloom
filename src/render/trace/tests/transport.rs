//! Real material arrays, immutable production BVH, and production path WGSL.
use super::super::scene::{Chunk, Scene, Triangle};
use crate::{content::Catalog, render};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const FIXTURE: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let index=u32(pixel.x);ray_rng=index*1973u+911u;
 let mode=ray_frame.counts.y;
 if mode>=19u&&mode<=21u {
  let energy=transport(vec3f(0.0),vec3f(0.0,0.0,1.0),1.0);
  return vec4f(energy,f32(test_vertex_count));
 }
 if mode==17u||mode==18u {
  let origin=select(vec3f(0.0),vec3f(-5.0,-3.0,-4.0),index==1u);
  return vec4f(sun_light_visible(origin,1.0),1.0);
 }
 if mode==12u||mode==16u {
  let direction=select(vec3f(0.0,1.0,0.0),normalize(vec3f(0.1,1.0,0.0)),index==1u);
  return vec4f(transport(vec3f(8.0),direction,0.0),1.0);
 }
 if mode==13u {return vec4f(sun_light_visible(vec3f(8.0),0.0),1.0);}
 if mode==14u {
  let directions=array<vec3f,4>(vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0),vec3f(1.0,0.00001,0.0),normalize(vec3f(1.0,0.5,0.0)));
  return vec4f(select(0.0,1.0,ray_certified_sky(vec3f(8.0),directions[index])),0.0,0.0,1.0);
 }
 if mode==11u {
  let case_index=index/4u;let variant=index%4u;
  let angle=case_index%6u;
  let cosines=array<f32,6>(1.0,0.5,0.0,-1.0,-0.5,1.0);
  let cosine=cosines[angle];let sun=vec4f(sqrt(1.0-cosine*cosine),cosine,0.0,1.0);
  let n=vec3f(0.0,1.0,0.0);let color=vec3f(0.6,0.36,0.34);
  let pbr=bg_decode_pbr(vec4f(0.2,0.0,0.0,0.0),color,false,true);
  let optics=bg_foliage_optics(color,0.653,4u<<27u);
  let botanical=case_index>=6u;
  let sky=select(1.0,0.0,angle==5u);
  let surface=RaySurface(color,pbr,vec3f(0.0),sky,select(0.0,optics.share,botanical),
   select(color,optics.reflected,botanical),select(vec3f(0.0),optics.transmitted,botanical));
  let solar=bg_sun_irradiance();
  let weight=bg_pbr_diffuse_weight(pbr,1.0);
  var primary=color*bg_direct_light(n,sun,sky)*weight;
  if botanical {primary=bg_foliage_optical_direct(n,n,n,sun,sky,0.0,optics,solar)*weight;}
  if variant==0u {return vec4f(primary,1.0);}
  if variant==1u {return vec4f(ray_diffuse_direct(n,n,sun.xyz,surface,solar*sky),1.0);}
  if variant==2u {return vec4f(bg_thin_scattering(n,n,n,sun,sky,0.0,0.0,solar)*color*weight,1.0);}
  return vec4f(solar,1.0);
 }
 if mode==7u {return vec4f(sun_light(vec3f(0.0)),1.0);}
 if mode==8u {
  let hit=ray_cast(vec3f(0.0),vec3f(0.0,0.0,1.0),512.0);
  let sheet=ray_surface(hit);
  return vec4f(sheet.albedo*sheet.transmission*(vec3f(1.0)-sheet.pbr.f0)*ray_frame.solar.xyz,1.0);
 }
 if mode==9u||mode==10u {
  let hit=ray_cast(vec3f(0.0),vec3f(0.0,0.0,1.0),512.0);
  let sheet=ray_surface(hit);
  if mode==10u {return vec4f((sheet.reflection+sheet.transmittance)*(vec3f(1.0)-sheet.pbr.f0),sheet.pbr.f0.r);}
  var energy=vec3f(0.0);
  for(var sample_index=0u;sample_index<64u;sample_index++) {
   energy+=scatter(hit.normal,hit.normal,sheet).weight;
  }
  return vec4f(energy/64.0,1.0);
 }
 if mode==0u {
  let direction=select(vec3f(0.0,0.0,1.0),vec3f(1.0,0.0,0.0),index==1u);
  let hit=ray_cast(vec3f(0.0),direction,512.0);
  return vec4f(hit.distance,select(0.0,1.0,hit.triangle!=0xffffffffu),0.0,1.0);
 }
 if mode==1u {
  let x=select(-1.0,1.0,index==1u);
  let hit=ray_cast(vec3f(x,0.0,0.0),vec3f(0.0,0.0,1.0),512.0);
  return vec4f(hit.distance,select(0.0,1.0,hit.triangle!=0xffffffffu),0.0,1.0);
 }
 var sum=vec3f(0.0);
 for(var sample_index=0u;sample_index<64u;sample_index++) {
  if mode==2u {
   sum+=transport(vec3f(0.0),vec3f(0.0,0.0,1.0),select(0.0,1.0,index==1u));
  } else if mode==3u {
   sum+=transport(vec3f(0.0),vec3f(0.0,0.0,1.0),0.0);
  } else {
   let foreground=select(vec3f(0.0),vec3f(1.0),mode==5u);
   sum+=ray_primary_medium(vec3f(0.0),vec3f(0.0,0.0,1.0),30.0,foreground,vec3f(0.0));
  }
 }
 return vec4f(sum/64.0,1.0);
}
"#;

fn source(bounces: u32) -> String {
    let transport = include_str!("../transport.wgsl")
        .replace("bounce<12u", &format!("bounce<{bounces}u"))
        // Controlled first-scatter fixture: retain actual geometry, production
        // integrator and HG scattering, while positioning the medium event past
        // a verified loaded-space boundary. Closed rooms still hit walls first.
        .replace("let event=medium_event(origin,direction,hit.distance);",
            "var event=medium_event(origin,direction,hit.distance); if ray_frame.counts.y==16u && bounce==0u {event=140.0;}")
        // Count real path vertices and control only the stochastic scatter for
        // exact-zero/tiny-positive energy fixtures. Production hit/shading,
        // direct/emission accumulation and zero termination remain untouched.
        .replace("let hit=ray_cast(origin,direction,512.0);\n        var event=",
            "test_vertex_count++;\n        let hit=ray_cast(origin,direction,512.0);\n        var event=")
        .replace("let sample=scatter(hit.normal,-direction,surface);",
            "var sample=scatter(hit.normal,-direction,surface); if ray_frame.counts.y>=19u && ray_frame.counts.y<=21u {sample=RayScatter(vec3f(0.0,0.0,1.0),vec3f(0.0)); if ray_frame.counts.y==21u && bounce==0u {sample.weight=vec3f(1e-20);}}")
        .replace("var origin=start;", "test_transport_count++;var origin=start;")
        .replace("let sample=scatter(n,-direction,surface);",
            "var sample=scatter(n,-direction,surface); if ray_frame.counts.y==22u||ray_frame.counts.y==23u {sample=RayScatter(vec3f(0.0,0.0,1.0),vec3f(select(0.0,1e-20,ray_frame.counts.y==23u)));}");
    format!(
        "var<private> test_vertex_count:u32;\nvar<private> test_transport_count:u32;\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{transport}\n{}\n{}\n{}\n{}\n{FIXTURE}",
        render::sky::environment_shader(),
        include_str!("../../material/pbr.wgsl"),
        include_str!("../../material/foliage.wgsl"),
        include_str!("../../material/foliage_optics.wgsl"),
        include_str!("../intersection.wgsl"),
        include_str!("../coverage.wgsl"),
        include_str!("../denoise.wgsl"),
        include_str!("../medium.wgsl"),
        include_str!("../../daylight/test_camera.wgsl"),
        include_str!("../../daylight.wgsl"),
        include_str!("../../material/relief.wgsl"),
    )
}

#[test]
fn production_transport_fixture_validates() {
    for bounces in [2, 12] {
        let module = wgpu::naga::front::wgsl::parse_str(&source(bounces)).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

fn quad(points: [[f32; 3]; 4], material: u32, uv: [f32; 2], cutout: bool) -> Vec<Triangle> {
    [[0, 1, 2], [0, 2, 3]]
        .into_iter()
        .map(|indices| {
            let [a, b, c] = indices.map(|index| points[index]);
            let n = (glam::Vec3::from_array(b) - glam::Vec3::from_array(a))
                .cross(glam::Vec3::from_array(c) - glam::Vec3::from_array(a))
                .normalize();
            Triangle {
                a: [a[0], a[1], a[2], material as f32],
                b: [b[0], b[1], b[2], 0.0],
                c: [c[0], c[1], c[2], f32::from(cutout)],
                uv_ab: [uv[0], uv[1], uv[0], uv[1]],
                uv_c: [uv[0], uv[1], 0.0, 0.0],
                normal: [n.x, n.y, n.z, 0.0],
            }
        })
        .collect()
}

fn plane(x0: f32, x1: f32, z: f32, material: u32, uv: [f32; 2], cutout: bool) -> Vec<Triangle> {
    quad(
        [[x0, -2.0, z], [x1, -2.0, z], [x1, 2.0, z], [x0, 2.0, z]],
        material,
        uv,
        cutout,
    )
}

fn alpha_samples(png: &[u8]) -> [[f32; 2]; 2] {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let image = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(image.color_type, png::ColorType::Rgba);
    let sample = |opaque| {
        for y in 1..image.height - 1 {
            for x in 1..image.width - 1 {
                if (-1..=1).all(|dy| {
                    (-1..=1).all(|dx| {
                        let index =
                            (((y as i32 + dy) as u32 * image.width + (x as i32 + dx) as u32) * 4
                                + 3) as usize;
                        if opaque {
                            bytes[index] == 255
                        } else {
                            bytes[index] == 0
                        }
                    })
                }) {
                    return [
                        (x as f32 + 0.5) / image.width as f32,
                        (y as f32 + 0.5) / image.height as f32,
                    ];
                }
            }
        }
        panic!("builtin leaf must contain a solid transparent/opaque alpha neighborhood");
    };
    [sample(false), sample(true)]
}

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    materials: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    layout: wgpu::BindGroupLayout,
}
impl Fixture {
    fn new(catalog: &Catalog) -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let limits = render::material::resources::required_limits(
            adapter.limits(),
            render::material::texture_layers_for(catalog) as usize,
        )
        .unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: limits,
            ..Default::default()
        }))
        .unwrap();
        let (pipeline, _, _, _, materials) = render::pipeline::create_voxel_pipeline_with_catalog(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba32Float,
            catalog,
        )
        .unwrap();
        let material_layout = pipeline.get_bind_group_layout(1);
        let entries = (0..3)
            .chain(std::iter::once(10))
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("production transport fixture inputs"),
            entries: &entries,
        });
        Self {
            device,
            queue,
            materials,
            material_layout,
            layout,
        }
    }
    fn run(
        &self,
        triangles: Vec<Triangle>,
        mode: u32,
        density: f32,
        bounces: u32,
        width: u32,
    ) -> Vec<[f32; 4]> {
        self.run_loaded(triangles, mode, density, bounces, width, vec![])
    }
    fn run_loaded(
        &self,
        triangles: Vec<Triangle>,
        mode: u32,
        density: f32,
        bounces: u32,
        width: u32,
        keys: Vec<crate::world::ChunkKey>,
    ) -> Vec<[f32; 4]> {
        let mut chunks = vec![Arc::new(Chunk {
            triangles,
            key: None,
        })];
        chunks.extend(keys.into_iter().map(|key| {
            Arc::new(Chunk {
                key: Some(key),
                triangles: vec![],
            })
        }));
        let scene = Scene::build(chunks);
        let mut frame = vec![0.0f32; 72];
        frame[36..40].copy_from_slice(&[0.0, 1.0, 0.0, f32::from(mode == 4)]);
        if matches!(mode, 7 | 8 | 17 | 18) {
            frame[36..40].copy_from_slice(&[0.0, 0.0, 1.0, 1.0]);
        }
        if mode == 19 {
            frame[36..40].copy_from_slice(&[0.0, 0.0, -1.0, 1.0]);
        }
        frame[40..44].copy_from_slice(&[1.0, 0.8, 0.6, 0.0]);
        if matches!(mode, 16 | 20 | 21) {
            frame[40..44].fill(0.0);
        }
        frame[44..48].copy_from_slice(&[0.1, 0.2, 0.3, 0.0]);
        frame[48..52].copy_from_slice(&[0.2, 0.3, 0.5, 1.0]);
        frame[60] = density;
        frame[63] = f32::from(mode == 18);
        frame[68] = f32::from_bits(scene.nodes.len() as u32);
        frame[69] = f32::from_bits(mode);
        let buffer = |data: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: if data.is_empty() { &[0u8; 96] } else { data },
                    usage,
                })
        };
        let uniform = buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM);
        let nodes = buffer(
            bytemuck::cast_slice(&scene.nodes),
            wgpu::BufferUsages::STORAGE,
        );
        let triangles = buffer(
            bytemuck::cast_slice(&scene.triangles),
            wgpu::BufferUsages::STORAGE,
        );
        let coverage = buffer(
            bytemuck::cast_slice(&scene.coverage),
            wgpu::BufferUsages::STORAGE,
        );
        let buffers = [&uniform, &nodes, &triangles, &coverage];
        let entries = buffers
            .iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: if binding == 3 { 10 } else { binding as u32 },
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &entries,
        });
        super::denoise::draw(
            &self.device,
            &self.queue,
            &source(bounces),
            width,
            1,
            &[&group, &self.materials],
            &[Some(&self.layout), Some(&self.material_layout)],
        )
    }
}

#[test]
fn gpu_transport_hits_hidden_geometry_passes_leaf_alpha_and_preserves_dark_caves() {
    let catalog = Catalog::builtins();
    let texture = |stem: &str| {
        catalog
            .textures()
            .iter()
            .position(|texture| texture.key.ends_with(&format!(":{stem}")))
            .unwrap() as u32
    };
    let stone = texture("stone");
    let leaves = texture("jg_oak_leaves");
    let fixture = Fixture::new(&catalog);
    let hits = fixture.run(plane(-2.0, 2.0, 3.0, stone, [0.5; 2], false), 0, 0.0, 12, 2);
    assert!((hits[0][0] - 3.0).abs() < 0.001 && hits[0][1] == 1.0);
    assert_eq!(hits[1][1], 0.0);
    let [transparent, opaque] = alpha_samples(&catalog.textures()[leaves as usize].png);
    let mut cutouts = plane(-2.0, 0.0, 1.0, leaves, transparent, true);
    cutouts.extend(plane(0.0, 2.0, 1.0, leaves, opaque, true));
    cutouts.extend(plane(-2.0, 2.0, 2.0, stone, [0.5; 2], false));
    let hits = fixture.run(cutouts, 1, 0.0, 12, 2);
    assert!(
        (hits[0][0] - 2.0).abs() < 0.001,
        "transparent leaf ray={hits:?}"
    );
    assert!((hits[1][0] - 1.0).abs() < 0.05, "opaque leaf ray={hits:?}");
    // Exercise production sun visibility with real source material maps. A
    // second thin sheet applies optical absorption, never opaque diffuse color.
    let cherry = texture("jg_cherry_leaves");
    let cherry_uv = alpha_samples(&catalog.textures()[cherry as usize].png)[1];
    let one_sheet = plane(-2.0, 2.0, 1.0, cherry, cherry_uv, true);
    let mut two_sheets = one_sheet.clone();
    two_sheets.extend(plane(-2.0, 2.0, 2.0, cherry, cherry_uv, true));
    let clear = fixture.run(vec![], 7, 0.0, 12, 1)[0];
    let one = fixture.run(one_sheet.clone(), 7, 0.0, 12, 1)[0];
    let two = fixture.run(two_sheets.clone(), 7, 0.0, 12, 1)[0];
    let opaque_proxy = fixture.run(one_sheet, 8, 0.0, 12, 1)[0];
    for channel in 0..3 {
        assert!(one[channel] > opaque_proxy[channel]);
        assert!(one[channel] > two[channel] && one[channel] < clear[channel]);
        assert!(
            (two[channel] - one[channel] * one[channel] / clear[channel]).abs() < 0.0001,
            "independent sheet transmission must multiply: clear={clear:?}, one={one:?}, two={two:?}"
        );
    }
    let sheet = plane(-2.0, 2.0, 1.0, cherry, cherry_uv, true);
    let allocation = fixture.run(sheet.clone(), 10, 0.0, 12, 1)[0];
    let samples = fixture.run(sheet, 9, 0.0, 12, 64);
    for channel in 0..3 {
        let energy = samples.iter().map(|pixel| pixel[channel]).sum::<f32>() / 64.0;
        assert!(
            energy >= allocation[channel] - 0.025
                && energy <= allocation[channel] + allocation[3] + 0.025,
            "production BSDF must retain its bounded R/T allocation: channel={channel}, energy={energy}, allocation={allocation:?}"
        );
    }
    two_sheets.extend(plane(-2.0, 2.0, 3.0, stone, [0.5; 2], false));
    assert_eq!(&fixture.run(two_sheets, 7, 0.0, 12, 1)[0][..3], &[0.0; 3]);
    let sky = fixture.run(vec![], 2, 0.0, 12, 2);
    assert_eq!(&sky[0][..3], &[0.0; 3]);
    assert!(sky[1][0] > 0.01 && sky[1][2] > sky[1][0]);

    // A closed box has no environment/direct light. The first hit is nonemissive;
    // reaching its emissive ceiling requires a scattered surface path.
    let glow = texture("glowstone");
    // Pale walls give later bounces enough energy for a stable regression margin.
    let wall = texture("jg_quartz_block_side");
    let mut box_triangles = plane(-1.0, 1.0, 1.0, wall, [0.5; 2], false);
    box_triangles.extend(plane(-1.0, 1.0, -1.0, wall, [0.5; 2], false));
    for (axis, side, material) in [
        (0, -1.0, wall),
        (0, 1.0, wall),
        (1, -2.0, wall),
        (1, 2.0, glow),
    ] {
        let points = if axis == 0 {
            [
                [side, -2.0, -1.0],
                [side, 2.0, -1.0],
                [side, 2.0, 1.0],
                [side, -2.0, 1.0],
            ]
        } else {
            [
                [-1.0, side, -1.0],
                [1.0, side, -1.0],
                [1.0, side, 1.0],
                [-1.0, side, 1.0],
            ]
        };
        box_triangles.extend(quad(points, material, [0.5; 2], false));
    }
    let mean = |pixels: Vec<[f32; 4]>| {
        pixels.iter().map(|pixel| pixel[0]).sum::<f32>() / pixels.len() as f32
    };
    let mut sealed = box_triangles.clone();
    for triangle in &mut sealed {
        triangle.a[3] = wall as f32;
    }
    assert_eq!(mean(fixture.run(sealed, 3, 0.0, 12, 2)), 0.0);
    let two = mean(fixture.run(box_triangles.clone(), 3, 0.0, 2, 64));
    let many = mean(fixture.run(box_triangles, 3, 0.0, 12, 64));
    assert!(
        two > 0.001,
        "emission must survive a diffuse surface bounce: {two}"
    );
    assert!(
        many > two * 1.05,
        "later surface bounces must contribute: two={two}, twelve={many}"
    );
    let vacuum = mean(fixture.run(vec![], 4, 0.0, 12, 64));
    let medium = mean(fixture.run(vec![], 4, 0.01, 12, 64));
    assert_eq!(vacuum, 0.0);
    assert!(
        medium.is_finite() && medium > 0.005,
        "participating medium must scatter sunlight: {medium}"
    );
    let illuminated = fixture.run(vec![], 5, 0.01, 12, 2);
    let black = fixture.run(vec![], 6, 0.01, 12, 2);
    let expected_extinction = 1.0 - (-0.3f32).exp();
    for (illuminated, black) in illuminated.iter().zip(black) {
        for channel in 0..3 {
            assert!(
                (illuminated[channel] - black[channel] + expected_extinction).abs() < 0.0001,
                "camera-segment extinction must obey Beer-Lambert, color={illuminated:?}, black={black:?}"
            );
        }
    }
}

mod primary;

mod medium;

mod radiometry;

mod sky_escape;

#[path = "transport/opaque_visibility.rs"]
mod opaque_visibility;
