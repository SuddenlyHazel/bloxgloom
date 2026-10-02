//! Startup-built content definitions and compiled, read-only state lookups.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::OnceLock;

use crate::world::{self, BlockId};

mod actions;
mod anchored;
pub(crate) mod appearance;
mod builtins;
pub(crate) mod client_metadata;
mod companions;
pub(crate) mod composition;
pub(crate) mod creatures;
pub(crate) mod declarations;
mod extensions;
mod gameplay;
mod gameplay_entities;
pub(crate) mod icons;
mod ids;
mod inventories;
pub(crate) mod machines;
mod manifest;
mod mobile;
pub(crate) mod moving;
mod observers;
mod owner_systems;
pub(crate) mod player;
mod players;
mod public;
pub use ids::{BlockStateId, BlockTypeId, EntityTypeId, ItemId, TextureId};
#[allow(unused_imports)] // Public extension and manifest-inspection API.
pub use manifest::{ContentEntry, ContentManifest, MAX_MANIFEST_BYTES};

pub const MAX_BLOCK_TYPES: usize = 32_768;
pub const MAX_BLOCK_STATES: usize = 131_072;
pub const MAX_ITEMS: usize = 32_768;
pub const MAX_ENTITY_TYPES: usize = 32_768;
pub const MAX_TEXTURES: usize = 8_192;
pub const MAX_ASSIGNED_ID: u32 = 1_048_576;

/// Stable builtin kiln identifiers. Kiln content is allocated in the extension
/// range so it does not collide with the original voxel and item IDs.
pub const KILN_BLOCK_TYPE: BlockTypeId = BlockTypeId(256);
pub const KILN_DEFAULT_STATE: BlockStateId = BlockStateId(512);
pub const KILN_ITEM: ItemId = ItemId(256);
#[cfg(test)]
pub const KILN_STATE_COUNT: u32 = 16;
pub const KILN_ENTITY_TYPE: EntityTypeId = EntityTypeId(3);
pub const KILN_SCHEMA_VERSION: u16 = 4;
pub const KILN_SCHEMA_FINGERPRINT: u64 = 0x4b49_4c4e_0000_0004;
pub const MOSSBUN_ENTITY_TYPE: EntityTypeId = EntityTypeId(4);
pub const MOSSBUN_SCHEMA_VERSION: u16 = 2;
pub const MOSSBUN_SCHEMA_FINGERPRINT: u64 = 0x4d4f_5353_4255_0002;
pub const HOPPER_BLOCK_TYPE: BlockTypeId = BlockTypeId(300);
pub const HOPPER_STATE: BlockStateId = BlockStateId(600);
pub const HOPPER_ITEM: ItemId = ItemId(300);
pub const HOPPER_ENTITY_TYPE: EntityTypeId = EntityTypeId(5);
pub const CHEST_BLOCK_TYPE: BlockTypeId = BlockTypeId(301);
pub const CHEST_STATE: BlockStateId = BlockStateId(601);
pub const CHEST_ITEM: ItemId = ItemId(301);
pub const CHEST_ENTITY_TYPE: EntityTypeId = EntityTypeId(6);

