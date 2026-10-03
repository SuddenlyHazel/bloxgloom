//! Opt-in rigid-body dynamics; omitted policy preserves legacy projectile motion.
use crate::RegistrationError;

pub const MAX_ANGULAR_SPEED: f32 = 8.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Physics {
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub max_angular_speed: f32,
}
impl Default for Physics {
    fn default() -> Self {
        Self {
            linear_damping: 0.0,
            angular_damping: 0.0,
            friction: 0.5,
            max_angular_speed: MAX_ANGULAR_SPEED,
        }
    }
}
impl Physics {
    pub fn validate(self) -> Result<(), RegistrationError> {
        if [self.linear_damping, self.angular_damping]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=32.0).contains(v))
            || !self.friction.is_finite()
            || !(0.0..=4.0).contains(&self.friction)
            || !self.max_angular_speed.is_finite()
            || !(0.0..=MAX_ANGULAR_SPEED).contains(&self.max_angular_speed)
        {
            return Err(RegistrationError(
                "invalid rigid body physics policy".into(),
            ));
        }
        Ok(())
    }
}
