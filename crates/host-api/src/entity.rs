//! Immutable mobile-entity contracts. Hooks must be deterministic, side-effect
//! free and thread safe. A future sandbox can marshal these contexts and effects;
//! the in-process Rust implementation is not a stable native plugin ABI.
use std::{any::Any, fmt, sync::Arc};

mod falling;
pub use falling::{FallingContext, FallingPlan, FallingWorld};
mod drop_merge;
pub use drop_merge::{DropMergeCandidate, DropMergeContext};
mod drop_pickup;
pub use drop_pickup::{DropLifetime, DropPickupContext};

#[derive(Clone)]
pub struct Payload(Arc<dyn Any + Send + Sync>);
impl Payload {
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl fmt::Debug for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Payload(<decoded>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidState,
    OutOfRange,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub half_width: f32,
    pub height: f32,
    pub speed: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub position: [f32; 3],
    pub vertical_velocity: f32,
    pub grounded: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    Arrived,
    Next([f32; 3]),
    Unreachable,
    BudgetExhausted,
}
pub struct Neighbour<'a> {
    pub id: u64,
    pub key: &'a str,
    pub position: [f32; 3],
    pub public: &'a [u8],
}
/// Read-only services use only the coordinator's bounded captured inputs. An
/// out-of-range read must fail the plan, never substitute procedural terrain.
pub trait World {
    fn solid(&self, cell: [i32; 3]) -> Result<bool, Error>;
    fn clear(&self, position: [f32; 3]) -> Result<bool, Error>;
    fn grounded(&self, position: [f32; 3]) -> Result<bool, Error>;
    fn walk_edge(&self, from: [f32; 3], to: [f32; 3]) -> Result<bool, Error>;
    fn route(&self, position: [f32; 3], goal: [i32; 2]) -> Result<Route, Error>;
    fn advance(
        &self,
        position: [f32; 3],
        vertical_velocity: f32,
        target: Option<[f32; 3]>,
    ) -> Result<Movement, Error>;
}
pub struct Context<'a> {
    pub id: u64,
    pub tick: u64,
    pub next_tick: Option<u64>,
    pub position: [f32; 3],
    pub state: &'a Payload,
    pub world: &'a dyn World,
    pub neighbours: &'a [Neighbour<'a>],
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub state: Option<Payload>,
    pub next_tick: Option<u64>,
    pub position: Option<[f32; 3]>,
    pub lifecycle: Lifecycle,
}
#[derive(Clone, Debug, Default)]
pub struct Lifecycle {
    pub spawns: Vec<Spawn>,
    pub despawn: bool,
}
#[derive(Clone, Debug)]
pub struct Spawn {
    pub key: String,
    pub position: [f32; 3],
    pub state: Payload,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub yaw: f32,
    pub grounded: bool,
}
pub trait Behavior: Send + Sync + 'static {
    fn initial(&self) -> Payload;
    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error>;
    fn encode(&self, state: &Payload) -> Result<Vec<u8>, Error>;
    fn public(&self, state: &Payload) -> Result<Vec<u8>, Error>;
    fn pose(&self, public: &[u8]) -> Result<Pose, Error>;
    fn tick(&self, context: &Context<'_>) -> Result<Plan, Error>;
    fn interact(&self, _state: &Payload, _request: &[u8]) -> Result<Payload, Error> {
        Err(Error::InvalidState)
    }
}
#[derive(Clone, Debug)]
pub struct Cuboid {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub color: [f32; 3],
    pub motion: PartMotion,
}
#[derive(Clone, Copy, Debug)]
pub enum PartMotion {
    Body,
    LeftFoot,
    RightFoot,
}
/// Client-only procedural gait. No value changes authoritative movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub stride_rate: f32,
    pub stride_amplitude: f32,
    pub idle_rate: f32,
    pub idle_bob: f32,
    pub walk_bob: f32,
    pub fall_stretch: f32,
    pub landing_squash: f32,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            stride_rate: 10.0,
            stride_amplitude: 1.0,
            idle_rate: 2.5,
            idle_bob: 0.004,
            walk_bob: 0.018,
            fall_stretch: 0.08,
            landing_squash: 0.16,
        }
    }
}
impl Animation {
    fn values(self) -> [f32; 7] {
        [
            self.stride_rate,
            self.stride_amplitude,
            self.idle_rate,
            self.idle_bob,
            self.walk_bob,
            self.fall_stretch,
            self.landing_squash,
        ]
    }
}
#[derive(Clone)]
pub struct MobileEntity {
    pub key: String,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
    pub max_state_bytes: usize,
    pub max_public_bytes: usize,
    pub body: Body,
    pub interval: u32,
    pub read_radius: u8,
    pub reads_neighbours: bool,
    pub wakes_on_terrain_change: bool,
    pub model: Vec<Cuboid>,
    pub animation: Animation,
    /// Opaque default right-click request; empty means not interactable.
    pub interaction: Vec<u8>,
    pub behavior: Arc<dyn Behavior>,
}
impl fmt::Debug for MobileEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MobileEntity")
            .field("key", &self.key)
            .finish()
    }
}
impl MobileEntity {
    pub fn validate(&self) -> Result<(), crate::RegistrationError> {
        let b = self.body;
        if self
            .animation
            .values()
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
            || self.animation.stride_rate > 40.0
            || self.animation.idle_rate > 20.0
            || self.animation.stride_amplitude > 3.0
            || self.animation.idle_bob > 0.1
            || self.animation.walk_bob > 0.2
            || self.animation.fall_stretch > 0.5
            || self.animation.landing_squash > 0.5
            || self.schema_version == 0
            || self.interval == 0
            || self.read_radius > 1
            || self.max_state_bytes == 0
            || self.max_state_bytes > 65536
            || self.max_public_bytes == 0
            || self.max_public_bytes > 4096
            || self.interaction.len() > 128
            || !b.half_width.is_finite()
            || !(0.05..=1.0).contains(&b.half_width)
            || !b.height.is_finite()
            || !(0.1..=3.0).contains(&b.height)
            || !b.speed.is_finite()
            || !(0.0..=4.0).contains(&b.speed)
            || self.model.is_empty()
            || self.model.len() > 64
            || self.model.iter().any(|p| {
                (0..3).any(|i| {
                    !p.min[i].is_finite()
                        || !p.max[i].is_finite()
                        || p.min[i] >= p.max[i]
                        || p.min[i] < -4.0
                        || p.max[i] > 4.0
                        || !p.color[i].is_finite()
                        || !(0.0..=1.0).contains(&p.color[i])
                })
            })
        {
            return Err(crate::RegistrationError(
                "invalid mobile entity bounds".into(),
            ));
        }
        let state = self.behavior.initial();
        let bytes = self
            .behavior
            .encode(&state)
            .map_err(|_| crate::RegistrationError("invalid initial entity state".into()))?;
        let decoded = self
            .behavior
            .decode(&bytes)
            .map_err(|_| crate::RegistrationError("invalid entity codec".into()))?;
        let public = self
            .behavior
            .public(&decoded)
            .map_err(|_| crate::RegistrationError("invalid entity projection".into()))?;
        if bytes.len() > self.max_state_bytes
            || public.len() > self.max_public_bytes
            || self.behavior.encode(&decoded).ok().as_ref() != Some(&bytes)
            || !self
                .behavior
                .pose(&public)
                .is_ok_and(|pose| pose.yaw.is_finite())
        {
            return Err(crate::RegistrationError(
                "invalid entity codec bounds or roundtrip".into(),
            ));
        }
        Ok(())
    }
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut out = vec![
            1,
            self.read_radius,
            self.reads_neighbours as u8,
            self.wakes_on_terrain_change as u8,
        ];
        out.extend(self.interval.to_le_bytes());
        out.extend((self.max_state_bytes as u32).to_le_bytes());
        out.extend((self.max_public_bytes as u32).to_le_bytes());
        for f in [self.body.half_width, self.body.height, self.body.speed] {
            out.extend(f.to_le_bytes());
        }
        for f in self.animation.values() {
            out.extend(f.to_le_bytes());
        }
        out.extend((self.interaction.len() as u32).to_le_bytes());
        out.extend(&self.interaction);
        out.push(self.model.len() as u8);
        for p in &self.model {
            for f in p.min.into_iter().chain(p.max).chain(p.color) {
                out.extend(f.to_le_bytes());
            }
            out.push(match p.motion {
                PartMotion::Body => 0,
                PartMotion::LeftFoot => 1,
                PartMotion::RightFoot => 2,
            });
        }
        out
    }
}
pub fn random(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
