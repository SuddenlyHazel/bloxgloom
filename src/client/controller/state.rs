//! Pure input state: radial dead zones and held-button fences across UI transitions.
use glam::Vec2;

pub(super) const SOUTH: u16 = 1 << 0;
pub(super) const EAST: u16 = 1 << 1;
pub(super) const WEST: u16 = 1 << 2;
pub(super) const NORTH: u16 = 1 << 3;
pub(super) const START: u16 = 1 << 4;
pub(super) const SELECT: u16 = 1 << 5;
pub(super) const LB: u16 = 1 << 6;
pub(super) const RB: u16 = 1 << 7;
pub(super) const LT: u16 = 1 << 8;
pub(super) const RT: u16 = 1 << 9;
pub(super) const L3: u16 = 1 << 10;

#[derive(Default, Clone, Copy)]
pub(super) struct Snapshot {
    pub movement: Vec2,
    pub look: Vec2,
    pub dpad: Vec2,
    pub buttons: u16,
    pub unavailable: bool,
}

#[derive(Default)]
pub(super) struct Input {
    raw: u16,
    blocked: u16,
    pub held: u16,
    pub pressed: u16,
    pub movement: Vec2,
}
impl Input {
    pub fn update(&mut self, sample: Snapshot) {
        self.raw = sample.buttons;
        self.blocked &= self.raw;
        let held = self.raw & !self.blocked;
        self.pressed = held & !self.held;
        self.held = held;
        self.movement = deadzone(sample.movement);
    }
    pub fn fence(&mut self) {
        self.blocked = self.raw;
        self.held = 0;
        self.pressed = 0;
        self.movement = Vec2::ZERO;
    }
}

pub(super) fn deadzone(value: Vec2) -> Vec2 {
    if !value.is_finite() {
        return Vec2::ZERO;
    }
    let length = value.length();
    if length <= 0.18 {
        Vec2::ZERO
    } else {
        value / length * ((length.min(1.0) - 0.18) / 0.82)
    }
}

pub(super) fn look_delta(value: Vec2, dt: f32) -> Vec2 {
    let value = deadzone(value);
    value * value.length() * 2.6 * dt.clamp(0.0, 0.05)
}
