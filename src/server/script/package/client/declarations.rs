//! A closed, data-only registration host. Never loads source, image payloads,
//! callbacks, paths or a VM. Public package requirements are identity metadata,
//! not permissions granted to the client. Runtime declarations contribute inert
//! compatibility identities, never client execution authority.
use super::*;
use bloxgloom_host_api::{composition, content};
use content::{DropAnimation, DropPolicy, DropSize, Geometry, Material, TagKind, TagMember};
mod appearance;
mod components;
mod runtime;
mod states;
mod storage;

const MAX_ITEMS: usize = 32;
const MAX_TEXTURES: usize = 32;
const MAX_BLOCKS: usize = 32;

pub(super) struct Format {
    pub sized: bool,
    pub animated: bool,
    pub player: bool,
    pub appearance: bool,
    pub policy: bool,
    pub extended_blocks: bool,
    pub tags: bool,
    pub visual_blocks: bool,
    pub block_states: bool,
    pub components: bool,
    pub state_textures: bool,
    pub storage: bool,
    pub screen_layout: bool,
}

#[derive(Debug)]
pub(super) struct Startup {
    appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    player_rules: Option<crate::content::player::Selection>,
    packages: Vec<composition::Package>,
    items: Vec<content::Item>,
    tags: Vec<content::Tag>,
    textures: Vec<content::Texture>,
    blocks: Vec<content::Block>,
    storage: Vec<crate::server::script::startup::StorageDeclaration>,
    runtime: runtime::Runtime,
}

