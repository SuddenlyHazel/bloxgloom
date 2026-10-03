//! Moving-sun self-shadowing and contact coverage through production passes.
use super::*;

fn sloped_floor(device: &wgpu::Device, slope: [f32; 2]) -> wgpu::Buffer {
    let normal = Vec3::new(-slope[0], 1.0, -slope[1]).normalize();
    let mut vertices = Vec::new();
    for (x, z) in [(-4.0, -4.0), (-4.0, 4.0), (4.0, 4.0), (4.0, -4.0)] {
        vertices.extend_from_slice(&[
            x,
            x * slope[0] + z * slope[1],
            z,
            normal.x,
            normal.y,
            normal.z,
            0.25,
            0.25,
            2.0,
            1.0,
            0.0,
            0.0,
            0.0,
        ]);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("coplanar tilted receiver and caster"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

#[test]
fn gpu_coplanar_receivers_stay_lit_as_the_sun_moves_at_every_quality() {
    let mut scene = Fixture::new();
    // The receiver itself must be in the depth map, as it is in live terrain.
    // Previous tests only submitted a separate floating occluder.
    for slope in [[0.0, 0.0], [0.35, 0.2], [-0.25, 0.45]] {
        scene.floor = sloped_floor(&scene.device, slope);
        let floor = sloped_floor(&scene.device, slope);
        for quality in [
            SunShadowQuality::Low,
            SunShadowQuality::Medium,
            SunShadowQuality::High,
        ] {
            for phase in [3, 7, 13, 19, 25, 31, 37, 43, 47] {
                let time = crate::daylight::CYCLE_MS * phase / 100;
                let clear = scene.render(None, time, quality);
                let self_cast = scene.render(Some((&floor, false)), time, quality);
                let changed = clear
                    .chunks_exact(4)
                    .zip(self_cast.chunks_exact(4))
                    .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(&x, &y)| x.abs_diff(y) > 1))
                    .count();
                assert_eq!(
                    changed, 0,
                    "coplanar self-shadow bands at phase {phase}, {quality:?}, slope {slope:?}"
                );
            }
        }
    }
}

#[test]
fn gpu_wall_shadows_keep_ground_contact_with_receiver_plane_correction() {
    let mut scene = Fixture::new();
    scene.cast_floor = true;
    for quality in [
        SunShadowQuality::Low,
        SunShadowQuality::Medium,
        SunShadowQuality::High,
    ] {
        for phase in [13, 25, 37] {
            let time = crate::daylight::CYCLE_MS * phase / 100;
            let sun = Atmosphere::at(time).sun;
            let horizontal = Vec3::new(sun.x, 0.0, sun.z).normalize();
            let tangent = Vec3::new(-horizontal.z, 0.0, horizontal.x);
            let mut vertices = Vec::new();
            for (side, height) in [(-1.0, 0.0), (-1.0, 2.0), (1.0, 2.0), (1.0, 0.0)] {
                let position = tangent * side + Vec3::Y * height;
                vertices.extend_from_slice(&[
                    position.x,
                    position.y,
                    position.z,
                    horizontal.x,
                    horizontal.y,
                    horizontal.z,
                    0.25,
                    0.25,
                    2.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ]);
            }
            let wall = scene
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("wall touching the receiver"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let clear = scene.render(None, time, quality);
            let shaded = scene.render(Some((&wall, false)), time, quality);
            let view = rh::view::look_at_mat4(Vec3::new(0.0, 5.0, 0.0), Vec3::ZERO, Vec3::Z);
            let matrix = rh::proj::directx::orthographic(-4.0, 4.0, -4.0, 4.0, 0.1, 20.0) * view;
            let mut drops = Vec::new();
            // A strip within 1/8–5/16 of a block from the wall's ground edge.
            // Excess bias would detach the shadow and leave this strip lit.
            for depth in [0.125, 0.1875, 0.25, 0.3125] {
                for side in [
                    -0.4375, -0.3125, -0.1875, -0.0625, 0.0625, 0.1875, 0.3125, 0.4375,
                ] {
                    let world = tangent * side - horizontal * depth;
                    let p = matrix.project_point3(world);
                    let x = ((p.x * 0.5 + 0.5) * SIZE as f32) as usize;
                    let y = ((0.5 - p.y * 0.5) * SIZE as f32) as usize;
                    let index = (y * SIZE as usize + x) * 4;
                    drops.push(clear[index].saturating_sub(shaded[index]));
                }
            }
            let full = *drops[24..].iter().max().unwrap();
            assert!(
                full > 4,
                "missing wall shadow at phase {phase}, {quality:?}"
            );
            // A filtered contact edge is partially shadowed. Compare to the
            // same frame's fully shadowed interior, rather than assuming an
            // absolute RGB difference means full coverage at every sun angle.
            assert!(
                drops[..8]
                    .iter()
                    .all(|&drop| u16::from(drop) * 2 >= u16::from(full)),
                "shadow detached at phase {phase}, {quality:?}: {drops:?}"
            );
            assert!(
                drops[16..]
                    .iter()
                    .all(|&drop| u16::from(drop) * 100 >= u16::from(full) * 85),
                "wall shadow weakened at phase {phase}, {quality:?}: {drops:?}"
            );
        }
    }
}
