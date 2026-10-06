//! A complete secondary path reaches an admitted emitter behind the camera.
use super::*;
use crate::render::trace::scene::{pages, surface};

const PATH: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 ray_rng=u32(pixel.x)*1973u+991u;
 var sum=vec3f(0.0);
 for(var i=0u;i<256u;i++) {sum+=transport(vec3f(0.0),vec3f(0.0,0.0,1.0),0.0);}
 return vec4f(sum/256.0,1.0);
}
"#;
fn metal_uv() -> [f32; 2] {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../../../../../assets/textures/blocks/jg_deepslate_iron_ore_s.png"
    )));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let pixel = bytes[..info.buffer_size()]
        .chunks_exact(4)
        .position(|p| p[1] >= 230)
        .expect("actual iron ore must contain preset metal");
    [
        (pixel % info.width as usize) as f32 / info.width as f32 + 0.5 / info.width as f32,
        (pixel / info.width as usize) as f32 / info.height as f32 + 0.5 / info.height as f32,
    ]
}
fn run(f: &Fixture, near: &Scene, distant: &[Scene]) -> Vec<[f32; 4]> {
    let mut frame = [0.0f32; 76];
    frame[36..40].copy_from_slice(&[0.0, 1.0, 0.0, 1.0]);
    frame[68] = f32::from_bits(near.nodes.len() as u32);
    let buffer = |data: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("offscreen integrated transport"),
                contents: if data.is_empty() { &[0; 96] } else { data },
                usage,
            })
    };
    let buffers = [
        buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM),
        buffer(
            bytemuck::cast_slice(&near.nodes),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&near.triangles),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&near.coverage),
            wgpu::BufferUsages::STORAGE,
        ),
    ];
    let distant: [wgpu::Buffer; pages::MAX_PAGES] = std::array::from_fn(|index| {
        let words = distant
            .get(index)
            .map_or_else(|| vec![0; 4], pages::packed_words);
        buffer(bytemuck::cast_slice(&words), wgpu::BufferUsages::STORAGE)
    });
    let mut entries: Vec<_> = buffers
        .iter()
        .enumerate()
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding: if binding == 3 { 10 } else { binding as u32 },
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend(
        distant
            .iter()
            .enumerate()
            .map(|(index, buffer)| wgpu::BindGroupEntry {
                binding: 11 + index as u32,
                resource: buffer.as_entire_binding(),
            }),
    );
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &f.layout,
        entries: &entries,
    });
    super::super::denoise::draw(
        &f.device,
        &f.queue,
        &source(12).replace(FIXTURE, PATH),
        4,
        1,
        &[&group, &f.materials, &f.dynamic.group],
        &[
            Some(&f.layout),
            Some(&f.material_layout),
            Some(&f.dynamic.layout),
        ],
    )
}
#[test]
fn gpu_complete_transport_reflects_offscreen_distant_emitter_and_obeys_near_blocker() {
    let catalog = Catalog::builtins();
    let id = |stem: &str| {
        catalog
            .textures()
            .iter()
            .position(|t| t.key.ends_with(&format!(":{stem}")))
            .unwrap() as u32
    };
    let f = Fixture::new(&catalog);
    let mirror = plane(
        -2.0,
        2.0,
        1.0,
        id("jg_deepslate_iron_ore"),
        metal_uv(),
        false,
    );
    let near = Scene::build([Arc::new(Chunk {
        triangles: mirror.clone(),
        ..Default::default()
    })]);
    // This source is behind the camera and cannot appear in an SSR color buffer.
    let distant = plane(-64.0, 64.0, -2.0, 0, [0.5; 2], false)
        .into_iter()
        .map(|mut t| {
            t.normal[3] = -1.0;
            t.with_surface(
                [1.0; 4],
                surface::LOD
                    | surface::COARSE_COLOR
                    | surface::NO_WIND
                    | (15 << surface::GLOW_SHIFT),
            )
        })
        .collect();
    let pages = pages::build(
        [Arc::new(Chunk {
            triangles: distant,
            ..Default::default()
        })],
        f.device.limits().max_storage_buffer_binding_size,
    )
    .unwrap();
    let lit = run(&f, &near, &pages);
    for pixel in &lit {
        assert!(
            pixel[..3].iter().all(|v| v.is_finite() && *v > 0.01),
            "behind-camera LOD source contributed no reflected radiance: {lit:?}"
        );
    }
    assert!(
        run(&f, &near, &[]).iter().all(|p| p[..3] == [0.0; 3]),
        "missing pages invented lighting"
    );
    let mut blocked = mirror;
    blocked.extend(plane(-64.0, 64.0, -1.0, id("stone"), [0.5; 2], false));
    let blocked = Scene::build([Arc::new(Chunk {
        triangles: blocked,
        ..Default::default()
    })]);
    assert!(
        run(&f, &blocked, &pages).iter().all(|p| p[..3] == [0.0; 3]),
        "near geometry failed to hide distant emitter"
    );
}