impl ClientBundle {
    /// Called only after every authoritative startup entry succeeds. Discovery's
    /// final absent-metadata marker is replaced; source/asset bytes stay intact.
    pub(in crate::server::script) fn with_startup(
        &self,
        declarations: &crate::server::script::startup::Declarations,
    ) -> Result<Self, ScriptError> {
        let packages = &declarations.packages;
        let items = &declarations.items;
        let tags = &declarations.tags;
        let textures = &declarations.textures;
        let blocks = &declarations.blocks;
        let storage = &declarations.storage;
        if self.declarations.is_some()
            || packages.len() != self.packages.len()
            || items.len() > MAX_PACKAGES * MAX_ITEMS
            || tags.len() > MAX_PACKAGES * 32
            || textures.len() > MAX_PACKAGES * MAX_TEXTURES
            || blocks.len() > MAX_PACKAGES * MAX_BLOCKS
            || storage.len() > MAX_PACKAGES * 8
        {
            return Err(invalid());
        }
        let mut writer = Writer(self.bytes[..self.bytes.len() - 4].to_vec());
        let has_appearance = declarations.appearance.is_some();
        let sized = has_appearance || items.iter().any(|item| item.drop_size != DropSize::Normal);
        let animated = has_appearance
            || items
                .iter()
                .any(|item| item.drop_animation != DropAnimation::default());
        let policy = items
            .iter()
            .any(|item| item.drop_policy != DropPolicy::default());
        let authored_blocks = blocks
            .iter()
            .any(crate::server::script::startup::extended_block);
        let tag_format = !tags.is_empty();
        let visual_format = blocks
            .iter()
            .any(crate::server::script::startup::visual_block)
            || textures
                .iter()
                .any(|texture| texture.definition.alpha_cutout);
        let storage_format = !storage.is_empty();
        let state_texture_format = blocks
            .iter()
            .any(|block| block.states.iter().any(|state| state.textures.is_some()));
        let item_components = items
            .iter()
            .any(|item| item.components != content::Components::None);
        let component_format = item_components || state_texture_format || storage_format;
        let authored_states = blocks
            .iter()
            .any(crate::server::script::startup::stateful_block);
        let states_format = authored_states || component_format;
        let extended_blocks = authored_blocks || tag_format || visual_format || states_format;
        let visual_format = visual_format || states_format;
        let screen_layout = storage.iter().any(|declaration| {
            !declaration.screen.hint.is_empty()
                || declaration.screen.groups.len() != 1
                || declaration.screen.groups[0].label != "STORAGE"
        });
        let version = if screen_layout {
            SCREEN_LAYOUT_MAGIC
        } else if storage_format {
            STORAGE_MAGIC
        } else if state_texture_format {
            STATE_TEXTURES_MAGIC
        } else if component_format {
            COMPONENTS_MAGIC
        } else if states_format {
            BLOCK_STATES_MAGIC
        } else if visual_format {
            VISUAL_BLOCKS_MAGIC
        } else if tag_format {
            TAGS_MAGIC
        } else if extended_blocks {
            BLOCK_OPTIONS_MAGIC
        } else if has_appearance && policy {
            APPEARANCE_POLICY_MAGIC
        } else if has_appearance {
            APPEARANCE_MAGIC
        } else if policy {
            if declarations.player_rules.is_some() {
                PLAYER_POLICY_MAGIC
            } else {
                POLICY_MAGIC
            }
        } else {
            match (declarations.player_rules.is_some(), animated, sized) {
                (false, false, false) => MAGIC,
                (false, false, true) => SIZED_MAGIC,
                (false, true, _) => ANIMATED_MAGIC,
                (true, false, false) => PLAYER_MAGIC,
                (true, false, true) => PLAYER_SIZED_MAGIC,
                (true, true, _) => PLAYER_ANIMATED_MAGIC,
            }
        };
        writer.0[MAGIC.len() - 1] = version[MAGIC.len() - 1];
        writer.count(1)?;
        let runtime = runtime::Runtime::project(declarations)?;
        let mut items = items.iter().collect::<Vec<_>>();
        items.sort_by(|a, b| a.key.cmp(&b.key));
        let mut textures = textures.iter().collect::<Vec<_>>();
        textures.sort_by(|a, b| a.definition.key.cmp(&b.definition.key));
        let mut blocks = blocks.iter().collect::<Vec<_>>();
        blocks.sort_by(|a, b| a.key.cmp(&b.key));
        let mut tags = tags.iter().collect::<Vec<_>>();
        tags.sort_by(|a, b| (a.key.as_str(), a.kind).cmp(&(b.key.as_str(), b.kind)));
        for ((name, _), package) in self.packages.iter().zip(packages) {
            if package.key != format!("{name}:package") || package.version != 1 {
                return Err(invalid());
            }
            writer.count(package.requires.len())?;
            for requirement in &package.requires {
                writer.field(requirement.as_bytes())?;
            }
            let own = items
                .iter()
                .filter(|item| {
                    item.key
                        .split_once(':')
                        .is_some_and(|(owner, _)| owner == name)
                })
                .collect::<Vec<_>>();
            writer.count(own.len())?;
            for item in own {
                let cube = blocks.iter().find(|block| block.key == item.key);
                if item.swatch != [1.0; 4]
                    || (item.components != content::Components::None && !component_format)
                    || match cube {
                        Some(block) => {
                            item.placeable.as_deref()
                                != Some(
                                    crate::server::script::startup::placement_state(block).as_str(),
                                )
                                || item.sprite
                                || item.texture != block.textures.top
                                || item.name != block.name
                        }
                        None => item.placeable.is_some(),
                    }
                {
                    return Err(invalid());
                }
                writer.field(item.key.as_bytes())?;
                writer.field(item.name.as_bytes())?;
                // The existing three-argument record remains byte-for-byte
                // unchanged. A non-placeable cube is distinguished by a
                // reserved, non-key prefix on its texture field; the decoder
                // strips it before resolving the actual texture key.
                if cube.is_none() && !item.sprite {
                    if item.texture.len() >= 255 {
                        return Err(invalid());
                    }
                    writer.field(format!("!{}", item.texture).as_bytes())?;
                } else {
                    writer.field(item.texture.as_bytes())?;
                }
                if sized || animated || policy || extended_blocks {
                    writer.field(&[match item.drop_size {
                        DropSize::Normal => 0,
                        DropSize::Small => 1,
                        DropSize::Large => 2,
                    }])?;
                }
                if animated || policy || extended_blocks {
                    if !item.drop_animation.valid() {
                        return Err(invalid());
                    }
                    writer.field(&item.drop_animation.to_bytes())?;
                }
                if policy || extended_blocks {
                    if !item.drop_policy.valid() {
                        return Err(invalid());
                    }
                    writer.field(&item.drop_policy.to_bytes())?;
                }
                if component_format {
                    components::encode(&mut writer, &item.components)?;
                }
            }
            let own = textures
                .iter()
                .filter(|texture| {
                    texture
                        .definition
                        .key
                        .split_once(':')
                        .is_some_and(|(owner, _)| owner == name)
                })
                .collect::<Vec<_>>();
            writer.count(own.len())?;
            for texture in own {
                let bytes = self.packages[name]
                    .textures
                    .get(&texture.asset)
                    .ok_or_else(|| {
                        error(
                            name,
                            format!("missing declared texture asset {}", texture.asset),
                        )
                    })?;
                if texture.definition.png.as_ref() != bytes.as_slice()
                    || !texture.definition.stitch_edges
                    || !texture.definition.stitch_vertical
                    || (texture.definition.alpha_cutout && !visual_format)
                    || texture.definition.emission_strength != 0.0
                {
                    return Err(error(
                        name,
                        "startup texture does not match the exact declared asset or supported material flags",
                    ));
                }
                writer.field(texture.definition.key.as_bytes())?;
                writer.field(texture.asset.as_bytes())?;
                if visual_format {
                    writer.field(&[u8::from(texture.definition.alpha_cutout)])?;
                }
            }
            let own = blocks
                .iter()
                .filter(|block| {
                    block
                        .key
                        .split_once(':')
                        .is_some_and(|(owner, _)| owner == name)
                })
                .collect::<Vec<_>>();
            writer.count(own.len())?;
            for block in own {
                writer.field(block.key.as_bytes())?;
                writer.field(block.name.as_bytes())?;
                writer.field(block.textures.top.as_bytes())?;
                writer
                    .field(&[u8::from(block.flammable) | (u8::from(block.supports_plant) << 1)])?;
                if extended_blocks {
                    writer.field(block.textures.side.as_bytes())?;
                    writer.field(block.textures.bottom.as_bytes())?;
                    writer.field(&[
                        u8::from(block.solid) | (u8::from(block.replaceable) << 1),
                        block.emission,
                        block.reflectance[0],
                        block.reflectance[1],
                        block.reflectance[2],
                    ])?;
                }
                if visual_format {
                    writer.field(&[match block.geometry {
                        Geometry::Cube => 0,
                        Geometry::CrossedPlant => 1,
                        Geometry::NarrowCrossedPlant => 2,
                    } | match block.material {
                        Material::Opaque => 0,
                        Material::Cutout => 4,
                        Material::Invisible => return Err(invalid()),
                    }])?;
                }
                if states_format {
                    states::encode(&mut writer, block, state_texture_format || storage_format)?;
                }
            }
            if tag_format || visual_format {
                let own = tags
                    .iter()
                    .filter(|tag| {
                        tag.key
                            .split_once(':')
                            .is_some_and(|(owner, _)| owner == name)
                    })
                    .collect::<Vec<_>>();
                writer.count(own.len())?;
                for tag in own {
                    if tag.members.is_empty() || tag.members.len() > 32 {
                        return Err(invalid());
                    }
                    writer.field(tag.key.as_bytes())?;
                    writer.field(&[match tag.kind {
                        TagKind::Block => 0,
                        TagKind::Item => 1,
                    }])?;
                    writer.count(tag.members.len())?;
                    for member in &tag.members {
                        let (kind, key) = crate::server::script::startup::tag_member_key(member);
                        writer.field(&[kind])?;
                        writer.field(key.as_bytes())?;
                    }
                }
            }
            runtime.encode_package(&mut writer, name)?;
            if storage_format {
                storage::encode(&mut writer, name, storage, screen_layout)?;
            }
        }
        if has_appearance || extended_blocks {
            writer.count(usize::from(declarations.player_rules.is_some()))?;
        }
        if let Some(selection) = &declarations.player_rules {
            selection.validate().map_err(|_| invalid())?;
            writer.field(selection.key.as_bytes())?;
            writer.field(&selection.revision.to_le_bytes())?;
            writer.field(&selection.rules.canonical_bytes())?;
        }
        if extended_blocks {
            writer.count(usize::from(has_appearance))?;
        }
        if let Some(appearance) = &declarations.appearance {
            appearance::encode(&mut writer, appearance)?;
        }
        let key = CacheKey(Sha256::digest(&writer.0).into());
        let result = Self::decode_verify(&writer.0, key)?;
        let decoded = result.declarations.as_ref().unwrap();
        if decoded.items.len() != items.len()
            || decoded.textures.len() != textures.len()
            || decoded.blocks.len() != blocks.len()
            || decoded.tags.len() != tags.len()
            || decoded.storage.len() != storage.len()
            || decoded.runtime.counts() != runtime.counts()
            || decoded.player_rules != declarations.player_rules
            || decoded.appearance != declarations.appearance
        {
            return Err(invalid());
        }
        Ok(result)
    }

