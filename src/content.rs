//! Startup-built content catalog. Hot-path lookups are dense and read-only after installation.
//!
//! A future mod loader can register named content before `install`; the client and server then
//! share the same frozen definitions. Numeric IDs remain the current save/wire IDs, so the
//! catalog rejects collisions instead of silently changing their meaning.

use std::borrow::Cow;
use std::io::Cursor;
use std::sync::OnceLock;

use crate::world::{self, BlockId};

mod builtins;

pub type TextureId = u8;

pub(crate) const SOLID: u8 = 1;
pub(crate) const OPAQUE: u8 = 2;
pub(crate) const CUTOUT: u8 = 4;
pub(crate) const PLANT: u8 = 8;
pub(crate) const REPLACEABLE: u8 = 16;
pub(crate) const SUPPORTS_PLANT: u8 = 32;

/// One compact source of truth for hot-path builtin physics. Custom definitions use the same
/// bits in the frozen catalog; builtin probes avoid an atomic registry lookup per voxel.
const BUILTIN_FLAGS: [u8; 16] = [
    REPLACEABLE,
    SOLID | OPAQUE | SUPPORTS_PLANT,
    SOLID | OPAQUE | SUPPORTS_PLANT,
    SOLID | OPAQUE,
    SOLID | OPAQUE,
    SOLID | OPAQUE,
    SOLID | OPAQUE | SUPPORTS_PLANT,
    SOLID | OPAQUE,
    SOLID | OPAQUE,
    SOLID | OPAQUE,
    SOLID | CUTOUT,
    CUTOUT | PLANT | REPLACEABLE,
    CUTOUT | PLANT | REPLACEABLE,
    CUTOUT | PLANT | REPLACEABLE,
    CUTOUT | PLANT | REPLACEABLE,
    CUTOUT | PLANT | REPLACEABLE,
];
const BUILTIN_EMISSION: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0, 0, 0];
const BUILTIN_REFLECTANCE: [[u8; 3]; 16] = [
    [115, 120, 128],
    [75, 170, 65],
    [140, 105, 72],
    [115, 120, 128],
    [185, 165, 115],
    [180, 200, 220],
    [75, 170, 65],
    [105, 110, 115],
    [190, 130, 75],
    [145, 105, 65],
    [115, 120, 128],
    [115, 120, 128],
    [115, 120, 128],
    [115, 120, 128],
    [115, 120, 128],
    [115, 120, 128],
];

#[derive(Clone, Debug)]
pub struct TextureDef {
    pub key: Cow<'static, str>,
    pub png: Cow<'static, [u8]>,
    pub stitch_edges: bool,
    pub stitch_vertical: bool,
    pub alpha_cutout: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockTextures {
    pub top: TextureId,
    pub side: TextureId,
    pub bottom: TextureId,
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub id: BlockId,
    pub key: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub swatch: [f32; 4],
    pub textures: BlockTextures,
    pub solid: bool,
    pub opaque: bool,
    pub cutout: bool,
    pub plant: bool,
    pub replaceable: bool,
    pub supports_plant: bool,
    pub emission: u8,
    pub reflectance: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: u8,
    pub key: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub swatch: [f32; 4],
    pub texture: TextureId,
    pub placeable: Option<BlockId>,
    pub sprite: bool,
}

#[allow(dead_code)] // Registration failures are part of the pre-loader catalog API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationError {
    DuplicateId,
    DuplicateKey,
    UnknownTexture,
    UnknownBlock,
    ReservedId,
    TooManyTextures,
    InvalidKey,
    InvalidTexture,
    InvalidDefinition,
}

pub struct Catalog {
    blocks: Vec<Option<BlockDef>>,
    items: Vec<Option<ItemDef>>,
    block_flags: [u8; 256],
    block_emission: [u8; 256],
    block_reflectance: [[u8; 3]; 256],
    primary_block_items: [Option<u8>; 256],
    textures: Vec<TextureDef>,
}

impl Catalog {
    fn new() -> Self {
        Self {
            blocks: vec![None; 256],
            items: vec![None; 256],
            block_flags: [0; 256],
            block_emission: [0; 256],
            block_reflectance: [[115, 120, 128]; 256],
            primary_block_items: [None; 256],
            textures: Vec::new(),
        }
    }

