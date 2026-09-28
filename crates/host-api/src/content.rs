//! Startup-only content descriptions. References are canonical keys, never IDs.
//! These expose the engine's existing voxel geometry and material capabilities;
//! arbitrary meshes, partial collision boxes and shaders are not supported.

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