    /// Fresh session definitions, never installed in the process-global catalog.
    /// The caller must still resolve the exact server manifest and fingerprint.
    pub(crate) fn session_catalog(&self) -> std::io::Result<crate::content::Catalog> {
        let startup = self.declarations.as_ref().ok_or_else(|| {
            std::io::Error::other("client bundle has no startup registration metadata")
        })?;
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let mut declarations = crate::content::declarations::Declarations::default();
                    let mut compile = || -> Result<_, bloxgloom_host_api::RegistrationError> {
                        for package in &startup.packages {
                            declarations.package(package.clone())?;
                        }
                        for texture in &startup.textures {
                            declarations.texture(texture.clone())?;
                        }
                        for block in &startup.blocks {
                            declarations.block(block.clone())?;
                        }
                        for tag in &startup.tags {
                            declarations.tag(tag.clone())?;
                        }
                        for item in &startup.items {
                            declarations.item(item.clone())?;
                        }
                        let mut catalog = crate::content::Catalog::builtins();
                        declarations.install_base(&mut catalog)?;
                        declarations.install_items_and_tags(&mut catalog)?;
                        catalog.refresh_builtin_fuels()?;
                        startup.runtime.install(&mut catalog)?;
                        for declaration in &startup.storage {
                            catalog.extension_storage(&declaration.storage)?;
                            catalog.register_inventory_screen(declaration.screen.clone())?;
                        }
                        if let Some(appearance) = &startup.appearance {
                            catalog
                                .register_player_appearance(appearance.clone())
                                .map_err(|error| {
                                    bloxgloom_host_api::RegistrationError(format!(
                                        "invalid appearance: {error:?}"
                                    ))
                                })?;
                        }
                        if let Some(selection) = &startup.player_rules {
                            catalog
                                .select_player_rules(selection.clone())
                                .map_err(|error| {
                                    bloxgloom_host_api::RegistrationError(format!(
                                        "invalid player rules: {error:?}"
                                    ))
                                })?;
                        }
                        catalog.validate().map_err(|error| {
                            bloxgloom_host_api::RegistrationError(format!(
                                "invalid client catalog: {error:?}"
                            ))
                        })?;
                        Ok(catalog)
                    };
                    compile().map_err(|error| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, error)
                    })
                })
                .join()
                .map_err(|_| std::io::Error::other("client catalog preparation worker panicked"))?
        })
    }
}

