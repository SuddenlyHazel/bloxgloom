//! Raw split bookkeeping versus the unchanged production sum on actual pools.
use super::*;

const PROBE: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let index=u32(pixel.x)/5u;let variant=u32(pixel.x)%5u;
 let direction=normalize(vec3f(select(0.0,0.65,(index&1u)==1u),-1.0,0.0));
 let origin=vec3f(8.0,6.5,8.0);
 let hit=ray_cast_primary(origin,direction,512.0);
 if !ray_water_is(hit) {return vec4f(-1.0);}
 let position=origin+direction*hit.distance;
 let seed=index*1973u+991u;
 ray_rng=seed;ray_dynamic_touched=false;
 if variant==0u {let result=ray_primary_water_radiance(hit,position,direction,vec4f(0.0));return vec4f(result,select(0.0,1.0,ray_dynamic_touched));}
 if variant==4u {
  let old=ray_primary_water_radiance(hit,position,direction,vec4f(0.0));let old_rng=ray_rng;let old_touch=ray_dynamic_touched;
  ray_rng=seed;ray_dynamic_touched=false;
  let split=ray_primary_water_lobes(hit,position,direction,vec4f(0.0));
  return vec4f(f32(old_rng&65535u),f32(ray_rng&65535u),f32(old_rng>>16u),f32(ray_rng>>16u));
 }
 let split=ray_primary_water_lobes(hit,position,direction,vec4f(0.0));
 if variant==1u {return vec4f(split.total,select(0.0,1.0,ray_dynamic_touched));}
 if variant==2u {return vec4f(split.reflection,1.0);}
 return vec4f(split.total-split.reflection,1.0);
}
"#;

#[test]
fn gpu_actual_first_water_lobes_preserve_raw_rgb_rng_and_unknown_blockers() {
    let (catalog, emitter) = catalog();
    let f = Fixture::new(&catalog);
    let mut lit_reflections = 0;
    let mut lit_transmissions = 0;
    for (name, scene, sun) in [
        (
            "open emission",
            pool(&catalog, emitter, true, false, false),
            false,
        ),
        ("solar", pool(&catalog, emitter, true, false, false), true),
        (
            "opaque bottom blocker",
            pool(&catalog, emitter, true, true, false),
            true,
        ),
        ("enclosed", pool(&catalog, emitter, true, false, true), true),
    ] {
        let rows = run_shader(&f, &scene, 64, sun, 64 * 5, PROBE);
        for (seed, group) in rows.chunks_exact(5).enumerate() {
            assert_ne!(
                group[0], [-1.0; 4],
                "{name} seed{seed} must hit a real loaded water interface"
            );
            assert_eq!(group[4][0], group[4][1], "{name} seed{seed} RNG low");
            assert_eq!(group[4][2], group[4][3], "{name} seed{seed} RNG high");
            assert_eq!(
                group[0][3], group[1][3],
                "{name} seed{seed} dynamic queries"
            );
            for channel in 0..3 {
                let original = group[0][channel];
                let sum = group[1][channel];
                let tolerance = 2e-5 * original.abs().max(1.0);
                assert!(
                    (original - sum).abs() <= tolerance,
                    "{name} seed{seed}: old{:?} split{:?}",
                    group[0],
                    group[1]
                );
                assert!(
                    (original - (group[2][channel] + group[3][channel])).abs() <= tolerance,
                    "{name} seed{seed}: lobes{:?}",
                    group
                );
                assert!(
                    group[2][channel] >= -tolerance && group[3][channel] >= -tolerance,
                    "positive physical lobes, no clipping: {group:?}"
                );
            }
            lit_reflections += usize::from(group[2][..3].iter().any(|v| *v > 0.0001));
            lit_transmissions += usize::from(group[3][..3].iter().any(|v| *v > 0.0001));
        }
    }
    assert!(
        lit_reflections > 0 && lit_transmissions > 0,
        "both real path families must carry energy"
    );
}