pub(crate) const SOLID: u8 = 1;
pub(crate) const OPAQUE: u8 = 2;
pub(crate) const CUTOUT: u8 = 4;
pub(crate) const PLANT: u8 = 8;
pub(crate) const REPLACEABLE: u8 = 16;
pub(crate) const SUPPORTS_PLANT: u8 = 32;
pub(crate) const FLAMMABLE: u8 = 64;

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
    SOLID | OPAQUE | FLAMMABLE,
    SOLID | CUTOUT | FLAMMABLE,
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
    pub emission_strength: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockTextures {
    pub top: TextureId,
    pub side: TextureId,
    pub bottom: TextureId,
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub id: BlockTypeId,
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
    pub flammable: bool,
    pub emission: u8,
    pub reflectance: [u8; 3],
    /// A bounded set of named choices from which legal state keys are compiled.
    pub properties: Vec<PropertyDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDef {
    pub name: Cow<'static, str>,
    pub values: Vec<Cow<'static, str>>,
}

#[derive(Clone, Debug)]
pub struct StateDef {
    pub id: BlockStateId,
    pub block_type: BlockTypeId,
    pub key: String,
    pub properties: Vec<(String, String)>,
    pub textures: BlockTextures,
    /// Compiled face textures in [-X, +X, -Y, +Y, -Z, +Z] order.
    pub face_textures: [TextureId; 6],
    pub flags: u8,
    pub emission: u8,
    pub reflectance: [u8; 3],
}

impl StateDef {
    #[inline]
    pub fn face_texture(&self, axis: usize, side: i32) -> Option<TextureId> {
        self.face_textures
            .get(axis.checked_mul(2)? + usize::from(side > 0))
            .copied()
    }
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: ItemId,
    pub key: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub swatch: [f32; 4],
    pub texture: TextureId,
    pub placeable: Option<BlockStateId>,
    pub sprite: bool,
}

#[derive(Clone, Debug)]
pub struct EntityTypeDef {
    pub id: EntityTypeId,
    pub key: Cow<'static, str>,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
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
    TooManyDefinitions,
    InvalidState,
}

#[derive(Clone, Debug)]
pub struct Catalog {
    block_acoustics: HashMap<String, bloxgloom_host_api::content::Acoustics>,
    pub(crate) sounds: HashSet<String>,
    player_appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    player_selection: Option<player::Selection>,
    player_rules: bloxgloom_host_api::player::PlayerRules,
    // Negotiated compatibility metadata, never executable registrations.
    client_metadata: client_metadata::Metadata,
    gameplay_entities:
        HashMap<String, std::sync::Arc<bloxgloom_host_api::gameplay::EntityDefinition>>,
    gameplay_dispatch: HashMap<
        bloxgloom_host_api::gameplay::EventKind,
        HashMap<String, std::sync::Arc<bloxgloom_host_api::gameplay::HandlerRegistration>>,
    >,
    gameplay_handlers: std::collections::BTreeMap<
        u32,
        std::sync::Arc<bloxgloom_host_api::gameplay::HandlerRegistration>,
    >,
    gameplay_observers: std::collections::BTreeMap<
        u32,
        std::sync::Arc<bloxgloom_host_api::gameplay::ObserverRegistration>,
    >,
    pub(crate) item_visuals: std::sync::Arc<crate::client::item_visuals::Cache>,
    item_icons: HashMap<String, std::sync::Arc<bloxgloom_host_api::icon::ItemIcon>>,
    player_lifecycles:
        std::collections::BTreeMap<u32, std::sync::Arc<bloxgloom_host_api::players::Registration>>,
    owner_systems:
        std::collections::BTreeMap<u32, std::sync::Arc<bloxgloom_host_api::system::System>>,
    anchored_blocks: Vec<Option<EntityTypeId>>,
    anchored_entities:
        Vec<Option<std::sync::Arc<bloxgloom_host_api::anchored::AnchoredBlockEntity>>>,
    narrow_plants: HashSet<String>,
    item_components: HashMap<String, bloxgloom_host_api::content::Components>,
    drop_sizes: HashMap<String, bloxgloom_host_api::content::DropSize>,
    drop_animations: HashMap<String, bloxgloom_host_api::content::DropAnimation>,
    drop_policies: HashMap<String, bloxgloom_host_api::content::DropPolicy>,
    max_drop_pickup_range: f32,
    pub(crate) composition: composition::Composition,
    machines: Vec<Option<std::sync::Arc<bloxgloom_host_api::machine::Machine>>>,
    mobile_entities: Vec<Option<std::sync::Arc<bloxgloom_host_api::entity::MobileEntity>>>,
    moving_entities: Vec<Option<std::sync::Arc<bloxgloom_host_api::motion::MovingEntity>>>,
    pub(crate) storage_lifecycles: Vec<bloxgloom_host_api::StorageBlockEntity>,
    inventory_screens: Vec<Option<std::sync::Arc<bloxgloom_host_api::InventoryScreen>>>,
    actions: bloxgloom_host_api::actions::Registry,
    action_outputs: std::collections::BTreeMap<String, ItemId>,
    blocks: Vec<Option<BlockDef>>,
    states: Vec<Option<StateDef>>,
    items: Vec<Option<ItemDef>>,
    entities: Vec<Option<EntityTypeDef>>,
    primary_block_items: Vec<Option<ItemId>>,
    textures: Vec<TextureDef>,
    texture_fingerprints: Vec<u64>,
    block_keys: HashSet<String>,
    state_keys: HashSet<String>,
    state_by_key: HashMap<String, BlockStateId>,
    item_keys: HashSet<String>,
    item_by_key: HashMap<String, ItemId>,
    entity_keys: HashSet<String>,
    texture_keys: HashSet<String>,
    state_counts: HashMap<BlockTypeId, usize>,
    block_count: usize,
    state_count: usize,
    item_count: usize,
    entity_count: usize,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            block_acoustics: Default::default(),
            sounds: [
                "bloxgloom:break",
                "bloxgloom:place",
                "bloxgloom:pickup",
                "bloxgloom:interact",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            player_appearance: None,
            player_selection: None,
            player_rules: bloxgloom_host_api::player::BUILTIN_RULES,
            client_metadata: Default::default(),
            gameplay_entities: Default::default(),
            gameplay_handlers: Default::default(),
            gameplay_observers: Default::default(),
            gameplay_dispatch: Default::default(),
            item_icons: HashMap::new(),
            item_visuals: Default::default(),
            player_lifecycles: Default::default(),
            owner_systems: Default::default(),
            anchored_blocks: Vec::new(),
            anchored_entities: Vec::new(),
            narrow_plants: HashSet::new(),
            item_components: HashMap::new(),
            drop_sizes: HashMap::new(),
            drop_animations: HashMap::new(),
            drop_policies: HashMap::new(),
            max_drop_pickup_range: bloxgloom_host_api::content::DropPolicy::default().pickup_range,
            composition: composition::Composition::default(),
            machines: Vec::new(),
            mobile_entities: Vec::new(),
            moving_entities: Vec::new(),
            storage_lifecycles: Vec::new(),
            inventory_screens: Vec::new(),
            actions: Default::default(),
            action_outputs: Default::default(),
            blocks: Vec::new(),
            states: Vec::new(),
            items: Vec::new(),
            entities: Vec::new(),
            primary_block_items: Vec::new(),
            textures: Vec::new(),
            texture_fingerprints: Vec::new(),
            block_keys: HashSet::new(),
            state_keys: HashSet::new(),
            state_by_key: HashMap::new(),
            item_keys: HashSet::new(),
            item_by_key: HashMap::new(),
            entity_keys: HashSet::new(),
            texture_keys: HashSet::new(),
            state_counts: HashMap::new(),
            block_count: 0,
            state_count: 0,
            item_count: 0,
            entity_count: 0,
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
        if self.texture_keys.contains(definition.key.as_ref()) {
            return Err(RegistrationError::DuplicateKey);
        }
        if !definition.emission_strength.is_finite()
            || !(0.0..=16.0).contains(&definition.emission_strength)
        {
            return Err(RegistrationError::InvalidTexture);
        }
        validate_texture(&definition.png)?;
        if self.textures.len() >= MAX_TEXTURES {
            return Err(RegistrationError::TooManyTextures);
        }
        let id = TextureId(self.textures.len() as u32);
        self.texture_keys.insert(definition.key.to_string());
        self.texture_fingerprints
            .push(fingerprint_texture(&definition));
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
        let id = checked_id(definition.id.0)?;
        if self.blocks.get(id).is_some_and(Option::is_some) {
            return Err(RegistrationError::DuplicateId);
        }
        if self.block_count >= MAX_BLOCK_TYPES {
            return Err(RegistrationError::TooManyDefinitions);
        }
        if definition.properties.len() > 8 || !valid_properties(&definition.properties) {
            return Err(RegistrationError::InvalidDefinition);
        }
        if self.block_keys.contains(definition.key.as_ref()) {
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
        if self.blocks.len() <= id {
            self.blocks.resize_with(id + 1, || None);
        }
        if self.primary_block_items.len() <= id {
            self.primary_block_items.resize(id + 1, None);
        }
        self.block_keys.insert(definition.key.to_string());
        self.block_count += 1;
        self.blocks[id] = Some(definition);
        Ok(())
    }

    /// State keys are canonical: block key followed by sorted `name=value` pairs.
    pub fn register_state(
        &mut self,
        id: BlockStateId,
        block_type: BlockTypeId,
        properties: Vec<(String, String)>,
        textures: Option<BlockTextures>,
    ) -> Result<(), RegistrationError> {
        self.register_state_with_emission(id, block_type, properties, textures, None)
    }

    /// Registers a legal state with an optional emission override. This keeps
    /// light-bearing variants such as an active kiln in the compiled state
    /// catalog instead of adding property lookups to voxel hot paths.
    pub fn register_state_with_emission(
        &mut self,
        id: BlockStateId,
        block_type: BlockTypeId,
        mut properties: Vec<(String, String)>,
        textures: Option<BlockTextures>,
        emission: Option<u8>,
    ) -> Result<(), RegistrationError> {
        let index = checked_id(id.0)?;
        if self.states.get(index).is_some_and(Option::is_some) {
            return Err(RegistrationError::DuplicateId);
        }
        if self.state_count >= MAX_BLOCK_STATES {
            return Err(RegistrationError::TooManyDefinitions);
        }
        let block = self
            .block_type(block_type)
            .ok_or(RegistrationError::UnknownBlock)?;
        properties.sort();
        if emission.is_some_and(|value| value > 15)
            || properties.len() != block.properties.len()
            || !properties
                .iter()
                .zip(&block.properties)
                .all(|((name, value), schema)| {
                    name == schema.name.as_ref()
                        && schema
                            .values
                            .iter()
                            .any(|candidate| candidate.as_ref() == value)
                })
            || self
                .state_counts
                .get(&block_type)
                .is_some_and(|&count| count >= 4_096)
        {
            return Err(RegistrationError::InvalidState);
        }
        let mut key = block.key.to_string();
        if !properties.is_empty() {
            key.push('[');
            for (index, (name, value)) in properties.iter().enumerate() {
                if index != 0 {
                    key.push(',');
                }
                key.push_str(name);
                key.push('=');
                key.push_str(value);
            }
            key.push(']');
        }
        if key.len() > 512 || self.state_keys.contains(&key) {
            return Err(RegistrationError::DuplicateKey);
        }
        let textures = textures.unwrap_or(block.textures);
        if [textures.top, textures.side, textures.bottom]
            .iter()
            .any(|&texture| self.texture(texture).is_none())
        {
            return Err(RegistrationError::UnknownTexture);
        }
        let cap_axis =
            properties
                .iter()
                .find(|(name, _)| name == "axis")
                .map_or(1, |(_, value)| match value.as_str() {
                    "x" => 0,
                    "z" => 2,
                    _ => 1,
                });
        let mut face_textures = [textures.side; 6];
        face_textures[cap_axis * 2] = textures.bottom;
        face_textures[cap_axis * 2 + 1] = textures.top;
        let state = StateDef {
            id,
            block_type,
            key,
            properties,
            textures,
            face_textures,
            flags: flags(block),
            emission: emission.unwrap_or(block.emission),
            reflectance: block.reflectance,
        };
        if self.states.len() <= index {
            self.states.resize_with(index + 1, || None);
        }
        self.state_keys.insert(state.key.clone());
        self.state_by_key.insert(state.key.clone(), id);
        *self.state_counts.entry(block_type).or_default() += 1;
        self.state_count += 1;
        self.states[index] = Some(state);
        Ok(())
    }

    pub fn register_item(&mut self, definition: ItemDef) -> Result<(), RegistrationError> {
        if definition.id.0 == 0 {
            return Err(RegistrationError::ReservedId);
        }
        if !valid_key(&definition.key) {
            return Err(RegistrationError::InvalidKey);
        }
        let id = checked_id(definition.id.0)?;
        if self.items.get(id).is_some_and(Option::is_some) {
            return Err(RegistrationError::DuplicateId);
        }
        if self.item_count >= MAX_ITEMS {
            return Err(RegistrationError::TooManyDefinitions);
        }
        if self.item_keys.contains(definition.key.as_ref()) {
            return Err(RegistrationError::DuplicateKey);
        }
        if self.texture(definition.texture).is_none() {
            return Err(RegistrationError::UnknownTexture);
        }
        if definition
            .placeable
            .is_some_and(|id| self.state(id).is_none())
        {
            return Err(RegistrationError::UnknownBlock);
        }
        if let Some(block) = definition.placeable {
            let type_id = self.state(block).expect("checked state").block_type;
            let primary = &mut self.primary_block_items[type_id.0 as usize];
            *primary = Some(primary.map_or(definition.id, |old| old.min(definition.id)));
        }
        if self.items.len() <= id {
            self.items.resize_with(id + 1, || None);
        }
        self.item_keys.insert(definition.key.to_string());
        self.item_by_key
            .insert(definition.key.to_string(), definition.id);
        self.item_count += 1;
        self.items[id] = Some(definition);
        Ok(())
    }

    pub fn register_entity_type(
        &mut self,
        definition: EntityTypeDef,
    ) -> Result<(), RegistrationError> {
        if definition.id.0 == 0 {
            return Err(RegistrationError::ReservedId);
        }
        let id = checked_id(definition.id.0)?;
        if !valid_key(&definition.key) || definition.schema_version == 0 {
            return Err(RegistrationError::InvalidDefinition);
        }
        if self.entities.get(id).is_some_and(Option::is_some) {
            return Err(RegistrationError::DuplicateId);
        }
        if self.entity_count >= MAX_ENTITY_TYPES {
            return Err(RegistrationError::TooManyDefinitions);
        }
        if self.entity_keys.contains(definition.key.as_ref()) {
            return Err(RegistrationError::DuplicateKey);
        }
        if self.entities.len() <= id {
            self.entities.resize_with(id + 1, || None);
        }
        self.entity_keys.insert(definition.key.to_string());
        self.entity_count += 1;
        self.entities[id] = Some(definition);
        Ok(())
    }

    #[inline]
    pub fn block(&self, id: BlockStateId) -> Option<&BlockDef> {
        let state = self.state(id)?;
        self.block_type(state.block_type)
    }

    #[inline]
    pub fn block_type(&self, id: BlockTypeId) -> Option<&BlockDef> {
        self.blocks.get(id.0 as usize)?.as_ref()
    }

    #[inline]
    pub fn state(&self, id: BlockStateId) -> Option<&StateDef> {
        self.states.get(id.0 as usize)?.as_ref()
    }

    /// Resolves one legal property change without putting property maps in voxel loops.
    pub fn state_with_property(
        &self,
        state: BlockStateId,
        name: &str,
        value: &str,
    ) -> Option<BlockStateId> {
        let source = self.state(state)?;
        let mut properties = source.properties.clone();
        let property = properties.iter_mut().find(|(key, _)| key == name)?;
        property.1 = value.to_owned();
        let mut key = self.block_type(source.block_type)?.key.to_string();
        key.push('[');
        for (index, (name, value)) in properties.iter().enumerate() {
            if index != 0 {
                key.push(',');
            }
            key.push_str(name);
            key.push('=');
            key.push_str(value);
        }
        key.push(']');
        self.state_by_key.get(&key).copied()
    }

    pub fn validate(&self) -> Result<(), RegistrationError> {
        self.player_rules
            .validate()
            .map_err(|_| RegistrationError::InvalidDefinition)?;
        self.validate_player_selection()?;
        if let Some(appearance) = &self.player_appearance {
            appearance
                .validate()
                .map_err(|_| RegistrationError::InvalidDefinition)?;
            if self.entity_type_id_by_key("bloxgloom:player").is_none() {
                return Err(RegistrationError::InvalidDefinition);
            }
        }
        self.actions
            .validate_composition()
            .map_err(|_| RegistrationError::InvalidDefinition)?;
        if self
            .state(BlockStateId(0))
            .is_none_or(|state| state.block_type != BlockTypeId(0))
            || self.item(ItemId(0)).is_some()
            || self.textures.len() != self.texture_fingerprints.len()
            || self.blocks.iter().flatten().any(|block| {
                self.state_counts
                    .get(&block.id)
                    .is_none_or(|&count| count == 0)
            })
        {
            return Err(RegistrationError::InvalidDefinition);
        }
        Ok(())
    }

    #[inline]
    pub fn block_flags(&self, id: BlockStateId) -> u8 {
        self.state(id).map_or(0, |state| state.flags)
    }

    #[inline]
    pub fn emission(&self, id: BlockStateId) -> u8 {
        self.state(id).map_or(0, |state| state.emission)
    }

    #[inline]
    pub fn reflectance(&self, id: BlockStateId) -> [u8; 3] {
        self.state(id)
            .map_or([115, 120, 128], |state| state.reflectance)
    }

    #[inline]
    pub fn item(&self, id: ItemId) -> Option<&ItemDef> {
        self.items.get(id.0 as usize)?.as_ref()
    }

    pub fn items(&self) -> impl Iterator<Item = &ItemDef> {
        self.items.iter().flatten()
    }

    pub fn drop_policy(&self, id: ItemId) -> bloxgloom_host_api::content::DropPolicy {
        self.item(id)
            .and_then(|item| self.drop_policies.get(item.key.as_ref()).copied())
            .unwrap_or_default()
    }

    /// Startup-compiled query envelope, at most eight blocks. Ordinary catalogs
    /// retain the stock envelope instead of capturing unrelated distant drops.
    pub fn max_drop_pickup_range(&self) -> f32 {
        self.max_drop_pickup_range
    }

    pub fn drop_size(&self, id: ItemId) -> bloxgloom_host_api::content::DropSize {
        self.item(id)
            .and_then(|item| self.drop_sizes.get(item.key.as_ref()).copied())
            .unwrap_or_default()
    }

    #[inline]
    pub fn drop_animation(&self, id: ItemId) -> bloxgloom_host_api::content::DropAnimation {
        self.item(id)
            .and_then(|item| self.drop_animations.get(item.key.as_ref()).copied())
            .unwrap_or_default()
    }

    pub(crate) fn item_by_key(&self, key: &str) -> Option<ItemId> {
        self.item_by_key.get(key).copied()
    }

    #[inline]
    pub fn entity_type(&self, id: EntityTypeId) -> Option<&EntityTypeDef> {
        self.entities.get(id.0 as usize)?.as_ref()
    }

    /// Startup lookup for a registered entity implementation. Save-local
    /// numeric assignments may differ; canonical keys are the stable contract.
    pub fn entity_type_id_by_key(&self, key: &str) -> Option<EntityTypeId> {
        self.entities
            .iter()
            .flatten()
            .find(|definition| definition.key == key)
            .map(|definition| definition.id)
    }

    #[inline]
    pub fn texture(&self, id: TextureId) -> Option<&TextureDef> {
        self.textures.get(id.0 as usize)
    }

    pub fn textures(&self) -> &[TextureDef] {
        &self.textures
    }

    pub fn primary_block_item(&self, block: BlockStateId) -> Option<ItemId> {
        let type_id = self.state(block)?.block_type;
        self.primary_block_items
            .get(type_id.0 as usize)
            .copied()
            .flatten()
    }

    /// Immutable player contract shared by every world and prediction consumer.
    pub fn player_rules(&self) -> bloxgloom_host_api::player::PlayerRules {
        self.player_rules
    }

    /// Assigned IDs and schema/behavior/material definitions used by a connection.
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut textures = self
            .textures
            .iter()
            .zip(&self.texture_fingerprints)
            .collect::<Vec<_>>();
        textures.sort_unstable_by_key(|(texture, _)| texture.key.as_ref());
        for (texture, fingerprint) in textures {
            hash_bytes(&mut hash, texture.key.as_bytes());
            hash_bytes(&mut hash, &fingerprint.to_le_bytes());
        }
        let mut identities = self.identities();
        identities.sort_unstable_by_key(|(kind, id, _, _)| (*kind, *id));
        for (kind, id, key, fingerprint) in identities {
            hash_bytes(&mut hash, &[kind]);
            hash_bytes(&mut hash, &id.to_le_bytes());
            hash_bytes(&mut hash, key.as_bytes());
            hash_bytes(&mut hash, &fingerprint.to_le_bytes());
        }
        let mut acoustics = self.block_acoustics.iter().collect::<Vec<_>>();
        acoustics.sort_unstable_by_key(|(key, _)| *key);
        for (key, acoustics) in acoustics {
            hash_bytes(&mut hash, key.as_bytes());
            hash_bytes(&mut hash, &acoustics.bytes());
        }
        hash
    }

    /// Stable save identities, including schema and compiled behavior in each fingerprint.
    pub fn identities(&self) -> Vec<(u8, u32, &str, u64)> {
        let mut entries = self.composition.identities();
        entries.extend(
            self.client_metadata
                .identities
                .iter()
                .map(|((kind, id), (key, hash))| (*kind, *id, key.as_str(), *hash)),
        );
        for (id, handler) in &self.gameplay_handlers {
            entries.push((
                b'G',
                *id,
                handler.key.as_str(),
                self.definition_fingerprint(b'G', *id),
            ));
        }
        for (id, observer) in &self.gameplay_observers {
            entries.push((
                b'O',
                *id,
                observer.key.as_str(),
                self.definition_fingerprint(b'O', *id),
            ));
        }
        for (id, lifecycle) in &self.player_lifecycles {
            entries.push((
                b'Q',
                *id,
                lifecycle.key.as_str(),
                self.definition_fingerprint(b'Q', *id),
            ));
        }
        for (id, system) in &self.owner_systems {
            entries.push((
                b'Y',
                *id,
                system.key.as_str(),
                self.definition_fingerprint(b'Y', *id),
            ));
        }
        for (id, definition) in self.blocks.iter().enumerate() {
            if let Some(block) = definition {
                entries.push((
                    b'B',
                    id as u32,
                    block.key.as_ref(),
                    self.definition_fingerprint(b'B', id as u32),
                ));
            }
        }
        for (id, definition) in self.states.iter().enumerate() {
            if let Some(state) = definition {
                debug_assert_eq!(state.id.0, id as u32);
                entries.push((
                    b'S',
                    state.id.0,
                    state.key.as_str(),
                    self.definition_fingerprint(b'S', state.id.0),
                ));
            }
        }
        for (id, definition) in self.items.iter().enumerate() {
            if let Some(item) = definition {
                entries.push((
                    b'I',
                    id as u32,
                    item.key.as_ref(),
                    self.definition_fingerprint(b'I', id as u32),
                ));
            }
        }
        for (id, definition) in self.entities.iter().enumerate() {
            if let Some(entity) = definition {
                entries.push((
                    b'E',
                    id as u32,
                    entity.key.as_ref(),
                    self.definition_fingerprint(b'E', id as u32),
                ));
            }
        }
        entries
    }

    fn definition_fingerprint(&self, kind: u8, id: u32) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut add = |bytes: &[u8]| hash_bytes(&mut hash, bytes);
        match kind {
            b'G' => add(&self.gameplay_handlers[&id].fingerprint_bytes()),
            b'O' => add(&self.gameplay_observers[&id].version.to_le_bytes()),
            b'Y' => add(&self.owner_systems[&id].fingerprint_bytes()),
            b'Q' => add(&self.player_lifecycles[&id].fingerprint_bytes()),
            b'B' => {
                let block = self.block_type(BlockTypeId(id)).unwrap();
                add(block.key.as_bytes());
                add(block.name.as_bytes());
                for value in block.swatch {
                    add(&value.to_le_bytes());
                }
                add(&[flags(block), block.emission]);
                add(&block.reflectance);
                for texture in [
                    block.textures.top,
                    block.textures.side,
                    block.textures.bottom,
                ] {
                    add(&self.texture_fingerprints[texture.0 as usize].to_le_bytes());
                }
                for property in &block.properties {
                    add(property.name.as_bytes());
                    add(&[0]);
                    for value in &property.values {
                        add(value.as_bytes());
                        add(&[0]);
                    }
                }
                if self.narrow_plants.contains(block.key.as_ref()) {
                    add(b"narrow-plant-selection");
                }
            }
            b'S' => {
                let state = self.state(BlockStateId(id)).unwrap();
                add(state.key.as_bytes());
                add(self.block_type(state.block_type).unwrap().key.as_bytes());
                add(&[state.flags, state.emission]);
                add(&state.reflectance);
                for texture in [
                    state.textures.top,
                    state.textures.side,
                    state.textures.bottom,
                ] {
                    add(&self.texture_fingerprints[texture.0 as usize].to_le_bytes());
                }
            }
            b'I' => {
                let item = self.item(ItemId(id)).unwrap();
                add(item.key.as_bytes());
                add(item.name.as_bytes());
                for value in item.swatch {
                    add(&value.to_le_bytes());
                }
                add(&self.texture_fingerprints[item.texture.0 as usize].to_le_bytes());
                if let Some(state) = item.placeable {
                    add(self.state(state).unwrap().key.as_bytes());
                }
                add(&[item.sprite as u8]);
                let drop_size = self.drop_size(item.id);
                if drop_size != bloxgloom_host_api::content::DropSize::Normal {
                    add(b"drop-size/v1");
                    add(&[match drop_size {
                        bloxgloom_host_api::content::DropSize::Small => 1,
                        bloxgloom_host_api::content::DropSize::Normal => unreachable!(),
                        bloxgloom_host_api::content::DropSize::Large => 2,
                    }]);
                }
                let animation = self.drop_animation(item.id);
                let policy = self.drop_policy(item.id);
                if policy != bloxgloom_host_api::content::DropPolicy::default() {
                    add(b"drop-policy/v1");
                    add(&policy.to_bytes());
                }
                if animation != bloxgloom_host_api::content::DropAnimation::default() {
                    add(b"drop-animation/v1");
                    add(&animation.to_bytes());
                }
                if let Some(icon) = self.item_icon(item.id) {
                    add(&[1, icon.rows.len() as u8]);
                    for row in &icon.rows {
                        add(&[row.len() as u8]);
                        add(row.as_bytes());
                    }
                    add(&[icon.palette.len() as u8]);
                    for (symbol, color) in &icon.palette {
                        add(&[*symbol]);
                        for value in color {
                            add(&value.to_bits().to_le_bytes());
                        }
                    }
                }
                self.component_fingerprint(item.key.as_ref(), &mut hash);
            }
            b'E' => {
                let entity = self.entity_type(EntityTypeId(id)).unwrap();
                add(entity.key.as_bytes());
                add(&entity.schema_version.to_le_bytes());
                add(&entity.schema_fingerprint.to_le_bytes());
                if let Some(generic) = self.gameplay_entities.get(entity.key.as_ref()) {
                    add(&generic.max_state_bytes.to_le_bytes());
                    add(&generic.initial_delay_ticks.unwrap_or(0).to_le_bytes());
                }
                if let Some(generic) = self.client_metadata.entities.get(entity.key.as_ref()) {
                    add(&generic.max_state_bytes.to_le_bytes());
                    add(&generic.initial_delay_ticks.unwrap_or(0).to_le_bytes());
                }
                if let Some(machine) = self.machine(entity.id) {
                    add(&machine.fingerprint_bytes());
                }
                if let Some(moving) = self.moving_entity(entity.id) {
                    add(&moving.fingerprint_bytes());
                }
                if let Some(mobile) = self.mobile_entity(entity.id) {
                    add(&mobile.fingerprint_bytes());
                }
                if let Some(screen) = self.inventory_screen(entity.id) {
                    add(&screen.fingerprint_bytes());
                }
                if let Some(storage) = self.storage_lifecycles.iter().find(|storage| {
                    storage.entity == entity.key.as_ref()
                        && storage.allowed_automation_faces().len()
                            != bloxgloom_host_api::machine::FACES.len()
                }) {
                    add(b"storage-automation-faces");
                    let mut faces = storage.allowed_automation_faces().to_vec();
                    faces.sort_unstable();
                    for face in faces {
                        for coordinate in face {
                            add(&coordinate.to_le_bytes());
                        }
                    }
                }
                // Action keys have no numeric save identity. The player contract
                // fingerprints the complete canonical registry at handshake/load.
                if entity.key == "bloxgloom:player" {
                    add(crate::appearance::fingerprint());
                    if let Some(appearance) = &self.player_appearance {
                        add(&appearance.fingerprint_bytes());
                    }
                    if let Some(selection) = &self.player_selection {
                        add(&selection.fingerprint_bytes());
                    }
                    for action in self.actions.values() {
                        add(&action.fingerprint_bytes());
                    }
                }
            }
            _ => unreachable!(),
        }
        hash
    }
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn fingerprint_texture(texture: &TextureDef) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    hash_bytes(&mut hash, texture.key.as_bytes());
    hash_bytes(&mut hash, &texture.emission_strength.to_le_bytes());
    hash_bytes(
        &mut hash,
        &[
            texture.stitch_edges as u8,
            texture.stitch_vertical as u8,
            texture.alpha_cutout as u8,
        ],
    );
    hash_bytes(&mut hash, &texture.png);
    hash
}

