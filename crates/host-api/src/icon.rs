//! Bounded bitmap HUD art; the renderer consumes registered pixels for every item.
use crate::RegistrationError;
#[derive(Clone, Debug)]
pub struct ItemIcon {
    pub item: String,
    /// Up to 32 ASCII rows, each 1..=32 pixels, centered independently.
    /// A dot is transparent; every other symbol must occur in the palette.
    pub rows: Vec<String>,
    pub palette: Vec<(u8, [f32; 4])>,
}
impl ItemIcon {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let mut symbols = std::collections::BTreeSet::new();
        if self.item.len() > 255
            || self.rows.is_empty()
            || self.rows.len() > 32
            || self.palette.len() > 32
            || self.palette.iter().any(|(s, c)| {
                *s == b'.'
                    || !s.is_ascii_graphic()
                    || !symbols.insert(*s)
                    || c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            })
            || self.rows.iter().any(|r| {
                r.is_empty()
                    || r.len() > 32
                    || r.bytes().any(|b| b != b'.' && !symbols.contains(&b))
            })
        {
            return Err(RegistrationError("invalid item icon".into()));
        }
        Ok(())
    }
}