    #[allow(dead_code)] // Used by future mod registration, and exercised by catalog tests.
    pub fn register_texture(
        &mut self,
        definition: TextureDef,
    ) -> Result<TextureId, RegistrationError> {
        if !valid_key(&definition.key) {
            return Err(RegistrationError::InvalidKey);
        }
        if self
            .textures
            .iter()
            .any(|other| other.key == definition.key)
        {
            return Err(RegistrationError::DuplicateKey);
        }
        validate_texture(&definition.png)?;
        let id = TextureId::try_from(self.textures.len())
            .map_err(|_| RegistrationError::TooManyTextures)?;
        self.textures.push(definition);
        Ok(id)
    }

    pub fn register_block(&mut self, definition: BlockDef) -> Result<(), RegistrationError> {
        if !valid_key(&definition.key) {
            return Err(RegistrationError::InvalidKey);
        }
        if definition.emission > 15
            || (definition.opaque && definition.cutout)
            || (definition.plant && (!definition.cutout || definition.solid))
            || (definition.replaceable && definition.solid)
        {
            return Err(RegistrationError::InvalidDefinition);
        }
        if self.blocks[definition.id as usize].is_some() {
            return Err(RegistrationError::DuplicateId);
        }
        if self
            .blocks
            .iter()
            .flatten()
            .any(|other| other.key == definition.key)
        {
            return Err(RegistrationError::DuplicateKey);
        }
        if [
            definition.textures.top,
            definition.textures.side,
            definition.textures.bottom,
        ]
        .iter()
        .any(|&layer| self.texture(layer).is_none())
        {
            return Err(RegistrationError::UnknownTexture);
        }
        let id = definition.id as usize;
        self.block_flags[id] = flags(&definition);
        self.block_emission[id] = definition.emission;
        self.block_reflectance[id] = definition.reflectance;
        self.blocks[id] = Some(definition);
        Ok(())
    }

    pub fn register_item(&mut self, definition: ItemDef) -> Result<(), RegistrationError> {
        if definition.id == 0 {
            return Err(RegistrationError::ReservedId);
        }
        if !valid_key(&definition.key) {
            return Err(RegistrationError::InvalidKey);
        }
        if self.items[definition.id as usize].is_some() {
            return Err(RegistrationError::DuplicateId);
        }
        if self
            .items
            .iter()
            .flatten()
            .any(|other| other.key == definition.key)
        {
            return Err(RegistrationError::DuplicateKey);
        }
        if self.texture(definition.texture).is_none() {
            return Err(RegistrationError::UnknownTexture);
        }
        if definition
            .placeable
            .is_some_and(|id| self.block(id).is_none())
        {
            return Err(RegistrationError::UnknownBlock);
        }
        if let Some(block) = definition.placeable {
            let primary = &mut self.primary_block_items[block as usize];
            *primary = Some(primary.map_or(definition.id, |old| old.min(definition.id)));
        }
        let id = definition.id as usize;
        self.items[id] = Some(definition);
        Ok(())
    }

    #[inline]
    pub fn block(&self, id: BlockId) -> Option<&BlockDef> {
        self.blocks[id as usize].as_ref()
    }

    #[inline]
    fn block_flags(&self, id: BlockId) -> u8 {
        self.block_flags[id as usize]
    }

    #[inline]
    fn emission(&self, id: BlockId) -> u8 {
        self.block_emission[id as usize]
    }

    #[inline]
    fn reflectance(&self, id: BlockId) -> [u8; 3] {
        self.block_reflectance[id as usize]
    }

    #[inline]
    pub fn item(&self, id: u8) -> Option<&ItemDef> {
        self.items[id as usize].as_ref()
    }

    #[inline]
    pub fn texture(&self, id: TextureId) -> Option<&TextureDef> {
        self.textures.get(id as usize)
    }

    pub fn textures(&self) -> &[TextureDef] {
        &self.textures
    }

    pub fn primary_block_item(&self, block: BlockId) -> Option<u8> {
        self.primary_block_items[block as usize]
    }

