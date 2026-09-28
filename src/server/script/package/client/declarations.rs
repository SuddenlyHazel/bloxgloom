//! A closed, data-only registration host. Never loads source, image payloads,
//! callbacks, paths or a VM. Public package requirements are identity metadata,
//! not permissions granted to the client. Runtime declarations contribute inert
//! compatibility identities, never client execution authority.
use super::*;
use bloxgloom_host_api::{composition, content};
use content::DropSize;
mod runtime;

const MAX_ITEMS: usize = 32;
const MAX_TEXTURES: usize = 32;
const MAX_BLOCKS: usize = 32;

#[derive(Debug)]
pub(super) struct Startup {
    player_rules: Option<crate::content::player::Selection>,
    packages: Vec<composition::Package>,
    items: Vec<content::Item>,
    textures: Vec<content::Texture>,
    blocks: Vec<content::Block>,
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
        let textures = &declarations.textures;
        let blocks = &declarations.blocks;
        if self.declarations.is_some()
            || packages.len() != self.packages.len()
            || items.len() > MAX_PACKAGES * MAX_ITEMS
            || textures.len() > MAX_PACKAGES * MAX_TEXTURES
            || blocks.len() > MAX_PACKAGES * MAX_BLOCKS
        {
            return Err(invalid());
        }
        let mut writer = Writer(self.bytes[..self.bytes.len() - 4].to_vec());
        let sized = items.iter().any(|item| item.drop_size != DropSize::Normal);
        if sized {
            writer.0[MAGIC.len() - 1] = SIZED_MAGIC[MAGIC.len() - 1];
        }
        if declarations.player_rules.is_some() {
            writer.0[MAGIC.len() - 1] = if sized {
                PLAYER_SIZED_MAGIC
            } else {
                PLAYER_MAGIC
            }[MAGIC.len() - 1];
        }
        writer.count(1)?;
        let runtime = runtime::Runtime::project(declarations)?;
        let mut items = items.iter().collect::<Vec<_>>();
        items.sort_by(|a, b| a.key.cmp(&b.key));
        let mut textures = textures.iter().collect::<Vec<_>>();
        textures.sort_by(|a, b| a.definition.key.cmp(&b.definition.key));
        let mut blocks = blocks.iter().collect::<Vec<_>>();
        blocks.sort_by(|a, b| a.key.cmp(&b.key));
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
                    || item.components != content::Components::None
                    || match cube {
                        Some(block) => {
                            item.placeable.as_deref() != Some(&block.key)
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
                if sized {
                    writer.field(&[match item.drop_size {
                        DropSize::Normal => 0,
                        DropSize::Small => 1,
                        DropSize::Large => 2,
                    }])?;
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
                    || texture.definition.alpha_cutout
                    || texture.definition.emission_strength != 0.0
                {
                    return Err(error(
                        name,
                        "startup texture does not match the exact declared asset or supported material flags",
                    ));
                }
                writer.field(texture.definition.key.as_bytes())?;
                writer.field(texture.asset.as_bytes())?;
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
            }
            runtime.encode_package(&mut writer, name)?;
        }
        if let Some(selection) = &declarations.player_rules {
            selection.validate().map_err(|_| invalid())?;
            writer.field(selection.key.as_bytes())?;
            writer.field(&selection.revision.to_le_bytes())?;
            writer.field(&selection.rules.canonical_bytes())?;
        }
        let key = CacheKey(Sha256::digest(&writer.0).into());
        let result = Self::decode_verify(&writer.0, key)?;
        let decoded = result.declarations.as_ref().unwrap();
        if decoded.items.len() != items.len()
            || decoded.textures.len() != textures.len()
            || decoded.blocks.len() != blocks.len()
            || decoded.runtime.counts() != runtime.counts()
            || decoded.player_rules != declarations.player_rules
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
                        for item in &startup.items {
                            declarations.item(item.clone())?;
                        }
                        let mut catalog = crate::content::Catalog::builtins();
                        declarations.install_base(&mut catalog)?;
                        declarations.install_items_and_tags(&mut catalog)?;
                        catalog.refresh_builtin_fuels()?;
                        startup.runtime.install(&mut catalog)?;
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
        sized: bool,
        player: bool,
    ) -> Result<Option<Self>, ScriptError> {
        if reader.count(1)? == 0 {
            if sized || player {
                return Err(invalid());
            }
            return Ok(None);
        }
        let mut has_nondefault_size = false;
        let mut startup = Self {
            player_rules: None,
            packages: Vec::new(),
            items: Vec::new(),
            textures: Vec::new(),
            blocks: Vec::new(),
            runtime: runtime::Runtime::default(),
        };
        for (name, package) in packages {
            if name == "bloxgloom" {
                return Err(invalid());
            }
            let mut requires = Vec::new();
            for _ in 0..reader.count(4)? {
                let requirement = reader.text(64)?;
                if ![
                    composition::CONTENT,
                    composition::GENERATION,
                    composition::ACTIONS,
                    composition::OWNER_SYSTEMS,
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
                    components: content::Components::None,
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
                previous.clone_from(&key);
                startup.textures.push(content::Texture {
                    key,
                    png: std::borrow::Cow::Owned(png.clone()),
                    stitch_edges: true,
                    stitch_vertical: true,
                    alpha_cutout: false,
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
                {
                    return Err(error(name, format!("block {key} item does not match")));
                }
                item.placeable = Some(key.clone());
                item.sprite = false;
                previous.clone_from(&key);
                let mut block =
                    crate::server::script::startup::cube(key, display, texture, mlua::Value::Nil)
                        .map_err(|reason| error(name, reason))?;
                block.flammable = flags & 1 != 0;
                block.supports_plant = flags & 2 != 0;
                startup.blocks.push(block);
            }
            startup.runtime.decode_package(reader, name, &requires)?;
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
        if sized && !has_nondefault_size {
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
