//! Startup-only content descriptions. References are canonical keys, never IDs.
//! These expose the engine's existing voxel geometry and material capabilities;
//! arbitrary meshes, partial collision boxes and shaders are not supported.

mod drop_policy;
pub use drop_policy::DropPolicy;

#[derive(Clone, Debug)]
pub struct Texture {
    pub key: String,
    /// Complete PNG, at most 4 MiB and 2048 by 2048 pixels.
    pub png: std::borrow::Cow<'static, [u8]>,
    pub stitch_edges: bool,
    pub stitch_vertical: bool,
    pub alpha_cutout: bool,
    /// Surface radiance multiplier, 0–16. Independent of voxel light emission;
    /// the builtin glowstone material uses 3.5. Applies to cube/cutout surfaces.
    pub emission_strength: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceTextures {
    pub top: String,
    pub side: String,
    pub bottom: String,
}
impl FaceTextures {
    pub fn uniform(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            top: key.clone(),
            side: key.clone(),
            bottom: key,
        }
    }
}

/// Existing geometry/selection profiles. Cubes select the voxel; plants select
/// a centered box of height .9 and width .56 (normal) or .30 (narrow grass).
/// Both plant profiles render the same crossed quads and have no collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    Cube,
    CrossedPlant,
    NarrowCrossedPlant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Opaque,
    Cutout,
    Invisible,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub key: String,
    pub name: String,
    pub swatch: [f32; 4],
    pub textures: FaceTextures,
    pub geometry: Geometry,
    pub material: Material,
    pub solid: bool,
    pub replaceable: bool,
    pub supports_plant: bool,
    pub flammable: bool,
    pub emission: u8,
    pub reflectance: [u8; 3],
    /// At most eight properties, with at most 4096 combinations. Only explicitly
    /// declared states are legal; no implicit Cartesian-product allocation.
    pub properties: Vec<Property>,
    pub states: Vec<BlockState>,
}

#[derive(Clone, Debug, Default)]
pub struct BlockState {
    /// Sorted by the host to form `namespace:block[name=value,...]`.
    pub properties: Vec<(String, String)>,
    pub textures: Option<FaceTextures>,
    pub emission: Option<u8>,
}

/// Component bytes retain exact equality when stacking, moving and persisting.
/// The host validates the declared version and size, not private byte semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Components {
    /// Existing built-in behavior: any nonzero version and 1–1024 opaque bytes.
    Unstructured,
    None,
    Opaque {
        version: u16,
        fingerprint: u64,
        max_bytes: u16,
        required: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Item {
    pub key: String,
    pub name: String,
    pub swatch: [f32; 4],
    pub texture: String,
    pub placeable: Option<String>,
    pub sprite: bool,
    /// Client-only world-drop size; does not affect stack, pickup, or placement rules.
    pub drop_size: DropSize,
    /// Client-only world-drop motion. Server age and pickup events remain authoritative.
    pub drop_animation: DropAnimation,
    /// Server-owned falling, pickup, merging and expiry. Frozen with item identity.
    pub drop_policy: DropPolicy,
    /// The stack cap is always 128; components cannot override conservation.
    pub components: Components,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl DropSize {
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::Small => 0.75,
            Self::Normal => 1.0,
            Self::Large => 1.25,
        }
    }
}

/// Bounded presentation parameters: durations in seconds, heights in blocks,
/// hover/spin speeds in radians/second, and pickup turn in total radians.
/// Defaults reproduce the builtin pop, hover, spin and pickup flight exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropAnimation {
    pub pop_duration: f32,
    pub pop_height: f32,
    pub hover_amplitude: f32,
    pub hover_speed: f32,
    pub spin_speed: f32,
    pub pickup_duration: f32,
    pub pickup_arc: f32,
    pub pickup_turn: f32,
}

impl Default for DropAnimation {
    fn default() -> Self {
        Self {
            pop_duration: 0.55,
            pop_height: 0.75,
            hover_amplitude: 0.07,
            hover_speed: 2.6,
            spin_speed: 2.1,
            pickup_duration: 0.34,
            pickup_arc: 0.32,
            pickup_turn: 5.0,
        }
    }
}

impl DropAnimation {
    pub const BYTE_LEN: usize = 32;

    pub fn valid(self) -> bool {
        fn bounded(value: f32, min: f32, max: f32) -> bool {
            value.is_finite()
                && (min..=max).contains(&value)
                && !(value == 0.0 && value.is_sign_negative())
        }
        bounded(self.pop_duration, 0.05, 4.0)
            && bounded(self.pop_height, 0.0, 2.0)
            && bounded(self.hover_amplitude, 0.0, 0.5)
            && bounded(self.hover_speed, 0.0, 16.0)
            && bounded(self.spin_speed, 0.0, 20.0)
            && bounded(self.pickup_duration, 0.05, 4.0)
            && bounded(self.pickup_arc, 0.0, 2.0)
            && bounded(self.pickup_turn, 0.0, 20.0)
    }

    pub fn to_bytes(self) -> [u8; Self::BYTE_LEN] {
        let mut bytes = [0; Self::BYTE_LEN];
        for (index, value) in [
            self.pop_duration,
            self.pop_height,
            self.hover_amplitude,
            self.hover_speed,
            self.spin_speed,
            self.pickup_duration,
            self.pickup_arc,
            self.pickup_turn,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: [u8; Self::BYTE_LEN]) -> Option<Self> {
        let read = |index: usize| {
            f32::from_bits(u32::from_le_bytes(
                bytes[index * 4..index * 4 + 4].try_into().unwrap(),
            ))
        };
        let animation = Self {
            pop_duration: read(0),
            pop_height: read(1),
            hover_amplitude: read(2),
            hover_speed: read(3),
            spin_speed: read(4),
            pickup_duration: read(5),
            pickup_arc: read(6),
            pickup_turn: read(7),
        };
        animation.valid().then_some(animation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TagKind {
    Block,
    Item,
}

#[derive(Clone, Debug)]
pub enum TagMember {
    Definition(String),
    Tag(String),
}

/// Contributions with the same key/kind are unioned, never last-writer-wins.
/// Nested tags must have the same kind; missing references and cycles fail startup.
#[derive(Clone, Debug)]
pub struct Tag {
    pub key: String,
    pub kind: TagKind,
    pub members: Vec<TagMember>,
}