    /// Wire compatibility, not an art checksum: IDs, behavior, and texture layer ordering must
    /// agree on both sides of a connection.
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut add = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for (layer, texture) in self.textures.iter().enumerate() {
            add(&[b'T', layer as u8]);
            add(texture.key.as_bytes());
            add(&[0, texture.alpha_cutout as u8]);
        }
        for (id, definition) in self.blocks.iter().enumerate() {
            if let Some(block) = definition {
                add(&[b'B', id as u8]);
                add(block.key.as_bytes());
                add(&[
                    0,
                    block.textures.top,
                    block.textures.side,
                    block.textures.bottom,
                    block.solid as u8,
                    block.opaque as u8,
                    block.cutout as u8,
                    block.plant as u8,
                    block.replaceable as u8,
                    block.supports_plant as u8,
                    block.emission,
                ]);
                add(&block.reflectance);
            }
        }
        for (id, definition) in self.items.iter().enumerate() {
            if let Some(item) = definition {
                add(&[b'I', id as u8]);
                add(item.key.as_bytes());
                add(&[
                    0,
                    item.texture,
                    item.placeable.unwrap_or(0),
                    item.sprite as u8,
                ]);
            }
        }
        hash
    }

    /// Stable save identities. Adding definitions is compatible; assigning an existing ID to a
    /// different key is not.
    pub fn identities(&self) -> Vec<(u8, u8, &str)> {
        let mut entries = Vec::new();
        for (id, definition) in self.blocks.iter().enumerate() {
            if let Some(block) = definition {
                entries.push((b'B', id as u8, block.key.as_ref()));
            }
        }
        for (id, definition) in self.items.iter().enumerate() {
            if let Some(item) = definition {
                entries.push((b'I', id as u8, item.key.as_ref()));
            }
        }
        entries
    }
}

fn valid_key(key: &str) -> bool {
    let Some((namespace, path)) = key.split_once(':') else {
        return false;
    };
    key.len() <= u8::MAX as usize
        && !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && path.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'/' | b'.' | b'-')
        })
}

#[allow(dead_code)] // Called by register_texture when external art is supplied.
fn validate_texture(bytes: &[u8]) -> Result<(), RegistrationError> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(RegistrationError::InvalidTexture);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| RegistrationError::InvalidTexture)?;
    let info = reader.info();
    if info.width == 0 || info.height == 0 || info.width > 2048 || info.height > 2048 {
        return Err(RegistrationError::InvalidTexture);
    }
    let size = reader
        .output_buffer_size()
        .ok_or(RegistrationError::InvalidTexture)?;
    if size > 2048 * 2048 * 4 {
        return Err(RegistrationError::InvalidTexture);
    }
    let mut pixels = vec![0; size];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|_| RegistrationError::InvalidTexture)?;
    if !matches!(frame.color_type, png::ColorType::Rgb | png::ColorType::Rgba) {
        return Err(RegistrationError::InvalidTexture);
    }
    Ok(())
}

fn flags(definition: &BlockDef) -> u8 {
    let mut bits = 0;
    if definition.solid {
        bits |= SOLID;
    }
    if definition.opaque {
        bits |= OPAQUE;
    }
    if definition.cutout {
        bits |= CUTOUT;
    }
    if definition.plant {
        bits |= PLANT;
    }
    if definition.replaceable {
        bits |= REPLACEABLE;
    }
    if definition.supports_plant {
        bits |= SUPPORTS_PLANT;
    }
    bits
}

static ACTIVE: OnceLock<Catalog> = OnceLock::new();

pub fn install(catalog: Catalog) -> Result<(), Box<Catalog>> {
    ACTIVE.set(catalog).map_err(Box::new)
}

#[inline]
pub fn catalog() -> &'static Catalog {
    #[cfg(test)]
    {
        ACTIVE.get_or_init(Catalog::builtins)
    }
    #[cfg(not(test))]
    {
        ACTIVE
            .get()
            .expect("content catalog must be installed before startup")
    }
}

#[inline]
pub fn block_def(id: BlockId) -> Option<&'static BlockDef> {
    catalog().block(id)
}

#[inline]
pub fn item_def(id: u8) -> Option<&'static ItemDef> {
    catalog().item(id)
}

#[inline]
pub fn block_flags(id: BlockId) -> u8 {
    if id <= world::MAX_BUILTIN_BLOCK {
        BUILTIN_FLAGS[id as usize]
    } else {
        catalog().block_flags(id)
    }
}

#[inline]
pub fn emission(id: BlockId) -> u8 {
    if id <= world::MAX_BUILTIN_BLOCK {
        BUILTIN_EMISSION[id as usize]
    } else {
        catalog().emission(id)
    }
}

#[inline]
pub fn reflectance(id: BlockId) -> [u8; 3] {
    if id <= world::MAX_BUILTIN_BLOCK {
        BUILTIN_REFLECTANCE[id as usize]
    } else {
        catalog().reflectance(id)
    }
}

#[cfg(test)]
#[path = "content/tests.rs"]
mod tests;
