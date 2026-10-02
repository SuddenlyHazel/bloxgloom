//! Bounded, public character selections shared by server, protocol and client.
//! These are catalog IDs and color bytes, never paths, profile IDs or model data.
mod catalog;
pub(crate) fn fingerprint() -> &'static [u8; 32] {
    catalog::fingerprint()
}

pub(crate) const CHARACTER_RECIPE_BYTES: usize = 12;
pub(crate) const MAX_APPEARANCE_BYTES: usize = 4 + CHARACTER_RECIPE_BYTES;
pub(crate) const BODIES: [&str; 2] = ["flat_chest", "defined_chest_sports_bra"];
/// sRGB highlight color from the kit's default tousled-crop metadata (#BE7940).
pub(crate) const DEFAULT_HAIR_COLOR: [u8; 3] = [190, 121, 64];
pub(crate) const EYES: [&str; 1] = ["authored"];
pub(crate) const MOUTHS: [&str; 1] = ["none"];
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
    pub body: u8,
    /// sRGB color bytes, decoded to linear light once by the renderer.
    pub hair_color: [u8; 3],
    pub hair: u8,
    pub eyes: u8,
    pub mouth: u8,
    /// None retains the authored iris pixels, rather than a guessed default tint.
    pub iris: Option<[u8; 3]>,
}

impl Default for CharacterRecipe {
    fn default() -> Self {
        Self {
            body: 0,
            hair_color: DEFAULT_HAIR_COLOR,
            hair: 1,
            eyes: 0,
            mouth: 0,
            iris: None,
        }
    }
}

impl CharacterRecipe {
    pub fn valid(self) -> bool {
        usize::from(self.body) < BODIES.len()
            && usize::from(self.hair) < HAIR.len()
            && usize::from(self.eyes) < EYES.len()
            && usize::from(self.mouth) < MOUTHS.len()
    }
    pub fn encode(self) -> [u8; CHARACTER_RECIPE_BYTES] {
        let rgb = self.iris.unwrap_or([0; 3]);
        [
            3,
            self.body,
            self.hair,
            self.eyes,
            self.mouth,
            u8::from(self.iris.is_some()),
            rgb[0],
            rgb[1],
            rgb[2],
            self.hair_color[0],
            self.hair_color[1],
            self.hair_color[2],
        ]
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        // Earlier character families are intentionally incompatible. Prerelease
        // worlds are never converted or reset; v23 uses a fresh folder.
        let [
            version,
            body,
            hair,
            eyes,
            mouth,
            enabled,
            r,
            g,
            b,
            hr,
            hg,
            hb,
        ]: [u8; CHARACTER_RECIPE_BYTES] = bytes.try_into().ok()?;
        if version != 3 || enabled > 1 || (enabled == 0 && [r, g, b] != [0; 3]) {
            return None;
        }
        let recipe = Self {
            body,
            hair_color: [hr, hg, hb],
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
    /// None uses the articulated default with these same palettes.
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
