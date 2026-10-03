//! Baked clip bindings and local camera presentation for a packaged player rig.
use crate::RegistrationError;
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerModel {
    pub idle: Option<String>,
    pub walk: Option<String>,
    pub run: Option<String>,
    pub crouch: Option<String>,
    pub tool_left: Option<String>,
    pub tool_right: Option<String>,
    pub crossfade_s: f32,
    /// Hidden only in the owner's color pass; world shadows retain the rig.
    pub first_person_hide: Vec<String>,
    pub first_person_offset: [f32; 3],
}
impl Default for PlayerModel {
    fn default() -> Self {
        Self {
            idle: None,
            walk: None,
            run: None,
            crouch: None,
            tool_left: None,
            tool_right: None,
            crossfade_s: 0.2,
            first_person_hide: Vec::new(),
            first_person_offset: [0.0; 3],
        }
    }
}
impl PlayerModel {
    pub fn clips(&self) -> [&Option<String>; 6] {
        [
            &self.idle,
            &self.walk,
            &self.run,
            &self.crouch,
            &self.tool_left,
            &self.tool_right,
        ]
    }
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let name = |s: &str| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control);
        if !self.crossfade_s.is_finite()
            || !(0.0..=5.0).contains(&self.crossfade_s)
            || self
                .first_person_offset
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 4.0)
            || self.first_person_hide.len() > 32
            || self
                .clips()
                .iter()
                .any(|n| n.as_ref().is_some_and(|n| !name(n)))
            || self.first_person_hide.iter().any(|n| !name(n))
            || self
                .first_person_hide
                .iter()
                .enumerate()
                .any(|(i, n)| self.first_person_hide[..i].contains(n))
        {
            return Err(RegistrationError(
                "invalid player clip or first-person settings".into(),
            ));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, RegistrationError> {
        self.validate()?;
        let mut bytes = vec![1];
        for clip in self.clips() {
            let s = clip.as_deref().unwrap_or("");
            bytes.push(s.len() as u8);
            bytes.extend(s.as_bytes());
        }
        bytes.extend(self.crossfade_s.to_le_bytes());
        for v in self.first_person_offset {
            bytes.extend(v.to_le_bytes());
        }
        bytes.push(self.first_person_hide.len() as u8);
        for n in &self.first_person_hide {
            bytes.push(n.len() as u8);
            bytes.extend(n.as_bytes());
        }
        Ok(bytes)
    }
    pub fn decode(mut bytes: &[u8]) -> Result<Self, RegistrationError> {
        let invalid = || RegistrationError("invalid player model metadata".into());
        fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
            let (a, b) = bytes.split_at_checked(n)?;
            *bytes = b;
            Some(a)
        }
        fn text(bytes: &mut &[u8]) -> Option<String> {
            let len = take(bytes, 1)?[0] as usize;
            Some(std::str::from_utf8(take(bytes, len)?).ok()?.to_owned())
        }
        if take(&mut bytes, 1) != Some(&[1][..]) {
            return Err(invalid());
        }
        let mut clips = Vec::new();
        for _ in 0..6 {
            let s = text(&mut bytes).ok_or_else(invalid)?;
            clips.push((!s.is_empty()).then_some(s));
        }
        let crossfade_s =
            f32::from_le_bytes(take(&mut bytes, 4).ok_or_else(invalid)?.try_into().unwrap());
        let mut first_person_offset = [0.0; 3];
        for v in &mut first_person_offset {
            *v = f32::from_le_bytes(take(&mut bytes, 4).ok_or_else(invalid)?.try_into().unwrap());
        }
        let count = take(&mut bytes, 1).ok_or_else(invalid)?[0] as usize;
        if count > 32 {
            return Err(invalid());
        }
        let mut first_person_hide = Vec::new();
        for _ in 0..count {
            first_person_hide.push(text(&mut bytes).ok_or_else(invalid)?);
        }
        if !bytes.is_empty() {
            return Err(invalid());
        }
        let mut clips = clips.into_iter();
        let value = Self {
            idle: clips.next().unwrap(),
            walk: clips.next().unwrap(),
            run: clips.next().unwrap(),
            crouch: clips.next().unwrap(),
            tool_left: clips.next().unwrap(),
            tool_right: clips.next().unwrap(),
            crossfade_s,
            first_person_offset,
            first_person_hide,
        };
        value.validate()?;
        Ok(value)
    }
}
#[cfg(test)]
mod tests;