impl Startup {
    pub(super) fn decode(
        reader: &mut Reader<'_>,
        packages: &BTreeMap<String, ClientPackage>,
        Format {
            sized,
            animated,
            player,
            appearance,
            policy,
            extended_blocks,
            tags,
            visual_blocks,
            block_states,
            components,
            state_textures,
            storage,
            screen_layout,
        }: Format,
    ) -> Result<Option<Self>, ScriptError> {
        if reader.count(1)? == 0 {
            if sized
                || animated
                || player
                || appearance
                || policy
                || extended_blocks
                || tags
                || visual_blocks
                || block_states
                || components
                || state_textures
                || storage
                || screen_layout
            {
                return Err(invalid());
            }
            return Ok(None);
        }
        let mut has_nondefault_size = false;
        let mut has_nondefault_animation = false;
        let mut has_nondefault_policy = false;
        let mut has_extended_block = false;
        let mut has_tag = false;
        let mut has_visual = false;
        let mut has_states = false;
        let mut has_components = false;
        let mut has_state_textures = false;
        let mut has_storage = false;
        let mut has_screen_layout = false;
        let mut startup = Self {
            appearance: None,
            player_rules: None,
            packages: Vec::new(),
            items: Vec::new(),
            tags: Vec::new(),
            textures: Vec::new(),
            blocks: Vec::new(),
            storage: Vec::new(),
            runtime: runtime::Runtime::default(),
        };
        for (name, package) in packages {
            if name == "bloxgloom" {
                return Err(invalid());
            }
            let mut requires = Vec::new();
            for _ in 0..reader.count(if storage { 6 } else { 4 })? {
                let requirement = reader.text(64)?;
                if ![
                    composition::CONTENT,
                    composition::GENERATION,
                    composition::ACTIONS,
                    composition::OWNER_SYSTEMS,
                    composition::STORAGE,
                    composition::INVENTORY_SCREENS,
                ]
                .contains(&requirement.as_str())
                    || requires.last().is_some_and(|last| last >= &requirement)
                {
                    return Err(invalid());
                }
                requires.push(requirement);
            }
            let count = reader.count(MAX_ITEMS)?;
            if count > 0 && !requires.iter().any(|r| r == composition::CONTENT) {
                return Err(invalid());
            }
            let mut previous = String::new();
            for _ in 0..count {
                let key = reader.text(129)?;
                let display = reader.text(255)?;
                let encoded_texture = reader.text(255)?;
                let drop_size = if sized {
                    match reader.field(1)? {
                        [0] => DropSize::Normal,
                        [1] => DropSize::Small,
                        [2] => DropSize::Large,
                        _ => return Err(invalid()),
                    }
                } else {
                    DropSize::Normal
                };
                has_nondefault_size |= drop_size != DropSize::Normal;
                let drop_animation = if animated {
                    let bytes: [u8; DropAnimation::BYTE_LEN] = reader
                        .field(DropAnimation::BYTE_LEN)?
                        .try_into()
                        .map_err(|_| invalid())?;
                    DropAnimation::from_bytes(bytes).ok_or_else(invalid)?
                } else {
                    DropAnimation::default()
                };
                has_nondefault_animation |= drop_animation != DropAnimation::default();
                let drop_policy = if policy {
                    DropPolicy::from_bytes(
                        reader
                            .field(DropPolicy::BYTE_LEN)?
                            .try_into()
                            .map_err(|_| invalid())?,
                    )
                    .ok_or_else(invalid)?
                } else {
                    DropPolicy::default()
                };
                has_nondefault_policy |= drop_policy != DropPolicy::default();
                let item_components = if components {
                    let value = components::decode(reader)?;
                    has_components |= value != content::Components::None;
                    value
                } else {
                    content::Components::None
                };
                let (sprite, texture) = match encoded_texture.strip_prefix('!') {
                    Some(texture) => (false, texture.to_owned()),
                    None => (true, encoded_texture),
                };
                if key <= previous
                    || key
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                    || display.is_empty()
                    || texture.split_once(':').is_none_or(|(owner, local)| {
                        (owner != "bloxgloom" && owner != name) || !identifier(local)
                    })
                {
                    return Err(invalid());
                }
                previous.clone_from(&key);
                startup.items.push(content::Item {
                    key,
                    name: display,
                    texture,
                    swatch: [1.0; 4],
                    placeable: None,
                    sprite,
                    drop_size,
                    drop_animation,
                    drop_policy,
                    components: item_components,
                });
            }
            let count = reader.count(MAX_TEXTURES)?;
            if count > 0 && !requires.iter().any(|r| r == composition::CONTENT) {
                return Err(error(name, "startup textures require content capability"));
            }
            let mut previous = String::new();
            for _ in 0..count {
                let key = reader.text(129)?;
                let asset = reader.identifier()?;
                if key <= previous
                    || key
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                {
                    return Err(error(
                        name,
                        format!("invalid or foreign startup texture {key}"),
                    ));
                }
                let png = package.textures.get(&asset).ok_or_else(|| {
                    error(name, format!("missing declared texture asset {asset}"))
                })?;
                let alpha_cutout = if visual_blocks {
                    match reader.field(1)? {
                        [0] => false,
                        [1] => true,
                        _ => return Err(invalid()),
                    }
                } else {
                    false
                };
                has_visual |= alpha_cutout;
                previous.clone_from(&key);
                startup.textures.push(content::Texture {
                    key,
                    png: std::borrow::Cow::Owned(png.clone()),
                    stitch_edges: true,
                    stitch_vertical: true,
                    alpha_cutout,
                    emission_strength: 0.0,
                });
            }
            for item in startup
                .items
                .iter()
                .filter(|item| item.key.starts_with(&format!("{name}:")))
            {
                if item.texture.starts_with(&format!("{name}:"))
                    && !startup
                        .textures
                        .iter()
                        .any(|texture| texture.key == item.texture)
                {
                    return Err(error(
                        name,
                        format!(
                            "item {} references an unregistered package texture",
                            item.key
                        ),
                    ));
                }
            }
            let count = reader.count(MAX_BLOCKS)?;
            if count > 0 && !requires.iter().any(|r| r == composition::CONTENT) {
                return Err(error(name, "startup blocks require content capability"));
            }
            let mut previous = String::new();
            for _ in 0..count {
                let key = reader.text(129)?;
                let display = reader.text(255)?;
                let texture = reader.text(129)?;
                let flags = reader.field(1)?;
                let [flags] = flags else {
                    return Err(error(name, "invalid startup block flags"));
                };
                if flags & !3 != 0 {
                    return Err(error(name, "invalid startup block flags"));
                }
                let extras = if extended_blocks {
                    let side = reader.text(129)?;
                    let bottom = reader.text(129)?;
                    let bytes = reader.field(5)?;
                    let [options, emission, r, g, b] = bytes else {
                        return Err(invalid());
                    };
                    if options & !3 != 0 || *emission > 15 {
                        return Err(invalid());
                    }
                    Some((side, bottom, *options, *emission, [*r, *g, *b]))
                } else {
                    None
                };
                let visual = if visual_blocks {
                    let [flags] = reader.field(1)? else {
                        return Err(invalid());
                    };
                    let geometry = match flags & 3 {
                        0 => Geometry::Cube,
                        1 => Geometry::CrossedPlant,
                        2 => Geometry::NarrowCrossedPlant,
                        _ => return Err(invalid()),
                    };
                    if flags & !7 != 0 {
                        return Err(invalid());
                    }
                    let material = if flags & 4 != 0 {
                        Material::Cutout
                    } else {
                        Material::Opaque
                    };
                    has_visual |= geometry != Geometry::Cube || material != Material::Opaque;
                    Some((geometry, material))
                } else {
                    None
                };
                if key <= previous
                    || key
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                    || texture
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                    || display.is_empty()
                    || !startup.textures.iter().any(|t| t.key == texture)
                {
                    return Err(error(name, format!("invalid startup block {key}")));
                }
                let item = startup
                    .items
                    .iter_mut()
                    .find(|i| i.key == key)
                    .ok_or_else(|| error(name, format!("block {key} has no placeable item")))?;
                if item.name != display
                    || item.texture != texture
                    || !item.sprite
                    || item.drop_size != DropSize::Normal
                    || item.drop_animation != DropAnimation::default()
                    || item.drop_policy != DropPolicy::default()
                {
                    return Err(error(name, format!("block {key} item does not match")));
                }
                item.sprite = false;
                previous.clone_from(&key);
                let mut block =
                    crate::server::script::startup::cube(key, display, texture, mlua::Value::Nil)
                        .map_err(|reason| error(name, reason))?;
                block.flammable = flags & 1 != 0;
                block.supports_plant = flags & 2 != 0;
                if let Some((side, bottom, options, emission, reflectance)) = extras {
                    for face in [&side, &bottom] {
                        if face
                            .split_once(':')
                            .is_none_or(|(owner, local)| owner != name || !identifier(local))
                            || !startup
                                .textures
                                .iter()
                                .any(|registered| registered.key == *face)
                        {
                            return Err(invalid());
                        }
                    }
                    block.textures.side = side;
                    block.textures.bottom = bottom;
                    block.solid = options & 1 != 0;
                    block.replaceable = options & 2 != 0;
                    block.emission = emission;
                    block.reflectance = reflectance;
                    has_extended_block |= crate::server::script::startup::extended_block(&block);
                }
                if let Some((geometry, material)) = visual {
                    block.geometry = geometry;
                    block.material = material;
                    if geometry != Geometry::Cube && (material != Material::Cutout || block.solid) {
                        return Err(invalid());
                    }
                    if material == Material::Cutout
                        && [
                            &block.textures.top,
                            &block.textures.side,
                            &block.textures.bottom,
                        ]
                        .iter()
                        .any(|face| {
                            !startup
                                .textures
                                .iter()
                                .any(|texture| texture.key == face.as_str() && texture.alpha_cutout)
                        })
                    {
                        return Err(invalid());
                    }
                }
                if block_states {
                    states::decode(reader, &mut block, state_textures, &startup.textures, name)?;
                    has_states |= crate::server::script::startup::stateful_block(&block);
                    has_state_textures |= block.states.iter().any(|state| state.textures.is_some());
                }
                item.placeable = Some(crate::server::script::startup::placement_state(&block));
                startup.blocks.push(block);
            }
            if tags {
                let count = reader.count(32)?;
                if count > 0 && !requires.contains(&composition::CONTENT.to_owned()) {
                    return Err(invalid());
                }
                has_tag |= count > 0;
                let mut previous: Option<(String, TagKind)> = None;
                for _ in 0..count {
                    let key = reader.text(129)?;
                    let kind = match reader.field(1)? {
                        [0] => TagKind::Block,
                        [1] => TagKind::Item,
                        _ => return Err(invalid()),
                    };
                    if key
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                        || previous
                            .as_ref()
                            .is_some_and(|last| last >= &(key.clone(), kind))
                    {
                        return Err(invalid());
                    }
                    previous = Some((key.clone(), kind));
                    let mut members = Vec::new();
                    for _ in 0..reader.count(32)? {
                        let member = reader.field(1)?;
                        let target = reader.text(129)?;
                        if target
                            .split_once(':')
                            .is_none_or(|(owner, local)| !identifier(owner) || !identifier(local))
                        {
                            return Err(invalid());
                        }
                        members.push(match member {
                            [0] => TagMember::Definition(target),
                            [1] => TagMember::Tag(target),
                            _ => return Err(invalid()),
                        });
                    }
                    if members.is_empty()
                        || members.windows(2).any(|pair| {
                            crate::server::script::startup::tag_member_key(&pair[0])
                                >= crate::server::script::startup::tag_member_key(&pair[1])
                        })
                    {
                        return Err(invalid());
                    }
                    startup.tags.push(content::Tag { key, kind, members });
                }
            }
            startup.runtime.decode_package(reader, name, &requires)?;
            if storage {
                let decoded =
                    storage::decode(reader, name, &requires, &startup.blocks, screen_layout)?;
                has_storage |= !decoded.is_empty();
                has_screen_layout |= decoded.iter().any(|declaration| {
                    !declaration.screen.hint.is_empty()
                        || declaration.screen.groups.len() != 1
                        || declaration.screen.groups[0].label != "STORAGE"
                });
                startup.storage.extend(decoded);
            }
            startup.packages.push(composition::Package {
                key: format!("{name}:package"),
                version: 1,
                dependencies: package
                    .dependencies
                    .keys()
                    .map(|name| composition::Dependency {
                        package: format!("{name}:package"),
                        version: 1,
                    })
                    .collect(),
                requires,
            });
        }
        let player = if appearance || extended_blocks {
            reader.count(1)? == 1
        } else {
            player
        };
        if player {
            let key = reader.text(129)?;
            let revision = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
            let rules = bloxgloom_host_api::player::PlayerRules::from_canonical_bytes(
                reader.field(40)?.try_into().map_err(|_| invalid())?,
            )
            .map_err(|_| invalid())?;
            let (owner, local) = key.split_once(':').ok_or_else(invalid)?;
            if !identifier(local)
                || !startup.packages.iter().any(|package| {
                    package.key == format!("{owner}:package")
                        && package.requires.iter().any(|r| r == composition::CONTENT)
                })
            {
                return Err(invalid());
            }
            let selection = crate::content::player::Selection {
                key,
                revision,
                rules,
            };
            selection.validate().map_err(|_| invalid())?;
            startup.player_rules = Some(selection);
        }
        let appearance = appearance || extended_blocks && reader.count(1)? == 1;
        if appearance {
            startup.appearance = Some(appearance::decode(reader, &startup.packages)?);
        }
        if !appearance
            && !extended_blocks
            && ((sized && !animated && !has_nondefault_size)
                || (animated && !policy && !has_nondefault_animation))
            || (policy && !extended_blocks && !has_nondefault_policy)
            || (extended_blocks && !has_extended_block && !tags && !visual_blocks)
            || (tags && !has_tag && !visual_blocks)
            || (visual_blocks && !has_visual && !block_states)
            || (block_states && !has_states && !components)
            || (components && !has_components && !state_textures)
            || (state_textures && !has_state_textures && !storage)
            || (storage && !has_storage)
            || (screen_layout && !has_screen_layout)
        {
            return Err(invalid());
        }
        if !startup.textures.is_empty() {
            // PNG decode and dimension/memory checks belong to preparation, not
            // the window thread. Validate even cached/streamed bundles before
            // publishing their decoded startup metadata.
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let mut catalog = crate::content::Catalog::builtins();
                        for texture in &startup.textures {
                            catalog
                                .public_texture(texture)
                                .map_err(|e| error(&texture.key, e))?;
                        }
                        Ok::<(), ScriptError>(())
                    })
                    .join()
                    .map_err(|_| error("<client-bundle>", "texture preparation worker panicked"))?
            })?;
        }
        Ok(Some(startup))
    }
}
