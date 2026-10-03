//! Distance-dependent penumbrae through the actual terrain caster/receiver path.
use super::*;

fn aligned_caster(device: &wgpu::Device, height: f32, sun: Vec3) -> wgpu::Buffer {
    // All heights cast the same centered footprint. Only separation changes.
    let center = sun * (height / sun.y);
    let mut vertices = Vec::new();
    for (x, z) in [(-0.8, -0.8), (-0.8, 0.8), (0.8, 0.8), (0.8, -0.8)] {
        let position = center + Vec3::new(x, 0.0, z);
        vertices.extend_from_slice(&[
            position.x, position.y, position.z, 0.0, 1.0, 0.0, 0.25, 0.25, 2.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0,
        ]);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fixed-footprint variable-distance shadow caster"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn partial_pixels(clear: &[u8], shadow: &[u8]) -> usize {
    let drops: Vec<_> = clear
        .chunks_exact(4)
        .zip(shadow.chunks_exact(4))
        .map(|(clear, shadow)| u16::from(clear[0].saturating_sub(shadow[0])))
        .collect();
    let full = *drops.iter().max().unwrap();
    assert!(
        full > 8,
        "fixture must contain a solid shadow interior, got {full}"
    );
    drops
        .iter()
        .filter(|&&drop| drop * 10 > full && drop * 10 < full * 9)
        .count()
}

fn capture(name: &str, pixels: &[u8]) {
    let Ok(directory) = std::env::var("BLOXGLOOM_SHADOW_CAPTURE_DIRECTORY") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    let file = std::fs::File::create(std::path::Path::new(&directory).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, SIZE, SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

#[test]
fn gpu_soft_sun_widens_with_blocker_distance_and_preserves_contact() {
    let mut scene = Fixture::new();
    let layer = crate::render::material::material_layer(crate::world::SAND, 1, 1) as f32;
    scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, layer, [0.25; 2], 1.0, 0.0);
    let time = crate::daylight::INITIAL_MS;
    let sun = Atmosphere::at(time).sun;
    let near = aligned_caster(&scene.device, 0.1, sun);
    let far = aligned_caster(&scene.device, 8.0, sun);
    for quality in [SunShadowQuality::Medium, SunShadowQuality::High] {
        scene
            .shadows
            .configure(&scene.device, &scene.camera, quality);
        scene.shadows.projection.settings.softness = Softness::default();
        let clear = scene.render(None, time, quality);
        let contact = scene.render(Some((&near, false)), time, quality);
        let detached = scene.render(Some((&far, false)), time, quality);
        let near_partial = partial_pixels(&clear, &contact);
        let far_partial = partial_pixels(&clear, &detached);
        assert!(
            far_partial > near_partial * 3 / 2,
            "{quality:?}: distant blocker must have a wider penumbra: {near_partial} vs {far_partial}"
        );
        assert_eq!(
            detached,
            scene.render(Some((&far, false)), time, quality),
            "stationary receivers must not acquire frame-dependent filter noise"
        );
        scene.shadows.projection.settings.softness = Softness::from_values(Some("0"), None);
        let fixed = scene.render(Some((&far, false)), time, quality);
        let fixed_partial = partial_pixels(&clear, &fixed);
        assert!(
            far_partial > fixed_partial * 3 / 2,
            "{quality:?}: PCSS must visibly exceed the fixed filter: {fixed_partial} vs {far_partial}"
        );
        eprintln!(
            "{quality:?} penumbra pixels: contact={near_partial}, fixed={fixed_partial}, detached={far_partial}"
        );
        if quality == SunShadowQuality::High {
            capture("contact.png", &contact);
            capture("before-fixed.png", &fixed);
            capture("after-soft.png", &detached);
        }
    }
}

#[test]
fn gpu_soft_sun_does_not_attenuate_cave_or_emissive_light() {
    let mut scene = Fixture::new();
    let time = crate::daylight::INITIAL_MS;
    let caster = aligned_caster(&scene.device, 8.0, Atmosphere::at(time).sun);
    for (sky, glow) in [(0.0, 0.0), (0.0, 1.0)] {
        scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, 2.0, [0.25; 2], sky, glow);
        for quality in [SunShadowQuality::Medium, SunShadowQuality::High] {
            assert_eq!(
                scene.render(None, time, quality),
                scene.render(Some((&caster, false)), time, quality),
                "{quality:?}: soft sun must leave sealed-cave/torch energy unchanged"
            );
        }
    }
}

#[test]
fn gpu_thin_diagonal_caster_keeps_a_soft_footprint() {
    let mut scene = Fixture::new();
    let layer = crate::render::material::material_layer(crate::world::SAND, 1, 1) as f32;
    scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, layer, [0.25; 2], 1.0, 0.0);
    let quality = SunShadowQuality::High;
    scene
        .shadows
        .configure(&scene.device, &scene.camera, quality);
    let time = crate::daylight::INITIAL_MS;
    let center = Atmosphere::at(time).sun * (8.0 / Atmosphere::at(time).sun.y);
    let along = Vec3::new(1.0, 0.0, 1.0).normalize();
    let across = Vec3::new(-1.0, 0.0, 1.0).normalize();
    let mut vertices = Vec::new();
    // A 0.12-unit-wide diagonal strip, narrower than the default penumbra.
    for (a, b) in [(-1.5, -0.06), (-1.5, 0.06), (1.5, 0.06), (1.5, -0.06)] {
        let p = center + along * a + across * b;
        vertices.extend_from_slice(&[
            p.x, p.y, p.z, 0.0, 1.0, 0.0, 0.25, 0.25, layer, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ]);
    }
    let caster = scene
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("thin diagonal distant shadow caster"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let clear = scene.render(None, time, quality);
    scene.shadows.projection.settings.softness = Softness::from_values(Some("0"), None);
    let fixed = scene.render(Some((&caster, false)), time, quality);
    scene.shadows.projection.settings.softness = Softness::default();
    let soft = scene.render(Some((&caster, false)), time, quality);
    let footprint = |pixels: &[u8]| {
        clear
            .chunks_exact(4)
            .zip(pixels.chunks_exact(4))
            .filter(|(a, b)| a[0].saturating_sub(b[0]) > 1)
            .count()
    };
    assert!(
        footprint(&fixed) > 30,
        "thin fixture must actually cast a shadow"
    );
    assert!(
        footprint(&soft) > footprint(&fixed) * 5 / 4,
        "diagonal caster penumbra disappeared: fixed={}, soft={}",
        footprint(&fixed),
        footprint(&soft)
    );
    capture("thin-before-fixed.png", &fixed);
    capture("thin-after-soft.png", &soft);
}