fn checked_id(id: u32) -> Result<usize, RegistrationError> {
    if id >= MAX_ASSIGNED_ID {
        return Err(RegistrationError::TooManyDefinitions);
    }
    Ok(id as usize)
}

fn valid_properties(properties: &[PropertyDef]) -> bool {
    let mut last = "";
    let mut state_count = 1usize;
    for property in properties {
        let name = property.name.as_ref();
        if !valid_property_token(name) || name <= last || property.values.is_empty() {
            return false;
        }
        last = name;
        state_count = state_count.saturating_mul(property.values.len());
        if state_count > 4_096 {
            return false;
        }
        let mut values = property
            .values
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        if values.iter().any(|value| !valid_property_token(value)) {
            return false;
        }
        values.sort_unstable();
        if values.windows(2).any(|pair| pair[0] == pair[1]) {
            return false;
        }
    }
    true
}

fn valid_property_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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
    if definition.flammable {
        bits |= FLAMMABLE;
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
pub fn block_flags(id: BlockId) -> u8 {
    if id <= world::MAX_BUILTIN_BLOCK {
        BUILTIN_FLAGS[id.0 as usize]
    } else {
        catalog().block_flags(id)
    }
}

#[cfg(test)]
#[path = "content/tests.rs"]
mod tests;
