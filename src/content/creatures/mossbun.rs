use bloxgloom_host_api::entity::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Mossbun {
    pub cycle: u64,
    pub facing: u8,
    pub steps: u8,
    pub goal: Option<[i32; 2]>,
    pub waypoint: Option<[i32; 2]>,
    pub vertical_velocity: f32,
    pub grounded: bool,
    pub think_at: u64,
}
pub(crate) struct Wander;
impl Behavior for Wander {
    fn initial(&self) -> Payload {
        Payload::new(Mossbun::default())
    }
    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error> {
        if bytes.len() != 41 || bytes[14] > 1 {
            return Err(Error::InvalidState);
        }
        let cell = |at: usize| {
            let p = [
                i32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()),
                i32::from_le_bytes(bytes[at + 5..at + 9].try_into().unwrap()),
            ];
            match bytes[at] {
                0 if p == [0; 2] => Ok(None),
                1 => Ok(Some(p)),
                _ => Err(Error::InvalidState),
            }
        };
        let state = Payload::new(Mossbun {
            cycle: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            facing: bytes[8],
            steps: bytes[9],
            vertical_velocity: f32::from_le_bytes(bytes[10..14].try_into().unwrap()),
            grounded: bytes[14] == 1,
            goal: cell(15)?,
            waypoint: cell(24)?,
            think_at: u64::from_le_bytes(bytes[33..].try_into().unwrap()),
        });
        self.encode(&state)?;
        Ok(state)
    }
    fn encode(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        let bun = state.downcast_ref::<Mossbun>().ok_or(Error::InvalidState)?;
        if bun.facing > 3
            || bun.steps > 16
            || !bun.vertical_velocity.is_finite()
            || !(-24.0..=0.0).contains(&bun.vertical_velocity)
            || [bun.goal, bun.waypoint]
                .into_iter()
                .flatten()
                .flatten()
                .any(|v| !(-999_999..999_999).contains(&v))
        {
            return Err(Error::InvalidState);
        }
        let mut out = bun.cycle.to_le_bytes().to_vec();
        out.extend([bun.facing, bun.steps]);
        out.extend(bun.vertical_velocity.to_le_bytes());
        out.push(bun.grounded as u8);
        for p in [bun.goal, bun.waypoint] {
            out.push(p.is_some() as u8);
            for v in p.unwrap_or([0; 2]) {
                out.extend(v.to_le_bytes());
            }
        }
        out.extend(bun.think_at.to_le_bytes());
        Ok(out)
    }
    fn public(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        self.encode(state)?;
        let b = state.downcast_ref::<Mossbun>().unwrap();
        Ok(vec![
            b.facing,
            u8::from(b.waypoint.is_some()) | (u8::from(b.grounded) << 1),
        ])
    }
    fn pose(&self, bytes: &[u8]) -> Result<Pose, Error> {
        match bytes {
            [facing, flags] if *facing < 4 && *flags < 4 => Ok(Pose {
                yaw: f32::from(*facing) * std::f32::consts::FRAC_PI_2,
                grounded: *flags & 2 != 0,
            }),
            _ => Err(Error::InvalidState),
        }
    }
    fn tick(&self, c: &Context<'_>) -> Result<Plan, Error> {
        let mut plan = Plan {
            state: None,
            next_tick: c.next_tick,
            position: None,
            lifecycle: Lifecycle::default(),
        };
        let position = c.position;
        let tick = c.tick;
        let clear = c.world.clear(position)?;
        let grounded = c.world.grounded(position)?;
        let mut bun = *c
            .state
            .downcast_ref::<Mossbun>()
            .ok_or(Error::InvalidState)?;
        if c.next_tick.is_some_and(|due| due > tick)
            && (!clear || grounded || bun.vertical_velocity < 0.0)
        {
            return Ok(plan);
        }
        let mut delay = 1;
        if !clear {
            bun.steps = 0;
            bun.goal = None;
            bun.waypoint = None;
            delay = 10;
        } else if !grounded {
            bun.goal = None;
            bun.waypoint = None;
            bun.steps = 0;
        } else if bun.goal.is_none() && tick < bun.think_at {
            delay = (bun.think_at - tick).min(10);
        } else if bun.goal.is_none() {
            bun.cycle = bun.cycle.wrapping_add(1);
            let choice = random(c.id ^ random(bun.cycle));
            bun.goal = Some([
                position[0].floor() as i32 + (choice % 7) as i32 - 3,
                position[2].floor() as i32 + ((choice >> 8) % 7) as i32 - 3,
            ]);
            bun.steps = 16;
        }
        if clear
            && grounded
            && let Some(goal) = bun.goal
        {
            let point = |p: [i32; 2]| [p[0] as f32 + 0.5, position[1], p[1] as f32 + 0.5];
            if let Some(waypoint) = bun.waypoint {
                let target = point(waypoint);
                if (0..3)
                    .map(|i| (position[i] - target[i]).powi(2))
                    .sum::<f32>()
                    < 0.02 * 0.02
                {
                    bun.waypoint = None;
                    bun.steps = bun.steps.saturating_sub(1);
                } else if !c.world.walk_edge(position, target)? {
                    bun.waypoint = None;
                }
            }
            if bun.waypoint.is_none() {
                match c.world.route(position, goal)? {
                    Route::Next(target) if bun.steps > 0 => {
                        bun.waypoint = Some([target[0].floor() as i32, target[2].floor() as i32])
                    }
                    _ => {
                        bun.goal = None;
                        bun.steps = 0;
                        bun.think_at = tick
                            .checked_add(40 + random(bun.cycle ^ c.id) % 41)
                            .ok_or(Error::Exhausted)?;
                        delay = 10;
                    }
                }
            }
        }
        let target = bun
            .waypoint
            .map(|p| [p[0] as f32 + 0.5, position[1], p[1] as f32 + 0.5]);
        let movement = c.world.advance(position, bun.vertical_velocity, target)?;
        bun.vertical_velocity = movement.vertical_velocity;
        bun.grounded = movement.grounded;
        let next = movement.position;
        let dx = next[0] - position[0];
        let dz = next[2] - position[2];
        if dx.abs() + dz.abs() > 0.0001 {
            bun.facing = if dx.abs() > dz.abs() {
                if dx > 0.0 { 1 } else { 3 }
            } else if dz > 0.0 {
                0
            } else {
                2
            };
        }
        plan.next_tick = Some(tick.checked_add(delay).ok_or(Error::Exhausted)?);
        plan.state = Some(Payload::new(bun));
        plan.position = (next != position).then_some(next);
        Ok(plan)
    }
}
pub(crate) fn definition() -> MobileEntity {
    let model = [
        ([-0.32, 0.17, -0.28], [0.32, 0.55, 0.28], 0),
        ([-0.26, 0.12, -0.24], [0.26, 0.65, 0.24], 0),
        ([-0.23, 0.26, 0.245], [0.23, 0.46, 0.305], 1),
        ([-0.26, 0.60, -0.03], [-0.12, 0.91, 0.09], 0),
        ([0.12, 0.60, -0.03], [0.26, 0.85, 0.09], 0),
        ([-0.225, 0.67, 0.091], [-0.155, 0.85, 0.10], 2),
        ([0.155, 0.66, 0.091], [0.225, 0.79, 0.10], 2),
        ([-0.235, 0.0, -0.17], [-0.075, 0.19, 0.22], 5),
        ([0.075, 0.0, -0.17], [0.235, 0.19, 0.22], 6),
        ([-0.22, 0.44, 0.281], [-0.105, 0.565, 0.303], 3),
        ([0.105, 0.44, 0.281], [0.22, 0.565, 0.303], 3),
        ([-0.202, 0.516, 0.304], [-0.167, 0.552, 0.311], 4),
        ([0.123, 0.516, 0.304], [0.158, 0.552, 0.311], 4),
        ([-0.04, 0.376, 0.307], [0.04, 0.42, 0.322], 2),
        ([-0.012, 0.346, 0.307], [0.012, 0.377, 0.32], 3),
        ([-0.29, 0.35, 0.282], [-0.22, 0.405, 0.30], 2),
        ([0.22, 0.35, 0.282], [0.29, 0.405, 0.30], 2),
        ([-0.10, 0.25, -0.35], [0.10, 0.45, -0.27], 1),
    ]
    .into_iter()
    .map(|(min, max, part)| Cuboid {
        min,
        max,
        color: match part {
            0 => [0.49, 0.77, 0.58],
            1 | 5 | 6 => [0.96, 0.88, 0.68],
            2 => [0.93, 0.49, 0.51],
            3 => [0.025, 0.045, 0.05],
            _ => [1.0, 0.97, 0.86],
        },
        motion: match part {
            5 => PartMotion::LeftFoot,
            6 => PartMotion::RightFoot,
            _ => PartMotion::Body,
        },
    })
    .collect();
    MobileEntity {
        authored_model: None,
        key: "bloxgloom:mossbun".into(),
        schema_version: 2,
        schema_fingerprint: 0x4d4f_5353_4255_0002,
        max_state_bytes: 41,
        max_public_bytes: 2,
        body: Body {
            half_width: 0.36,
            height: 0.94,
            speed: 1.5625,
        },
        interval: 1,
        read_radius: 1,
        reads_neighbours: false,
        wakes_on_terrain_change: true,
        model,
        animation: Animation::default(),
        interaction: vec![],
        behavior: Arc::new(Wander),
    }
}
