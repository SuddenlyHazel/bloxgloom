//! Bounded, public character selections shared by server, protocol and client.
//! These are catalog IDs and color bytes, never paths, profile IDs or model data.
mod catalog;
pub(crate) fn fingerprint() -> &'static [u8; 32] {
    catalog::fingerprint()
}

pub(crate) const MAX_APPEARANCE_BYTES: usize = 12;
pub(crate) const EYES: [&str; 8] = [
    "classic",
    "cute_glint",
    "kawaii_star",
    "playful_wink",
    "happy_crescent",
    "neon_focus",
    "neon_curious",
    "soft_sleepy",
];
pub(crate) const MOUTHS: [&str; 6] = [
    "classic",
    "soft_smile",
    "cat_smile",
    "tiny_open",
    "playful",
    "smirk",
];
pub(crate) const HAIR: [&str; 14] = [
    "none",
    "tousled_crop",
    "side_swept_undercut",
    "space_buns",
    "curly_bob",
    "curly_pigtails",
    "sidepart_bob",
    "compact_braid",
    "long_loose_curls",
    "long_curly_ponytail",
    "half_up_curly_cascade",
    "rounded_afro",
    "twin_braids",
    "curly_mohawk",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CharacterRecipe {
    pub hair: u8,
    pub eyes: u8,
    pub mouth: u8,
    /// None retains the authored iris pixels, rather than a guessed default tint.
    pub iris: Option<[u8; 3]>,
}

impl Default for CharacterRecipe {
    fn default() -> Self {
        Self {
            hair: 1,
            eyes: 1,
            mouth: 1,
            iris: None,
        }
    }
}

impl CharacterRecipe {
    pub fn valid(self) -> bool {
        usize::from(self.hair) < HAIR.len()
            && usize::from(self.eyes) < EYES.len()
            && usize::from(self.mouth) < MOUTHS.len()
    }
    pub fn encode(self) -> [u8; 8] {
        let rgb = self.iris.unwrap_or([0; 3]);
        [
            1,
            self.hair,
            self.eyes,
            self.mouth,
            u8::from(self.iris.is_some()),
            rgb[0],
            rgb[1],
            rgb[2],
        ]
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let [version, hair, eyes, mouth, enabled, r, g, b]: [u8; 8] = bytes.try_into().ok()?;
        if version != 1 || enabled > 1 || (enabled == 0 && [r, g, b] != [0; 3]) {
            return None;
        }
        let recipe = Self {
            hair,
            eyes,
            mouth,
            iris: (enabled == 1).then_some([r, g, b]),
        };
        recipe.valid().then_some(recipe)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AppearanceState {
    pub palettes: [u8; 3],
    pub character: Option<CharacterRecipe>,
}

impl AppearanceState {
    pub fn legacy(self) -> [u8; 4] {
        [self.palettes[0], self.palettes[1], self.palettes[2], 0]
    }
    pub fn encode(self) -> Vec<u8> {
        let mut bytes = self.legacy().to_vec();
        if let Some(recipe) = self.character {
            bytes.extend(recipe.encode());
        }
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if !matches!(bytes.len(), 4 | MAX_APPEARANCE_BYTES) || bytes[3] != 0 {
            return None;
        }
        let character = if bytes.len() == 4 {
            None
        } else {
            Some(CharacterRecipe::decode(&bytes[4..])?)
        };
        Some(Self {
            palettes: [bytes[0], bytes[1], bytes[2]],
            character,
        })
    }
}

#[cfg(test)]
mod tests;
