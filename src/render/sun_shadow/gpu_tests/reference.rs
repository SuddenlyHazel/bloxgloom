//! Actual source-warped terrain casters/receivers, without global environment mutation.
use super::*;
#[test]
fn gpu_reference_shadow_casts_opaque_holes_and_preserves_cave_emission() {
    let mut scene = Fixture::new();
    scene.shadows.projection.settings = super::super::reference::configure(
        scene.shadows.projection.settings,
        true,
        scene.device.limits().max_texture_dimension_2d,
    );
    // Recreate the actual2048px resource, then explicitly enable the focused
    // mode. The production configured constructor does the same at startup.
    scene.shadows.view = super::super::depth(&scene.device, 2048);
    scene.shadows.camera_group = super::super::group(
        &scene.device,
        &scene.camera,
        &scene.shadows.uniform,
        &scene.shadows.view,
        &super::super::sampler(&scene.device),
    );
    let time = crate::daylight::INITIAL_MS;
    let opaque = Fixture::quad(&scene.device, 0.6, 2.0, 0.0, 2.0, [0.25; 2], 1.0, 0.0);
    let clear = scene.render(None, time, SunShadowQuality::Low);
    let shadow = scene.render(Some((&opaque, false)), time, SunShadowQuality::Low);
    let dark = clear
        .chunks_exact(4)
        .zip(shadow.chunks_exact(4))
        .filter(|(a, b)| a[0].saturating_sub(b[0]) > 4)
        .count();
    assert!(
        dark > 100,
        "source-warped opaque geometry must cast actual shadows: {dark}"
    );
    assert_eq!(
        shadow,
        scene.render(Some((&opaque, false)), time, SunShadowQuality::Low),
        "source ring taps must be deterministic"
    );
    let pixels = crate::render::material::material_tiles_for(crate::content::catalog());
    let size = crate::render::material::TEXTURE_SIZE as usize;
    let leaves = &pixels[12 * size * size * 4..13 * size * size * 4];
    for transparent in [true, false] {
        let texel = leaves
            .chunks_exact(4)
            .position(|p| if transparent { p[3] == 0 } else { p[3] == 255 })
            .unwrap();
        let uv = [
            ((texel % size) as f32 + 0.5) / size as f32,
            ((texel / size) as f32 + 0.5) / size as f32,
        ];
        let card = Fixture::quad(&scene.device, 0.6, 2.0, 0.0, 12.0, uv, 1.0, 0.0);
        let cutout = scene.render(Some((&card, true)), time, SunShadowQuality::Low);
        if transparent {
            assert_eq!(
                clear, cutout,
                "alpha hole must not produce an opaque leaf sheet"
            );
        } else {
            assert_eq!(
                cutout,
                scene.render(Some((&card, false)), time, SunShadowQuality::Low),
                "accepted leaf texels are opaque source casters"
            );
        }
    }
    for (sky, glow) in [(0.0, 0.0), (0.0, 1.0)] {
        scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, 2.0, [0.25; 2], sky, glow);
        assert_eq!(
            scene.render(None, time, SunShadowQuality::Low),
            scene.render(Some((&opaque, false)), time, SunShadowQuality::Low),
            "sealed-cave/torch radiance cannot be attenuated by reference shadow"
        );
    }
}
