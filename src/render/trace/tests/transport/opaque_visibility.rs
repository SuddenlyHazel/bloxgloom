//! Actual source material alpha and production sunlight; opt-in specialization
//! must agree with the original ordered closest-hit optical path.
use super::*;

#[test]
fn gpu_opaque_any_hit_matches_ordered_sunlight_with_leaf_sheets_and_alpha_holes() {
    let mut catalog = Catalog::builtins();
    let texture = |catalog: &Catalog, stem: &str| {
        catalog
            .textures()
            .iter()
            .position(|t| t.key.ends_with(&format!(":{stem}")))
            .unwrap() as u32
    };
    let stone = texture(&catalog, "stone");
    let cherry = texture(&catalog, "jg_cherry_leaves");
    let [hole, solid] = alpha_samples(&catalog.textures()[cherry as usize].png);
    // Same actual alpha artwork with generic-cutout metadata: tests the
    // specialization's own LOD0 rejection, rather than its botanical skip.
    let mut generic = catalog.textures()[cherry as usize].clone();
    generic.key = "test:opaque-alpha-cutout".into();
    generic.foliage = Default::default();
    generic.emission_strength = 0.0;
    let generic = catalog.register_texture(generic).unwrap().get();
    let fixture = Fixture::new(&catalog);
    let leaf = |z| plane(-2.0, 2.0, z, cherry, solid, true);
    let block = |z| plane(-2.0, 2.0, z, stone, [0.5; 2], false);
    let opaque_alpha = |uv| plane(-2.0, 2.0, 1.0, generic, uv, true);
    let mut two = leaf(1.0);
    two.extend(leaf(2.0));
    let mut before = block(0.5);
    before.extend(two.clone());
    let mut after = two.clone();
    after.extend(block(3.0));
    let mut hole_then_leaf = opaque_alpha(hole);
    hole_then_leaf.extend(leaf(2.0));
    let mut beyond_first_limit = leaf(1.0);
    beyond_first_limit.extend(block(512.5));
    let mut behind = block(-1.0);
    behind.extend(leaf(1.0));
    let cases = [
        vec![],
        leaf(1.0),
        two,
        before,
        after,
        opaque_alpha(hole),
        opaque_alpha(solid),
        hole_then_leaf,
        beyond_first_limit,
        behind,
    ];
    for (case, triangles) in cases.into_iter().enumerate() {
        let mut both = triangles.clone();
        both.extend(triangles.into_iter().map(|mut t| {
            for p in [&mut t.a, &mut t.b, &mut t.c] {
                p[0] -= 5.0;
                p[1] -= 3.0;
                p[2] -= 4.0;
            }
            t
        }));
        let reference = fixture.run(both.clone(), 17, 0.0, 12, 2);
        let optimized = fixture.run(both, 18, 0.0, 12, 2);
        assert_eq!(
            optimized, reference,
            "case {case}: positive/negative-coordinate visibility changed"
        );
        if matches!(case, 3 | 4 | 6 | 8) {
            for pixel in optimized {
                assert_eq!(&pixel[..3], &[0.0; 3], "case {case}: opaque blocker leaked");
            }
        } else {
            assert!(
                optimized.iter().all(|pixel| pixel[0] > 0.0),
                "case {case}: clear/thin path blacked out"
            );
        }
    }
    // Retain current-vertex direct/emissive energy before an exact zero
    // scatter, and skip every later path vertex. Both use real material maps.
    let glow = texture(&catalog, "glowstone");
    for first in [stone, glow] {
        let mut geometry = plane(-2.0, 2.0, 1.0, first, [0.5; 2], false);
        geometry.extend(plane(-2.0, 2.0, 2.0, glow, [0.5; 2], false));
        let one = fixture.run(geometry.clone(), 19, 0.0, 1, 1)[0];
        let twelve = fixture.run(geometry, 19, 0.0, 12, 1)[0];
        assert_eq!(
            one, twelve,
            "zero continuation must retain current direct/emission exactly"
        );
        assert!(twelve[0] > 0.0);
        assert_eq!(twelve[3], 1.0, "zero path traced extra vertices");
    }
    let mut geometry = plane(-2.0, 2.0, 1.0, stone, [0.5; 2], false);
    geometry.extend(plane(-2.0, 2.0, 2.0, glow, [0.5; 2], false));
    let zero = fixture.run(geometry.clone(), 20, 0.0, 12, 1)[0];
    assert_eq!(
        zero,
        [0.0, 0.0, 0.0, 1.0],
        "zero throughput cannot see later emission"
    );
    let tiny = fixture.run(geometry, 21, 0.0, 12, 1)[0];
    assert!(
        tiny[..3].iter().all(|v| *v > 0.0 && *v < 1e-18),
        "tiny positive energy was cut off: {tiny:?}"
    );
    assert_eq!(
        tiny[3], 2.0,
        "positive throughput must continue until it becomes exactly zero"
    );
    let mut geometry = plane(-2.0, 2.0, 1.0, stone, [0.5; 2], false);
    geometry.extend(plane(-2.0, 2.0, 2.0, glow, [0.5; 2], false));
    let zero_primary = super::primary::run_mode(&fixture, geometry.clone(), 1.0, 0.0, false, 22);
    for row in zero_primary {
        assert_eq!(
            row[0], 0.0,
            "exact-zero primary sample still invoked transport"
        );
        assert_eq!(row[1], 1.0);
        assert_eq!(row[3], 1.0);
        assert!(row[2].is_finite(), "ambient replacement remains finite");
    }
    let tiny_primary = super::primary::run_mode(&fixture, geometry, 1.0, 0.0, false, 23);
    assert!(
        tiny_primary.iter().all(|row| row[0] > 0.0),
        "tiny positive primary weight was skipped"
    );
}
