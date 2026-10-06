//! Actual optional traversal, source textures and bounded-overflow reference.
use super::*;
use crate::render::trace::scene::Node;

const CHECK: &str = r#"
var<private> test_overflow:u32;
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 let k=u32(p.x);let x=select(-1.0,1.0,(k&1u)!=0u);
 let limits=array<f32,6>(512.0,512.0,1.0,2.0,3.0,512.0);
 let direction=select(vec3f(0.0,0.0,1.0),vec3f(0.0,0.0,-1.0),k==5u);
 let origin=vec3f(x,0.0,0.0);
 let reference=ray_cast_stackless(origin,direction,limits[k]);
 let candidate=ray_cast(origin,direction,limits[k]);
 let equal=reference.distance==candidate.distance&&reference.triangle==candidate.triangle
  &&all(reference.uv==candidate.uv)&&all(reference.normal==candidate.normal);
 return vec4f(candidate.distance,f32(candidate.triangle),select(1.0,0.0,equal),f32(test_overflow));
}
"#;
fn shader() -> String {
    source(12)
        .replace(
            "const RAY_NEAR_FIRST:bool=false;",
            "const RAY_NEAR_FIRST:bool=true;",
        )
        .replace(FIXTURE, CHECK)
        .replace(
            "if length==16u {return ray_cast_stackless(origin,direction,limit);}",
            "if length==16u {test_overflow++;return ray_cast_stackless(origin,direction,limit);}",
        )
}
fn node(first: u32, count: u32, escape: u32, near: f32) -> Node {
    Node {
        min: [-4.0, -4.0, near],
        max: [4.0, 4.0, 4.0],
        first,
        count,
        escape,
        padding: [0; 3],
    }
}
fn run(f: &Fixture, scene: &Scene, seconds: f32, predeformed: bool) -> Vec<[f32; 4]> {
    let mut frame = [0f32; 76];
    frame[35] = seconds;
    frame[61] = f32::from(predeformed);
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    let buffer = |bytes: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: if bytes.is_empty() { &[0u8; 96] } else { bytes },
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
    let coverage = buffer(&[0u8; 48], wgpu::BufferUsages::STORAGE);
    let buffers = [&uniform, &nodes, &triangles, &coverage];
    let mut entries = buffers
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: if i == 3 { 10 } else { i as u32 },
            resource: b.as_entire_binding(),
        })
        .collect::<Vec<_>>();
    f.append_empty_pages(&mut entries);
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &f.layout,
        entries: &entries,
    });
    let result = super::super::denoise::draw(
        &f.device,
        &f.queue,
        &shader(),
        6,
        1,
        &[&group, &f.materials, &f.dynamic.group],
        &[
            Some(&f.layout),
            Some(&f.material_layout),
            Some(&f.dynamic.layout),
        ],
    );
    assert!(
        result.iter().all(|v| v[2] == 0.0),
        "nearest identity/UV/normal mismatch: {result:?}"
    );
    result
}
#[test]
fn optional_near_first_fixture_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&shader()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_near_first_preserves_alpha_wind_finite_segments_ties_and_stack_overflow() {
    let catalog = Catalog::builtins();
    let texture = |key: &str| {
        catalog
            .textures()
            .iter()
            .position(|t| t.key.ends_with(&format!(":{key}")))
            .unwrap() as u32
    };
    let stone = texture("stone");
    let leaf = texture("jg_oak_leaves");
    let [transparent, opaque] = alpha_samples(&catalog.textures()[leaf as usize].png);
    let f = Fixture::new(&catalog);
    // Right child has a closer AABB than left: traversal order actually changes.
    let mut triangles = plane(-2.0, 2.0, 3.0, stone, [0.5; 2], false);
    triangles.extend(plane(-2.0, 0.0, 1.0, leaf, transparent, true));
    triangles.extend(plane(0.0, 2.0, 1.0, leaf, opaque, true));
    let scene = Scene {
        water_offset: 0,

        nodes: vec![node(0, 0, 3, 0.0), node(0, 2, 2, 0.2), node(2, 4, 3, 0.0)],
        triangles,
        coverage: vec![],
    };
    let mut moving_hits = Vec::new();
    for seconds in [0.0, 17.25] {
        let actual = run(&f, &scene, seconds, false);
        assert!(
            (actual[0][0] - 3.0).abs() < 0.0001,
            "transparent alpha must pass leaf: {actual:?}"
        );
        assert!(
            (actual[1][0] - 1.0).abs() < 0.04,
            "solid alpha must hit wind-deformed leaf: {actual:?}"
        );
        assert_eq!(actual[5][0], 512.0, "reverse segment misses");
        moving_hits.push(actual);
    }
    assert!(
        (moving_hits[0][1][0] - moving_hits[1][1][0]).abs() > 0.0001,
        "wind must alter the accepted sheet distance"
    );
    // Independently bake the leaf positions as the production compute pass
    // does, then disable per-intersection deformation. Both paths must agree.
    let mut baked = Scene {
        water_offset: 0,

        nodes: scene.nodes.clone(),
        triangles: scene.triangles.clone(),
        coverage: vec![],
    };
    for triangle in &mut baked.triangles[2..] {
        for position in [&mut triangle.a, &mut triangle.b, &mut triangle.c] {
            let phase = position[0] * 0.48 + position[2] * 0.31;
            let time = 17.25 * (std::f32::consts::TAU / 128.0);
            let gust =
                (time * 17.0 + phase).sin() * 0.72 + (time * 29.0 + phase * 1.37).sin() * 0.28;
            position[0] += gust * 0.035;
            position[1] += 0.12 * gust * 0.035;
            position[2] += (time * 17.0 + phase + 1.2).sin() * 0.65 * 0.035;
        }
    }
    let baked_hits = run(&f, &baked, 17.25, true);
    for (raw, baked) in moving_hits[1].iter().zip(&baked_hits) {
        assert!(
            (raw[0] - baked[0]).abs() < 0.00001 && raw[1] == baked[1],
            "CPU-baked wind disagrees with shader: raw={raw:?},baked={baked:?}"
        );
    }
    let mut triangles = plane(-2.0, 2.0, 2.0, stone, [0.2; 2], false);
    triangles.extend(plane(-2.0, 2.0, 2.0, stone, [0.8; 2], false));
    let mut ties = Scene {
        water_offset: 0,

        nodes: scene.nodes.clone(),
        triangles,
        coverage: vec![],
    };
    ties.nodes[2].count = 2;
    let hits = run(&f, &ties, 0.0, true);
    assert!(
        hits[0][1] < 2.0 && hits[1][1] < 2.0,
        "earlier triangle wins reversed exact tie"
    );
    assert_eq!(hits[2][0], 1.0);
    assert_eq!(hits[3][0], 2.0);
    assert_eq!(hits[2][1], u32::MAX as f32);
    assert_eq!(
        hits[3][1],
        u32::MAX as f32,
        "hit at exact segment end excluded"
    );
    // A deliberately pathological overlapping tree exceeds sixteen pending
    // siblings. Its nearest acceptance must survive restarting the reference.
    fn deep(nodes: &mut Vec<Node>, triangles: &mut Vec<Triangle>, depth: u32, stone: u32) {
        let index = nodes.len();
        nodes.push(node(0, 0, 0, 0.0));
        if depth == 0 {
            let first = triangles.len() as u32;
            triangles.extend(plane(-2.0, 2.0, 2.0, stone, [0.5; 2], false));
            nodes[index].first = first;
            nodes[index].count = 2;
        } else {
            deep(nodes, triangles, depth - 1, stone);
            deep(nodes, triangles, 0, stone);
        }
        nodes[index].escape = nodes.len() as u32;
    }
    let mut overflow = Scene::default();
    deep(&mut overflow.nodes, &mut overflow.triangles, 18, stone);
    let hits = run(&f, &overflow, 0.0, true);
    assert!(
        hits[0][3] > 0.0 && hits[1][3] > 0.0,
        "fixture must exercise overflow fallback: {hits:?}"
    );
    assert_eq!(hits[0][0], 2.0);
    assert_eq!(hits[0][1], 1.0); // x=-1,y=0 lies in the quad's second triangle.
}
