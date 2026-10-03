//! Startup-authored axis aligned regions. Feet positions use half-open bounds.
use crate::RegistrationError;

pub const MAX_REGIONS: usize = 256;
#[derive(Clone, Debug, PartialEq)]
pub struct Registration {
    pub key: String,
    pub minimum: [f32; 3],
    pub maximum: [f32; 3],
    /// Existing transactional player lifecycle service receiving the transition.
    pub service: String,
}
impl Registration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if !valid_key(&self.key)
            || !valid_key(&self.service)
            || (0..3).any(|axis| {
                !self.minimum[axis].is_finite()
                    || !self.maximum[axis].is_finite()
                    || self.minimum[axis].abs() > 1_000_000.
                    || self.maximum[axis].abs() > 1_000_000.
                    || self.minimum[axis] >= self.maximum[axis]
            })
        {
            return Err(RegistrationError("invalid region declaration".into()));
        }
        Ok(())
    }
    pub fn contains(&self, feet: [f32; 3]) -> bool {
        (0..3).all(|axis| self.minimum[axis] <= feet[axis] && feet[axis] < self.maximum[axis])
    }
    pub fn contract_bytes(&self) -> Vec<u8> {
        let mut bytes = self.service.as_bytes().to_vec();
        bytes.push(0);
        for value in self.minimum.into_iter().chain(self.maximum) {
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }
}
pub(crate) fn valid_key(key: &str) -> bool {
    key.len() <= 128
        && key.split_once(':').is_some_and(|(namespace, name)| {
            !namespace.is_empty()
                && !name.is_empty()
                && namespace
                    .bytes()
                    .chain(name.bytes())
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        })
}

#[cfg(test)]
mod tests;
