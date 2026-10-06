//! Compare independent scalar/vector pipelines over identical page bytes.
use super::*;
#[path = "vector/harness.rs"]
mod harness;

const DECODE: &str = r#"
@compute @workgroup_size(1) fn decode_records(){
    var output=0u;
    for(var page=0u;page<4u;page++) {
        for(var i=0u;i<ray_lod_word(page,0u);i++) {
            let node=ray_lod_node(page,i);
            results[output]=vec4u(bitcast<vec4u>(vec4f(node.low,0.0)).xyz,node.first);output++;
            results[output]=vec4u(bitcast<vec4u>(vec4f(node.high,0.0)).xyz,node.count);output++;
            results[output]=vec4u(node.escape,0u,0u,0u);output++;
        }
        for(var i=0u;i<ray_lod_word(page,1u);i++) {
            let triangle=ray_lod_triangle(((page+1u)<<28u)|i);
            results[output]=bitcast<vec4u>(triangle.a);output++;
            results[output]=bitcast<vec4u>(triangle.b);output++;
            results[output]=bitcast<vec4u>(triangle.c);output++;
            results[output]=bitcast<vec4u>(triangle.uv_ab);output++;
            results[output]=vec4u(bitcast<vec2u>(triangle.uv_c),triangle.surface_color,triangle.surface_flags);output++;
            results[output]=bitcast<vec4u>(triangle.normal);output++;
        }
    }
}
@compute @workgroup_size(1) fn debug_middle_roots(){
    let origin=vec3f(3.0,0.0,0.0);let direction=vec3f(1.0,0.0,0.0);
    let inverse=vec3f(1.0,10000000.0,10000000.0);
    let empty=RayHit(1000.0,0xffffffffu,vec2f(0.0),vec3f(0.0));
    for(var page=0u;page<4u;page++) {
        let node=ray_lod_node(page,0u);
        let near=ray_box_near(origin,inverse,node,1000.0);
        let hit=ray_lod_cast_page(origin,direction,inverse,1000.0,empty,page);
        results[page]=vec4u(bitcast<u32>(near),bitcast<u32>(hit.distance),hit.triangle,u32(ray_box(origin,inverse,node,1000.0)));
        results[4u+page]=vec4u(bitcast<vec4u>(vec4f(node.low,0.0)).xyz,node.first);
        results[8u+page]=vec4u(bitcast<vec4u>(vec4f(node.high,0.0)).xyz,node.count);
        results[12u+page]=vec4u(node.escape,ray_lod_word(page,0u),0u,0u);
    }
    let ordered=ray_lod_cast(origin,direction,1000.0,empty);
    let fixed=full_cast(origin,direction,1000.0,empty);
    results[16]=vec4u(bitcast<u32>(ordered.distance),ordered.triangle,bitcast<u32>(fixed.distance),fixed.triangle);
}
"#;

fn controls(ordered: bool) -> [String; 2] {
    let scalar = format!("{}\n{DECODE}", source()).replace(
        "const RAY_LOD_ROOT_ORDER:bool=true;",
        if ordered {
            "const RAY_LOD_ROOT_ORDER:bool=true;"
        } else {
            "const RAY_LOD_ROOT_ORDER:bool=false;"
        },
    );
    // Some matrix cases contain an explicitly empty page2. No logical node
    // exists there; robust backend OOB clamping is not a record contract.
    assert_eq!(scalar.matches("let root=ray_lod_node(2u,0u);").count(), 1);
    let scalar=scalar.replace("let root=ray_lod_node(2u,0u);",
        "var root=RayNode(vec3f(0.0),0u,vec3f(0.0),0u,0u,0u,0u,0u);if ray_lod_word(2u,0u)>0u {root=ray_lod_node(2u,0u);}");
    let vector = super::super::vector::source_for(&scalar, true);
    if ordered {
        assert_eq!(
            vector.as_bytes(),
            scalar.as_bytes(),
            "combined request must explicitly retain scalar decoder"
        );
    } else {
        for page in 0..4 {
            assert_eq!(
                vector
                    .matches(&format!("ray_lod_page_{page}:array<vec4u>"))
                    .count(),
                1,
                "native fixed-order vector page{page} decoder not active"
            );
        }
    }
    [scalar, vector]
}

