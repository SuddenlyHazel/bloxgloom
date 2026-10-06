//! Real occlusion, not initial voxel-light state, distinguishes open/closed rooms.
use super::*;
use crate::world::ChunkKey;
fn room(material: u32, open: bool) -> Vec<Triangle> {
    let mut triangles = Vec::new();
    let mut append = |corners| triangles.extend(quad(corners, material, [0.5; 2], false));
    append([[2., 2., 2.], [14., 2., 2.], [14., 2., 14.], [2., 2., 14.]]);
    append([[2., 2., 2.], [2., 14., 2.], [14., 14., 2.], [14., 2., 2.]]);
    append([
        [2., 2., 14.],
        [14., 2., 14.],
        [14., 14., 14.],
        [2., 14., 14.],
    ]);
    append([[2., 2., 2.], [2., 2., 14.], [2., 14., 14.], [2., 14., 2.]]);
    append([
        [14., 2., 2.],
        [14., 14., 2.],
        [14., 14., 14.],
        [14., 2., 14.],
    ]);
    if open {
        for (x0, x1, z0, z1) in [
            (2., 6., 2., 14.),
            (10., 14., 2., 14.),
            (6., 10., 2., 6.),
            (6., 10., 10., 14.),
        ] {
            append([[x0, 14., z0], [x1, 14., z0], [x1, 14., z1], [x0, 14., z1]]);
        }
    } else {
        append([
            [2., 14., 2.],
            [14., 14., 2.],
            [14., 14., 14.],
            [2., 14., 14.],
        ]);
    }
    // Every face has zero voxel sky, in both rooms.
    assert!(triangles.iter().all(|t| t.b[3] == 0.0));
    triangles
}
#[test]
fn gpu_loaded_aperture_lights_zero_sky_receivers_but_closed_and_unknown_stay_dark() {
    let catalog = Catalog::builtins();
    let material = catalog
        .textures()
        .iter()
        .position(|t| t.key.ends_with(":stone"))
        .unwrap() as u32;
    let f = Fixture::new(&catalog);
    let keys: Vec<_> = (0..8).map(|y| ChunkKey { x: 0, y, z: 0 }).collect();
    for mode in [12, 13] {
        let closed = f.run_loaded(room(material, false), mode, 0.0, 12, 1, keys.clone())[0];
        let open = f.run_loaded(room(material, true), mode, 0.0, 12, 1, keys.clone())[0];
        assert_eq!(&closed[..3], &[0.0; 3], "sealed room {mode}: {closed:?}");
        assert!(
            open[..3].iter().all(|v| *v > 0.05),
            "actual aperture must see exterior {mode}: {open:?}"
        );
        let mut hole = keys.clone();
        hole.retain(|k| k.y != 4);
        let unknown = f.run_loaded(room(material, true), mode, 0.0, 12, 1, hole)[0];
        assert_eq!(
            &unknown[..3],
            &[0.0; 3],
            "unloaded roof cell must not certify escape: {unknown:?}"
        );
        let no_coverage = f.run_loaded(room(material, true), mode, 0.0, 12, 1, vec![])[0];
        assert_eq!(
            &no_coverage[..3],
            &[0.0; 3],
            "unknown/disabled coverage retains fallback"
        );
    }
    // The slanted aperture path enters a second X column before reaching sky.
    // Bounds alone would falsely accept the missing column; 3D coverage must not.
    let diagonal_unknown = f.run_loaded(room(material, true), 12, 0.0, 12, 2, keys.clone());
    assert!(diagonal_unknown[0][0] > 0.05);
    assert_eq!(&diagonal_unknown[1][..3], &[0.0; 3]);
    let mut adjacent = keys.clone();
    adjacent.extend((0..8).map(|y| ChunkKey { x: 1, y, z: 0 }));
    let diagonal_known = f.run_loaded(room(material, true), 12, 0.0, 12, 2, adjacent);
    assert!(diagonal_known[1][..3].iter().all(|v| *v > 0.05));
    // Force the first *actual integrator* medium event above the certified
    // boundary. Source solar is zero, isolating escaped sky after HG scatter.
    let witnessed = f.run_loaded(room(material, true), 16, 0.0, 12, 64, keys.clone());
    let energy = witnessed.iter().map(|p| p[0] + p[1] + p[2]).sum::<f32>() / 64.0;
    assert!(
        energy > 0.08,
        "certified exterior sky was discarded after scattering: {energy}"
    );
    let mut unknown = keys.clone();
    unknown.retain(|k| k.y != 4);
    let unwitnessed = f.run_loaded(room(material, true), 16, 0.0, 12, 16, unknown);
    assert!(
        unwitnessed.iter().all(|p| p[..3] == [0.0; 3]),
        "a missing coverage cell must not manufacture a witness"
    );
    let sealed = f.run_loaded(room(material, false), 16, 0.0, 12, 16, keys.clone());
    assert!(
        sealed.iter().all(|p| p[..3] == [0.0; 3]),
        "closed room cannot gain a false medium sky witness"
    );
    let certificates = f.run_loaded(room(material, true), 14, 0.0, 12, 4, keys.clone());
    assert_eq!(
        certificates.iter().map(|p| p[0]).collect::<Vec<_>>(),
        [1.0, 0.0, 0.0, 0.0],
        "only a fully covered upward exit is certified"
    );
}
