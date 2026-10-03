//! Visual acceptance uses the production instanced renderer and server solver.
//! Rows are launch arc, bounce, guided control and stop/impact/removal.
use super::{
    tests::{HEIGHT, WIDTH, render_avatars},
    *,
};
use crate::server::motion_preview_solver as solver;

#[test]
fn gpu_moving_projectile_flight_bounce_guidance_and_impact_filmstrip() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut catalog = super::moving_tests::catalog([0.15, 0.05, 0.05]);
    let id = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    let mut ground = catalog.moving_entity(id).unwrap().as_ref().clone();
    ground.key = "demo:ground".into();
    ground.model[0].min = [-1.3, -0.05, -0.3];
    ground.model[0].max = [1.3, 0.05, 0.3];
    ground.model[0].color = [0.22, 0.32, 0.42];
    catalog.register_moving(ground).unwrap();
    let ground_id = catalog.entity_type_id_by_key("demo:ground").unwrap();
    let mut contact = catalog.moving_entity(id).unwrap().as_ref().clone();
    contact.key = "demo:contact".into();
    contact.model[0].min = [-0.06; 3];
    contact.model[0].max = [0.06; 3];
    contact.model[0].color = [0.2, 1.0, 0.3];
    catalog.register_moving(contact).unwrap();
    let contact_id = catalog.entity_type_id_by_key("demo:contact").unwrap();
    let visual = |entity_id, kind, position: [f64; 3], velocity: [f64; 3], tick| {
        let velocity = Vec3::from_array(velocity.map(|x| x as f32));
        let orientation = if velocity.length_squared() > 0.0001 {
            glam::Quat::from_rotation_arc(Vec3::X, velocity.normalize())
        } else {
            glam::Quat::IDENTITY
        };
        VisualAvatar {
            motion: Some(MovingVisual {
                tick,
                revision: tick,
                orientation: orientation.to_array(),
                velocity: velocity.to_array(),
                stopped: velocity.length_squared() == 0.0,
            }),
            animation: Default::default(),
            model: AvatarModel::Moving(kind),
            pose: [0.0; 4],
            model_pose: None,
            character_pose: [0.0; 4],
            character_look: [0.0; 2],
            character_crouch: 0.0,
            character_tool: None,
            character_recipe: None,
            airborne: true,
            id: entity_id,
            position: Vec3::from_array(position.map(|x| x as f32)),
            cosmetics: [0; 4],
            light_levels: [15, 0, 0, 0],
            bounce: [0; 4],
            glow_color: [0; 3],
            glow_direction: [0; 3],
            glow_bounce: [0; 4],
            tint: [1.0; 3],
        }
    };
    let floor = solver::Collider {
        target: solver::Target::Terrain {
            cell: [0, 0, 0],
            state: 1,
        },
        min: [-2.0, -1.0, -1.0],
        max: [2.0, 0.2, 1.0],
        displacement: [0.0; 3],
    };
    let limits = solver::Limits {
        colliders: 64,
        sweep_cells: 4096,
        contacts: 4,
        world_min: [-1000.0; 3],
        world_max: [1000.0; 3],
    };
    const FRAMES: usize = 10;
    let mut rows = vec![];
    for scenario in 0..4 {
        let mut state = solver::State {
            position: [-0.9, 1.4, 0.0],
            velocity: [1.2, 0.0, 0.0],
            acceleration: [0.0, if scenario == 0 { -1.2 } else { -4.0 }, 0.0],
        };
        let body = solver::Body {
            half_extents: catalog
                .moving_entity(id)
                .unwrap()
                .body
                .half_extents
                .map(f64::from),
            response: if scenario == 1 {
                solver::Response::Bounce
            } else {
                solver::Response::Stop
            },
            restitution: 0.8,
        };
        let mut impact = None;
        let mut removed = false;
        let mut row = vec![];
        for frame in 0..FRAMES {
            if scenario == 2 && frame == 4 {
                state.velocity = [0.4, 1.0, 0.0];
                state.acceleration = [1.0, -1.0, 0.0];
            }
            for _ in 0..3 {
                if !removed {
                    let step = solver::integrate(
                        state,
                        body,
                        0.04,
                        if scenario == 0 || scenario == 2 {
                            &[]
                        } else {
                            std::slice::from_ref(&floor)
                        },
                        limits,
                    )
                    .unwrap();
                    if let Some(contact) = step.contacts.first() {
                        impact = Some([contact.position[0], 0.27, 0.0]);
                    }
                    state.position = step.position;
                    state.velocity = step.velocity;
                }
            }
            let mut avatars = vec![visual(
                2,
                ground_id,
                [0.0, 0.15, 0.0],
                [0.0; 3],
                frame as u64,
            )];
            if !removed {
                avatars.push(visual(1, id, state.position, state.velocity, frame as u64));
            }
            if let Some(position) = impact {
                avatars.push(visual(3, contact_id, position, [0.0; 3], frame as u64));
            }
            row.push(render_avatars(&device, &queue, &catalog, &avatars, None));
            if scenario == 3 && impact.is_some() {
                removed = true;
            }
        }
        assert_ne!(
            row[0], row[3],
            "flight/control must move production geometry"
        );
        if scenario == 1 {
            assert!(
                impact.is_some() && state.velocity[1] > 0.0,
                "bounce trajectory must leave contact moving upward"
            );
        }
        if scenario == 3 {
            assert!(
                removed && impact.is_some(),
                "impact row ends after authoritative removal"
            );
        }
        rows.push(row);
    }
    if let Some(path) = std::env::var_os("BLOXGLOOM_PROJECTILE_PREVIEW") {
        let width = WIDTH * FRAMES as u32;
        let height = HEIGHT * 4;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in rows {
            for y in 0..HEIGHT as usize {
                for frame in &row {
                    pixels.extend(&frame[y * WIDTH as usize * 4..(y + 1) * WIDTH as usize * 4]);
                }
            }
        }
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
}