fn check_records(scenes: &[Scene]) {
    let mut expected = vec![];
    for scene in scenes {
        for node in &scene.nodes {
            expected.extend(node.min.map(f32::to_bits));
            expected.push(node.first);
            expected.extend(node.max.map(f32::to_bits));
            expected.push(node.count);
            expected.extend([node.escape, 0, 0, 0]);
        }
        for triangle in &scene.triangles {
            expected.extend_from_slice(bytemuck::cast_slice(std::slice::from_ref(triangle)));
        }
    }
    for source in controls(false) {
        let words = harness::run(&source, scenes, &[], "decode_records");
        assert_eq!(
            &words[..expected.len()],
            expected,
            "decoded GPU records differ from CPU bytes"
        );
    }
}

#[test]
fn gpu_vector_lod_record_decode_preserves_unsigned_metadata_and_all_pages() {
    let mut scenes = scenes();
    for (page, scene) in scenes.iter_mut().enumerate() {
        for (index, triangle) in scene.triangles.iter_mut().enumerate() {
            triangle.surface_color =
                [u32::MAX, 0x7fc00001, 0xff008080, 0x80000000][(page + index) % 4];
        }
        for node in &mut scene.nodes {
            node.padding = [u32::MAX, 0x7fc00001, 0x80000000];
        }
        let words = pages::packed_words(scene);
        assert_eq!(words.len() % 4, 0);
        assert_eq!(words[2] % 4, 0);
        assert_eq!(words[3] % 4, 0);
    }
    check_records(&scenes);
    check_records(&[]);
}

#[test]
fn gpu_vector_lod_hits_alpha_fine_authority_ties_and_distant_origins_match_scalar() {
    let mut variants = vec![scenes(), vec![]];
    for shift in [-65536.0, 65536.0] {
        let mut translated = scenes();
        for scene in &mut translated {
            for triangle in &mut scene.triangles {
                for position in [&mut triangle.a, &mut triangle.b, &mut triangle.c] {
                    position[0] += shift;
                    position[1] -= 128.0;
                    position[2] += 32768.0;
                }
            }
            for node in &mut scene.nodes {
                for position in [&mut node.min, &mut node.max] {
                    position[0] += shift;
                    position[1] -= 128.0;
                    position[2] += 32768.0;
                }
            }
        }
        variants.push(translated);
    }
    let page = || {
        Scene::build([Arc::new(Chunk {
            triangles: vec![plane(2.0, 0)],
            ..Default::default()
        })])
    };
    let mut tied = vec![page(), Scene::build([]), Scene::build([]), page()];
    tied[3].nodes[0].min[0] = 1.0;
    variants.push(tied);
    for ordered in [false, true] {
        println!(
            "vector root matrix: rootorder{ordered}, decoder={}",
            if ordered {
                "explicit scalar fallback (native combined mode unsupported)"
            } else {
                "native vector versus scalar"
            }
        );
        let controls = controls(ordered);
        for (case, scenes) in variants.iter().enumerate() {
            for masked in [false, true] {
                let coverage = if masked {
                    vec![0u32, 0, 0, 113, 1, 1, 1, 1, 1, 0, 0, 0]
                } else {
                    vec![]
                };
                let scalar = harness::run(&controls[0], scenes, &coverage, "check");
                let vector = harness::run(&controls[1], scenes, &coverage, "check");
                if case == 0 && !masked {
                    let root_scalar = harness::run(&controls[0], scenes, &[], "debug_middle_roots");
                    let root_vector = harness::run(&controls[1], scenes, &[], "debug_middle_roots");
                    println!(
                        "rootorder{ordered} middle scalar roots (near,hitdist,id,box +bounds/count/escape): {:?}",
                        &root_scalar[..68]
                    );
                    println!(
                        "rootorder{ordered} middle vector roots (near,hitdist,id,box +bounds/count/escape): {:?}",
                        &root_vector[..68]
                    );
                }
                for (word, (scalar, vector)) in scalar.iter().zip(&vector).enumerate() {
                    assert_eq!(
                        scalar, vector,
                        "vector hit/result bits changed rootorder{ordered} case{case} masked{masked} word{word}"
                    );
                }
                assert_eq!(scalar[12], 0, "shared geometric contract differs");
                if case == 0 && !masked {
                    assert!(
                        scalar[14] > 0 && scalar[15] > 0,
                        "alpha coverage not exercised"
                    );
                }
                if case == 0 && masked {
                    assert_eq!(scalar[1], u32::MAX);
                    assert_eq!(scalar[7], 0);
                }
                if case == 4 && !masked {
                    assert_eq!(scalar[1], 0x10000000);
                    assert_eq!(scalar[11], 7);
                }
            }
        }
    }
}
