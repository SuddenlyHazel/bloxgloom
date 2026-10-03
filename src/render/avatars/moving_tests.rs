use super::{
    tests::{HEIGHT, WIDTH, render_avatars},
    *,
};
use bloxgloom_host_api::{RegistrationError, gameplay::EntityState, motion};

struct State;
impl EntityState for State {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn public(&self, _: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(vec![])
    }
}

pub(super) fn catalog(half: [f32; 3]) -> crate::content::Catalog {
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_moving(motion::MovingEntity {
            key: "demo:projectile".into(),
            schema_version: 1,
            schema_fingerprint: 1,
            max_state_bytes: 1,
            max_public_bytes: 1,
            body: motion::Body {
                half_extents: [half.into_iter().fold(0.0, f32::max); 3],
                collisions: motion::CollisionMask {
                    terrain: true,
                    players: false,
                    creatures: false,
                },
                response: motion::Response::Stop,
                restitution: 0.0,
                gravity_scale: 1.0,
                max_speed: 16.0,
                max_acceleration: 16.0,
            },
            lifetime_ticks: 100,
            interval: 1,
            source_exclusion_ticks: 0,
            handles_impact: false,
            handles_expiry: false,
            model: vec![bloxgloom_host_api::entity::Cuboid {
                min: half.map(|value| -value),
                max: half,
                color: [1.0, 0.2, 0.1],
                motion: bloxgloom_host_api::entity::PartMotion::LeftFoot,
            }],
            state: std::sync::Arc::new(State),
        })
        .unwrap();
    catalog
}

#[test]
fn gpu_rigid_moving_model_rotates_in_three_dimensions_without_creature_deformation() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let catalog = catalog([0.5, 0.1, 0.1]);
    let id = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    let mut avatar = VisualAvatar {
        motion: Some(MovingVisual {
            tick: 1,
            revision: 1,
            orientation: glam::Quat::IDENTITY.to_array(),
            velocity: [1.0, 0.0, 0.0],
            stopped: false,
        }),
        animation: Default::default(),
        model: AvatarModel::Moving(id),
        pose: [0.0; 4],
        model_pose: None,
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
        airborne: true,
        id: 1,
        position: Vec3::new(0.0, 0.8, 0.0),
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    };
    let horizontal = render_avatars(&device, &queue, &catalog, &[avatar], None);
    avatar.pose = [2.0, 3.0, 4.0, 0.5];
    let unchanged = render_avatars(&device, &queue, &catalog, &[avatar], None);
    assert_eq!(
        horizontal, unchanged,
        "creature pose must not deform rigid moving parts"
    );
    avatar.motion.as_mut().unwrap().orientation =
        glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    let vertical = render_avatars(&device, &queue, &catalog, &[avatar], None);
    let bounds = |pixels: &[u8]| {
        let mut min = [WIDTH, HEIGHT];
        let mut max = [0, 0];
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel[0] > 20 {
                let p = [(i as u32) % WIDTH, (i as u32) / WIDTH];
                for axis in 0..2 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
            }
        }
        [max[0] - min[0], max[1] - min[1]]
    };
    let h = bounds(&horizontal);
    let v = bounds(&vertical);
    assert!(
        h[0] > h[1] * 3 && v[1] > v[0] * 3,
        "quaternion rotation must rotate silhouette: {h:?} -> {v:?}"
    );
    if let Some(path) = std::env::var_os("BLOXGLOOM_MOTION_PREVIEW") {
        let mut pixels = Vec::with_capacity((WIDTH * 2 * HEIGHT * 4) as usize);
        for row in 0..HEIGHT as usize {
            let range = row * WIDTH as usize * 4..(row + 1) * WIDTH as usize * 4;
            pixels.extend(&horizontal[range.clone()]);
            pixels.extend(&vertical[range]);
        }
        let mut encoder =
            png::Encoder::new(std::fs::File::create(path).unwrap(), WIDTH * 2, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
}
