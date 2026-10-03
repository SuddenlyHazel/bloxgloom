//! Copperling: a low orange patrol creature. Right-click toggles resting; gravity
//! continues during rest. All imports come from the supported public contract.
use bloxgloom_host_api::entity::*;
use std::sync::Arc;
pub const KEY: &str = "fixture:copperling";
#[derive(Clone, Copy, Default)]
struct State {
    phase: u8,
    paused: bool,
    grounded: bool,
    velocity: f32,
    home: Option<[i32; 2]>,
    yaw: f32,
}
struct Patrol;
impl Behavior for Patrol {
    fn initial(&self) -> Payload {
        Payload::new(State::default())
    }
    fn encode(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        let s = state.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        if s.phase > 3
            || !s.velocity.is_finite()
            || !(-24.0..=0.0).contains(&s.velocity)
            || !s.yaw.is_finite()
            || s.home
                .into_iter()
                .flatten()
                .any(|v| !(-999_999..999_999).contains(&v))
        {
            return Err(Error::InvalidState);
        }
        let mut out = vec![
            1,
            s.phase,
            s.paused as u8,
            s.grounded as u8,
            s.home.is_some() as u8,
        ];
        out.extend(s.velocity.to_le_bytes());
        out.extend(s.yaw.to_le_bytes());
        for p in s.home.unwrap_or([0; 2]) {
            out.extend(p.to_le_bytes());
        }
        Ok(out)
    }
    fn decode(&self, b: &[u8]) -> Result<Payload, Error> {
        if b.len() != 21 || b[0] != 1 || b[2] > 1 || b[3] > 1 || b[4] > 1 {
            return Err(Error::InvalidState);
        }
        let home = [
            i32::from_le_bytes(b[13..17].try_into().unwrap()),
            i32::from_le_bytes(b[17..21].try_into().unwrap()),
        ];
        if b[4] == 0 && home != [0; 2] {
            return Err(Error::InvalidState);
        }
        let s = Payload::new(State {
            phase: b[1],
            paused: b[2] == 1,
            grounded: b[3] == 1,
            home: (b[4] == 1).then_some(home),
            velocity: f32::from_le_bytes(b[5..9].try_into().unwrap()),
            yaw: f32::from_le_bytes(b[9..13].try_into().unwrap()),
        });
        self.encode(&s)?;
        Ok(s)
    }
    fn public(&self, state: &Payload) -> Result<Vec<u8>, Error> {
        self.encode(state)?;
        let s = state.downcast_ref::<State>().unwrap();
        let mut b = vec![s.grounded as u8, s.paused as u8];
        b.extend(s.yaw.to_le_bytes());
        Ok(b)
    }
    fn pose(&self, b: &[u8]) -> Result<Pose, Error> {
        if b.len() != 6 || b[0] > 1 || b[1] > 1 {
            return Err(Error::InvalidState);
        }
        let yaw = f32::from_le_bytes(b[2..].try_into().unwrap());
        if !yaw.is_finite() {
            return Err(Error::InvalidState);
        }
        Ok(Pose {
            yaw,
            grounded: b[0] == 1,
        })
    }
    fn interact(&self, state: &Payload, request: &[u8]) -> Result<Payload, Error> {
        if request != [1] {
            return Err(Error::InvalidState);
        }
        let mut s = *state.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        s.paused = !s.paused;
        Ok(Payload::new(s))
    }
    fn tick(&self, c: &Context<'_>) -> Result<Plan, Error> {
        let mut s = *c.state.downcast_ref::<State>().ok_or(Error::InvalidState)?;
        if c.next_tick.is_some_and(|due| due > c.tick) {
            return Ok(Plan {
                state: None,
                next_tick: c.next_tick,
                position: None,
                lifecycle: Lifecycle::default(),
            });
        }
        let home = *s
            .home
            .get_or_insert([c.position[0].floor() as i32, c.position[2].floor() as i32]);
        let offset = [[3, 0], [3, 3], [0, 3], [0, 0]][s.phase as usize];
        let goal = [home[0] + offset[0], home[1] + offset[1]];
        let target = if !s.paused && c.world.grounded(c.position)? {
            match c.world.route(c.position, goal)? {
                Route::Next(p) => Some(p),
                Route::Arrived | Route::Unreachable | Route::BudgetExhausted => {
                    s.phase = (s.phase + 1) % 4;
                    None
                }
            }
        } else {
            None
        };
        let m = c.world.advance(c.position, s.velocity, target)?;
        s.velocity = m.vertical_velocity;
        s.grounded = m.grounded;
        let dx = m.position[0] - c.position[0];
        let dz = m.position[2] - c.position[2];
        if dx.abs() + dz.abs() > 0.0001 {
            s.yaw = dx.atan2(dz);
        }
        Ok(Plan {
            state: Some(Payload::new(s)),
            next_tick: Some(c.tick.checked_add(1).ok_or(Error::Exhausted)?),
            position: (m.position != c.position).then_some(m.position),
            lifecycle: Lifecycle::default(),
        })
    }
}
pub fn definition() -> MobileEntity {
    let orange = [0.91, 0.42, 0.12];
    let dark = [0.11, 0.055, 0.025];
    let model = [
        (
            [-0.27, 0.12, -0.25],
            [0.27, 0.39, 0.25],
            orange,
            PartMotion::Body,
        ),
        (
            [-0.18, 0.39, -0.18],
            [0.18, 0.52, 0.18],
            [0.98, 0.66, 0.21],
            PartMotion::Body,
        ),
        (
            [-0.08, 0.52, -0.06],
            [0.08, 0.69, 0.06],
            orange,
            PartMotion::Body,
        ),
        (
            [-0.29, 0.0, -0.2],
            [-0.13, 0.16, 0.2],
            dark,
            PartMotion::LeftFoot,
        ),
        (
            [0.13, 0.0, -0.2],
            [0.29, 0.16, 0.2],
            dark,
            PartMotion::RightFoot,
        ),
        (
            [-0.2, 0.26, 0.251],
            [-0.1, 0.34, 0.27],
            dark,
            PartMotion::Body,
        ),
        (
            [0.1, 0.26, 0.251],
            [0.2, 0.34, 0.27],
            dark,
            PartMotion::Body,
        ),
    ]
    .into_iter()
    .map(|(min, max, color, motion)| Cuboid {
        min,
        max,
        color,
        motion,
    })
    .collect();
    MobileEntity {
        authored_model: None,
        key: KEY.into(),
        schema_version: 1,
        schema_fingerprint: 0x434f_5050_4552_0001,
        max_state_bytes: 21,
        max_public_bytes: 6,
        body: Body {
            half_width: 0.3,
            height: 0.72,
            speed: 2.4,
        },
        interval: 1,
        read_radius: 1,
        reads_neighbours: false,
        wakes_on_terrain_change: true,
        model,
        animation: Animation {
            stride_rate: 16.0,
            stride_amplitude: 0.7,
            walk_bob: 0.008,
            ..Animation::default()
        },
        interaction: vec![1],
        behavior: Arc::new(Patrol),
    }
}
