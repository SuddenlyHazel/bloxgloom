//! Startup-only adapter: bounded Luau declarations become a single public
//! extension bundle. No VM or partial catalog survives installation. Generation
//! and semantic action registrations retain immutable sources, not VM callbacks.
//!
//! Opt in via `server-packages <package-root> <address> <save-dir> [max-clients]`.
//! Add `requires bloxgloom:content/v1` to package.txt, then return an entry like:
//! `function(host) host.register_item("demo:token", "Token", "bloxgloom:stone") end`.
//! This host replaces integer inputs only for startup, not execute_package.
//! The optional fourth item argument accepts `drop_policy={gravity=12,
//! terminal_speed=4, radius=0.4, pickup_range=5, merge_range=3, lifetime_ms=2000}`.
//! Missing fields retain stock defaults. These are frozen server gameplay rules,
//! saved with item identity and mirrored as inert client compatibility metadata;
//! they never change the 128 stack cap or authorize client-owned pickup timing.
//!
//! Startup registers namespaced content, actions/decision handlers, entities,
//! owner systems, generators, storage, machines and creatures through public
//! contracts. See SCRIPTING.md for the complete current binding inventory and
//! bounds. Blocks support explicit states, face textures, light/material flags
//! and cube/crossed geometry; the three-argument form keeps cube defaults.
//! Imported helpers retain only the entry package's declaration authority.
//! Public metadata is negotiated; server callbacks, owner seeds and codecs
//! stay server-only. Dependency entries run too and may import declared helpers.
//! Reused worker-owned VMs evaluate isolated, bounded startup attempts.
//!
//! The bundle records identity:package with public contract version 1. Source
//! semver is checked exactly during discovery, not persisted or converted to
//! the public u32 version. Retry/restart rediscovers sources with isolated state;
//! failed declaration collection publishes nothing and never opens the world.
//! Generator execution failures are reported later by chunk loading. Existing catalog
//! identity/reconciliation and client compatibility checks remain authoritative.
use super::{Invocation, Output, Program, ScriptInput, ScriptWorker, package::PackageSnapshot};
use bloxgloom_host_api::{
    Extension, Registrar, RegistrationError,
    content::{Block, Components, Item, Texture},
};
use mlua::{Function, Lua, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use super::capacity::{
    BLOCKS_PER_PACKAGE as MAX_BLOCKS_PER_PACKAGE, ITEMS_PER_PACKAGE as MAX_ITEMS_PER_PACKAGE,
    TEXTURES_PER_PACKAGE as MAX_TEXTURES_PER_PACKAGE,
};
mod acoustics;
mod block;
mod composition;
#[cfg(test)]
mod tests;
pub(in crate::server::script) use block::cube;
pub(in crate::server::script) use block::extended as extended_block;
pub(in crate::server::script) use block::visual as visual_block;
pub(in crate::server::script) use block::{has_state, placement_state, stateful as stateful_block};
mod anchored;
mod appearance;
mod icons;
mod item;
pub(in crate::server::script) mod models;
mod player;
mod player_health;
mod player_world;
mod storage;
pub(in crate::server::script) use storage::Declaration as StorageDeclaration;
mod creature;
mod machine;
mod moving;
pub(in crate::server::script) use machine::Declaration as MachineDeclaration;
mod tag;
pub(in crate::server::script) use tag::member_key as tag_member_key;

pub(super) struct PackageTexture {
    pub(super) definition: Texture,
    pub(super) asset: String,
}

pub(in crate::server) struct Declarations {
    pub(in crate::server) snapshot: Arc<PackageSnapshot>,
    pub(in crate::server) appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    pub(in crate::server) player_rules: Option<crate::content::player::Selection>,
    pub(in crate::server) client_bundle: Arc<super::package::client::ClientBundle>,
    pub(super) packages: Vec<bloxgloom_host_api::composition::Package>,
    pub(super) items: Vec<Item>,
    pub(super) icons: Vec<bloxgloom_host_api::icon::ItemIcon>,
    pub(super) models: Vec<models::PackageModel>,
    pub(super) tags: Vec<bloxgloom_host_api::content::Tag>,
    pub(super) blocks: Vec<Block>,
    pub(super) textures: Vec<PackageTexture>,
    pub(super) generation: Vec<bloxgloom_host_api::generation::Registration>,
    pub(super) actions: Vec<super::gameplay::Registration>,
    pub(super) handlers: Vec<bloxgloom_host_api::gameplay::HandlerRegistration>,
    pub(super) entities: Vec<bloxgloom_host_api::gameplay::EntityDefinition>,
    pub(super) player_lifecycles: Vec<bloxgloom_host_api::players::Registration>,
    pub(super) regions: Vec<bloxgloom_host_api::regions::Registration>,
    pub(super) chat_hooks: Vec<bloxgloom_host_api::chat::Registration>,
    pub(super) damage_policies: Vec<bloxgloom_host_api::player_health::DamageRegistration>,
    pub(super) health_hooks: Vec<bloxgloom_host_api::player_health::HookRegistration>,
    pub(super) observers: Vec<bloxgloom_host_api::gameplay::ObserverRegistration>,
    pub(super) systems: Vec<bloxgloom_host_api::system::System>,
    pub(super) storage: Vec<StorageDeclaration>,
    pub(super) creatures: Vec<bloxgloom_host_api::entity::MobileEntity>,
    pub(super) moving: Vec<bloxgloom_host_api::motion::MovingEntity>,
    pub(super) machines: Vec<MachineDeclaration>,
    pub(super) anchored: Vec<bloxgloom_host_api::anchored::AnchoredBlockEntity>,
}

impl Declarations {
    pub(in crate::server) fn discover(root: &Path) -> std::io::Result<Self> {
        Self::from_snapshot(Arc::new(
            PackageSnapshot::discover(root).map_err(std::io::Error::other)?,
        ))
    }

    pub(in crate::server) fn from_snapshot(
        snapshot: Arc<PackageSnapshot>,
    ) -> std::io::Result<Self> {
        let deadline = std::time::Instant::now() + super::capacity::INSTALLATION_WALL_TIME;
        let packages = snapshot.startup_packages().map_err(std::io::Error::other)?;
        let worker = ScriptWorker::spawn(super::Limits::startup())?;
        let mut items = Vec::new();
        let mut icons = Vec::new();
        let mut models = Vec::new();
        let mut tags = Vec::new();
        let mut blocks = Vec::new();
        let mut textures = Vec::new();
        let mut generation = Vec::new();
        let mut actions = Vec::new();
        let mut handlers = Vec::new();
        let mut entities = Vec::new();
        let mut systems = Vec::new();
        let mut player_lifecycles = Vec::new();
        let mut regions = Vec::new();
        let mut chat_hooks = Vec::new();
        let mut damage_policies = Vec::new();
        let mut health_hooks = Vec::new();
        let mut observers = Vec::new();
        let mut storage = Vec::new();
        let mut creatures = Vec::new();
        let mut moving = Vec::new();
        let mut machines = Vec::new();
        let mut anchored = Vec::new();
        let mut player_rules = None;
        let mut appearance = None;
        let mut content_budget = crate::content::declarations::budget::Budget::default();
        for package in &packages {
            content_budget
                .reserve(
                    1,
                    crate::content::declarations::budget::package_bytes(package),
                    &package.key,
                )
                .map_err(std::io::Error::other)?;
        }
        // Packages run in lexical order with isolated mutable attempts and reusable
        // physical VMs. Final declarations are canonicalized before ID assignment.
        for package in &packages {
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::other(format!(
                    "{}: installation initialization deadline exceeded (maximum {:?})",
                    package.key,
                    super::capacity::INSTALLATION_WALL_TIME
                )));
            }
            let name = package.key.split_once(':').expect("package key").0;
            let entry = snapshot.entry(name).map_err(std::io::Error::other)?;
            let output = worker
                .submit(
                    Program::Package {
                        snapshot: Arc::clone(&snapshot),
                        entry,
                        invocation: Invocation::Startup,
                    },
                    ScriptInput { tick: 0, seed: 0 },
                )
                .map_err(std::io::Error::other)?;
            let Output::Declarations(declarations) = output else {
                unreachable!("startup execution")
            };
            content_budget
                .reserve_budget(&declarations.content_budget, &package.key)
                .map_err(std::io::Error::other)?;
            if let Some(selection) = declarations.player_rules
                && player_rules.replace(selection).is_some()
            {
                return Err(std::io::Error::other(
                    "duplicate player rules selection across packages",
                ));
            }
            items.extend(declarations.items);
            icons.extend(declarations.icons);
            tags.extend(declarations.tags);
            if let Some(selection) = declarations.appearance
                && appearance.replace(selection).is_some()
            {
                return Err(std::io::Error::other(
                    "duplicate appearance across packages",
                ));
            }
            blocks.extend(declarations.blocks);
            let attempted_texture_bytes = textures
                .iter()
                .chain(&declarations.textures)
                .map(|texture: &PackageTexture| texture.definition.png.len())
                .sum::<usize>();
            if attempted_texture_bytes > 64 * 1024 * 1024 {
                return Err(std::io::Error::other(format!(
                    "{} texture PNG bytes/installation: attempted {attempted_texture_bytes}; maximum 67108864",
                    package.key
                )));
            }
            textures.extend(declarations.textures);
            models.extend(declarations.models);
            handlers.extend(declarations.handlers);
            player_lifecycles.extend(declarations.player_lifecycles);
            regions.extend(declarations.regions);
            chat_hooks.extend(declarations.chat_hooks);
            damage_policies.extend(declarations.damage_policies);
            health_hooks.extend(declarations.health_hooks);
            observers.extend(declarations.observers);
            entities.extend(declarations.entities);
            storage.extend(declarations.storage);
            creatures.extend(declarations.creatures);
            moving.extend(declarations.moving);
            machines.extend(declarations.machines);
            anchored.extend(declarations.anchored);
            systems.extend(declarations.systems);
            for declaration in declarations.actions {
                actions.push(super::gameplay::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
            for declaration in declarations.generation {
                generation.push(super::generation::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
        }
        let mut result = Self {
            snapshot: Arc::clone(&snapshot),
            appearance,
            player_rules,
            client_bundle: Arc::clone(snapshot.client_bundle()),
            packages,
            items,
            icons,
            models,
            tags,
            blocks,
            textures,
            generation,
            actions,
            handlers,
            entities,
            systems,
            player_lifecycles,
            regions,
            chat_hooks,
            damage_policies,
            health_hooks,
            observers,
            storage,
            creatures,
            moving,
            machines,
            anchored,
        };
        result.items.sort_by(|a, b| a.key.cmp(&b.key));
        result.blocks.sort_by(|a, b| a.key.cmp(&b.key));
        result
            .textures
            .sort_by(|a, b| a.definition.key.cmp(&b.definition.key));
        result.regions.sort_by(|a, b| a.key.cmp(&b.key));
        result.chat_hooks.sort_by(|a, b| a.key.cmp(&b.key));
        result.damage_policies.sort_by(|a, b| a.key.cmp(&b.key));
        result.health_hooks.sort_by(|a, b| a.key.cmp(&b.key));
        result.systems.sort_by(|a, b| a.key.cmp(&b.key));
        result.generation.sort_by(|a, b| a.key.cmp(&b.key));
        composition::validate(&result.packages, &result.systems)?;
        if std::time::Instant::now() >= deadline {
            return Err(std::io::Error::other(format!(
                "installation initialization deadline exceeded (maximum {:?})",
                super::capacity::INSTALLATION_WALL_TIME
            )));
        }
        result.anchored.sort_by(|a, b| a.entity.cmp(&b.entity));
        snapshot
            .install_creature_initials(&result.creatures)
            .map_err(std::io::Error::other)?;
        result.client_bundle = Arc::new(
            snapshot
                .client_bundle()
                .with_startup(&result)
                .map_err(std::io::Error::other)?,
        );
        if std::time::Instant::now() >= deadline {
            return Err(std::io::Error::other(format!(
                "installation initialization deadline exceeded during bundle preparation (maximum {:?})",
                super::capacity::INSTALLATION_WALL_TIME
            )));
        }
        Ok(result)
    }
}

impl Extension for Declarations {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        for key in self
            .client_bundle
            .sounds()
            .keys()
            .filter(|key| !key.starts_with("bloxgloom:"))
        {
            registrar.sound(key.clone())?;
        }
        for observer in &self.observers {
            registrar.gameplay_observer(observer.clone())?;
        }
        for registration in &self.player_lifecycles {
            registrar.player_lifecycle(registration.clone())?;
        }
        for region in &self.regions {
            registrar.region(region.clone())?;
        }
        for policy in &self.damage_policies {
            registrar.damage_policy(policy.clone())?;
        }
        for hook in &self.health_hooks {
            registrar.health_hook(hook.clone())?;
        }
        for hook in &self.chat_hooks {
            registrar.chat_hook(hook.clone())?;
        }
        for package in &self.packages {
            registrar.package(package.clone())?;
        }
        for texture in &self.textures {
            registrar.texture(texture.definition.clone())?;
        }
        for block in &self.blocks {
            registrar.block(block.clone())?;
        }
        for item in &self.items {
            registrar.item(item.clone())?;
        }
        for model in &self.models {
            registrar.model_asset(model.definition.clone())?;
        }
        for icon in &self.icons {
            registrar.item_icon(icon.clone())?;
        }
        for tag in &self.tags {
            registrar.tag(tag.clone())?;
        }
        for generation in &self.generation {
            registrar.generation_contributor(generation.clone())?;
        }
        for (action, handler) in &self.actions {
            registrar.action(action.clone())?;
            registrar.gameplay_handler(handler.clone())?;
        }
        for system in &self.systems {
            registrar.owner_system(system.clone())?;
        }
        for handler in &self.handlers {
            registrar.gameplay_handler(handler.clone())?;
        }
        for entity in &self.entities {
            registrar.gameplay_entity(entity.clone())?;
        }
        for entity in &self.moving {
            registrar.moving_entity(entity.clone())?;
        }
        for creature in &self.creatures {
            registrar.mobile_entity(creature.clone())?;
        }
        for declaration in &self.machines {
            registrar.machine(declaration.machine.clone())?;
            registrar.inventory_screen(declaration.screen.clone())?;
        }
        for declaration in &self.anchored {
            registrar.anchored_block_entity(declaration.clone())?;
        }
        for declaration in &self.storage {
            registrar.storage_block_entity(declaration.storage.clone())?;
            registrar.inventory_screen(declaration.screen.clone())?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Pending {
    pub(super) player_lifecycles: Vec<bloxgloom_host_api::players::Registration>,
    pub(super) regions: Vec<bloxgloom_host_api::regions::Registration>,
    pub(super) chat_hooks: Vec<bloxgloom_host_api::chat::Registration>,
    pub(super) damage_policies: Vec<bloxgloom_host_api::player_health::DamageRegistration>,
    pub(super) health_hooks: Vec<bloxgloom_host_api::player_health::HookRegistration>,
    pub(super) observers: Vec<bloxgloom_host_api::gameplay::ObserverRegistration>,
    appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    player_rules: Option<crate::content::player::Selection>,
    items: Vec<Item>,
    icons: Vec<bloxgloom_host_api::icon::ItemIcon>,
    pub(super) models: Vec<models::PackageModel>,
    tags: Vec<bloxgloom_host_api::content::Tag>,
    blocks: Vec<Block>,
    textures: Vec<PackageTexture>,
    texture_bytes: usize,
    content_budget: crate::content::declarations::budget::Budget,
    pub(super) generation: Vec<super::generation::Declaration>,
    pub(super) actions: Vec<super::gameplay::Declaration>,
    pub(super) handlers: Vec<bloxgloom_host_api::gameplay::HandlerRegistration>,
    pub(super) entities: Vec<bloxgloom_host_api::gameplay::EntityDefinition>,
    pub(super) storage: Vec<storage::Declaration>,
    pub(super) creatures: Vec<bloxgloom_host_api::entity::MobileEntity>,
    pub(super) moving: Vec<bloxgloom_host_api::motion::MovingEntity>,
    pub(super) machines: Vec<machine::Declaration>,
    pub(super) anchored: Vec<bloxgloom_host_api::anchored::AnchoredBlockEntity>,
    pub(super) systems: Vec<bloxgloom_host_api::system::System>,
    pub(super) error: Option<&'static str>,
    diagnostic: Option<String>,
}

impl Pending {
    fn reserve_content(
        &mut self,
        count: usize,
        bytes: usize,
        key: &str,
    ) -> Result<(), &'static str> {
        self.content_budget
            .reserve(count, bytes, key)
            .map_err(|error| {
                self.diagnostic = Some(error.0.replace("/installation", "/package"));
                "content declaration admission limit exceeded"
            })
    }

    pub(super) fn reject(&mut self, error: &'static str, declaration: &str) -> mlua::Error {
        let message = match error {
            "content declaration admission limit exceeded" => {
                self.diagnostic.clone().unwrap_or_else(|| error.into())
            }
            "items/package limit exceeded" => format!(
                "items/package: attempted {}; maximum {}",
                self.items.len() + 1,
                MAX_ITEMS_PER_PACKAGE
            ),
            "blocks/package limit exceeded" => format!(
                "blocks/package: attempted {}; maximum {}",
                self.blocks.len() + 1,
                MAX_BLOCKS_PER_PACKAGE
            ),
            "textures/package limit exceeded" => format!(
                "textures/package: attempted {}; maximum {}",
                self.textures.len() + 1,
                MAX_TEXTURES_PER_PACKAGE
            ),
            "texture PNG bytes/package limit exceeded" => format!(
                "texture PNG bytes/package: attempted {}; maximum 67108864",
                self.texture_bytes
            ),
            "systems/package limit exceeded" => format!(
                "systems/package: attempted {}; maximum {}",
                self.systems.len() + 1,
                super::capacity::SYSTEMS_PER_PACKAGE
            ),
            "generators/package limit exceeded" => format!(
                "generators/package: attempted {}; maximum {}",
                self.generation.len() + 1,
                super::capacity::GENERATORS_PER_PACKAGE
            ),
            _ => error.into(),
        };
        let diagnostic = format!("{declaration} rejected: {message}");
        if self.error.is_none() {
            self.diagnostic = Some(diagnostic.clone());
        }
        self.error.get_or_insert(error);
        mlua::Error::RuntimeError(diagnostic)
    }
}

pub(super) fn declaration_key(value: &Value) -> String {
    text(value.clone()).unwrap_or_else(|_| "<invalid-key>".into())
}

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    namespace: &str,
    snapshot: &Arc<PackageSnapshot>,
) -> mlua::Result<Pending> {
    let permits_content = snapshot.permits_content(namespace);
    let pending = Rc::new(RefCell::new(Pending::default()));
    let region = player_world::region(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let chat = player_world::chat(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let player_rules = player::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
    let appearance = appearance::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
    let anchored = anchored::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let storage = storage::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let creature = creature::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let machine = machine::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let register_model = models::declarer(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        false,
    )?;
    let register_player_model = models::declarer(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        true,
    )?;
    let icon = icons::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
    let tag = tag::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
    let capture = Rc::clone(&pending);
    let texture_capture = Rc::clone(&pending);
    let block_capture = Rc::clone(&pending);
    let asset_snapshot = Arc::clone(snapshot);
    let generation =
        super::generation::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let action =
        super::gameplay::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let system =
        super::system::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let entity =
        super::entities::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let handler = super::gameplay::handler_declarer(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
    )?;
    let players =
        super::players::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let observer = super::observers::declarer(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        false,
    )?;
    let weather_observer = super::observers::declarer(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        true,
    )?;
    let damage_policy = player_health::declare(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        false,
    )?;
    let health_hook = player_health::declare(
        lua,
        Rc::clone(&pending),
        namespace,
        Arc::clone(snapshot),
        true,
    )?;
    let namespace = namespace.to_owned();
    let moving_namespace = namespace.clone();
    let texture_namespace = namespace.clone();
    let block_namespace = namespace.clone();
    let register = lua.create_function(
        move |_, (key, name, texture, options): (Value, Value, Value, Value)| {
            let mut pending = capture.borrow_mut();
            let declaration = format!("register_item {}", declaration_key(&key));
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permits_content {
                    return Err("register_item requires bloxgloom:content/v1");
                }
                if pending.items.len() >= MAX_ITEMS_PER_PACKAGE {
                    return Err("items/package limit exceeded");
                }
                // Validate lengths before copying VM strings into host allocations.
                // Only the bounded item option parser traverses a table; do not
                // invoke metamethods while decoding declarations.
                let key = text(key)?;
                let name = text(name)?;
                let texture = text(texture)?;
                let (sprite, drop_size, drop_animation, drop_policy, components) =
                    item::options(options)?;
                let Some((owner, local)) = key.split_once(':') else {
                    return Err("item key must be namespaced");
                };
                if owner != namespace || !super::package::manifest::identifier(local) {
                    return Err("item key must belong to the startup package namespace");
                }
                let Some((texture_owner, texture_local)) = texture.split_once(':') else {
                    return Err("item texture must be a namespaced key");
                };
                if (texture_owner != "bloxgloom" && texture_owner != namespace)
                    || !super::package::manifest::identifier(texture_local)
                {
                    return Err("startup items require a builtin or package-owned texture");
                }
                if pending.items.iter().any(|item| item.key == key)
                    || pending.blocks.iter().any(|block| block.key == key)
                {
                    return Err("duplicate startup item");
                }
                pending.reserve_content(1, 2048, &key)?;
                pending.items.push(Item {
                    key,
                    name,
                    texture,
                    swatch: [1.0; 4],
                    placeable: None,
                    sprite,
                    drop_size,
                    drop_animation,
                    drop_policy,
                    components,
                });
                Ok(())
            })();
            result.map_err(|error| {
                // pcall cannot turn a rejected host declaration into partial success.
                pending.reject(error, &declaration)
            })
        },
    )?;
    let register_texture =
        lua.create_function(move |_, (key, asset, options): (Value, Value, Value)| {
            let mut pending = texture_capture.borrow_mut();
            let declaration = format!("register_texture {}", declaration_key(&key));
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permits_content {
                    return Err("register_texture requires bloxgloom:content/v1");
                }
                if pending.textures.len() >= MAX_TEXTURES_PER_PACKAGE {
                    return Err("textures/package limit exceeded");
                }
                let key = text(key)?;
                let asset = text(asset)?;
                let alpha_cutout = match options {
                    Value::Nil => false,
                    Value::Table(table) => {
                        let mut cutout = false;
                        for (index, pair) in table.pairs::<Value, Value>().enumerate() {
                            if index >= 1 {
                                return Err("unknown texture option");
                            }
                            match pair.map_err(|_| "invalid texture option")? {
                                (Value::String(key), Value::Boolean(value))
                                    if key.as_bytes().as_ref() == b"alpha_cutout" =>
                                {
                                    cutout = value
                                }
                                _ => return Err("texture option must be alpha_cutout boolean"),
                            }
                        }
                        cutout
                    }
                    _ => return Err("texture options must be a table"),
                };
                let Some((owner, local)) = key.split_once(':') else {
                    return Err("texture key must be namespaced");
                };
                if owner != texture_namespace || !super::package::manifest::identifier(local) {
                    return Err("texture key must belong to the startup package namespace");
                }
                if !super::package::manifest::identifier(&asset) {
                    return Err("texture asset must be a local declared name");
                }
                let bytes = asset_snapshot
                    .texture_asset(&texture_namespace, &asset)
                    .ok_or("texture asset is not a local declared PNG")?;
                if pending
                    .textures
                    .iter()
                    .any(|texture| texture.definition.key == key)
                {
                    return Err("duplicate startup texture");
                }
                pending.texture_bytes = pending.texture_bytes.saturating_add(bytes.len());
                if pending.texture_bytes > 64 * 1024 * 1024 {
                    return Err("texture PNG bytes/package limit exceeded");
                }
                pending.reserve_content(1, bytes.len() + 256, &key)?;
                pending.textures.push(PackageTexture {
                    definition: Texture {
                        key,
                        png: std::borrow::Cow::Owned(bytes.to_vec()),
                        stitch_edges: true,
                        stitch_vertical: true,
                        alpha_cutout,
                        emission_strength: 0.0,
                    },
                    asset,
                });
                Ok(())
            })();
            result.map_err(|error| pending.reject(error, &declaration))
        })?;
    let register_block = lua.create_function(
        move |_, (key, name, texture, options): (Value, Value, Value, Value)| {
            let mut pending = block_capture.borrow_mut();
            let declaration = format!("register_block {}", declaration_key(&key));
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permits_content {
                    return Err("register_block requires bloxgloom:content/v1");
                }
                if pending.blocks.len() >= MAX_BLOCKS_PER_PACKAGE {
                    return Err("blocks/package limit exceeded");
                }
                if pending.items.len() >= MAX_ITEMS_PER_PACKAGE {
                    return Err("items/package limit exceeded");
                }
                let key = text(key)?;
                let name = text(name)?;
                let texture = text(texture)?;
                if key.split_once(':').is_none_or(|(owner, local)| {
                    owner != block_namespace || !super::package::manifest::identifier(local)
                }) {
                    return Err("block key must belong to the startup package namespace");
                }
                if texture.split_once(':').is_none_or(|(owner, local)| {
                    owner != block_namespace || !super::package::manifest::identifier(local)
                }) {
                    return Err("startup blocks require a package-owned texture");
                }
                if !pending.textures.iter().any(|t| t.definition.key == texture) {
                    return Err("startup block requires an already registered package texture");
                }
                if pending.blocks.iter().any(|b| b.key == key)
                    || pending.items.iter().any(|i| i.key == key)
                {
                    return Err("duplicate startup block or item");
                }
                let block = block::cube(key.clone(), name.clone(), texture.clone(), options)?;
                for face in [
                    &block.textures.top,
                    &block.textures.side,
                    &block.textures.bottom,
                ] {
                    if face.split_once(':').is_none_or(|(owner, local)| {
                        owner != block_namespace || !super::package::manifest::identifier(local)
                    }) || !pending
                        .textures
                        .iter()
                        .any(|registered| registered.definition.key == *face)
                    {
                        return Err("block faces require registered package-owned textures");
                    }
                }
                if block.material == bloxgloom_host_api::content::Material::Cutout
                    && [
                        &block.textures.top,
                        &block.textures.side,
                        &block.textures.bottom,
                    ]
                    .iter()
                    .any(|face| {
                        !pending.textures.iter().any(|texture| {
                            texture.definition.key == face.as_str()
                                && texture.definition.alpha_cutout
                        })
                    })
                {
                    return Err("cutout blocks require cutout face textures");
                }
                for state in &block.states {
                    if let Some(faces) = &state.textures {
                        for face in [&faces.top, &faces.side, &faces.bottom] {
                            if face.split_once(':').is_none_or(|(owner, local)| {
                                owner != block_namespace
                                    || !super::package::manifest::identifier(local)
                            }) || !pending.textures.iter().any(|texture| {
                                texture.definition.key == *face
                                    && (block.material
                                        != bloxgloom_host_api::content::Material::Cutout
                                        || texture.definition.alpha_cutout)
                            }) {
                                return Err(
                                    "state faces require registered matching package textures",
                                );
                            }
                        }
                    }
                }
                pending.reserve_content(
                    2,
                    crate::content::declarations::budget::block_bytes(&block) + 2048,
                    &key,
                )?;
                let placeable = block::placement_state(&block);
                pending.blocks.push(block);
                pending.items.push(Item {
                    key: key.clone(),
                    name,
                    texture,
                    swatch: [1.0; 4],
                    placeable: Some(placeable),
                    sprite: false,
                    drop_size: bloxgloom_host_api::content::DropSize::Normal,
                    drop_animation: Default::default(),
                    drop_policy: Default::default(),
                    components: Components::None,
                });
                Ok(())
            })();
            result.map_err(|error| pending.reject(error, &declaration))
        },
    )?;
    let host = lua.create_table()?;
    host.set("register_model", register_model)?;
    host.set("register_player_model", register_player_model)?;
    host.set("register_region", region)?;
    host.set("register_chat_hook", chat)?;
    host.set("register_damage_policy", damage_policy)?;
    host.set("register_health_hook", health_hook)?;
    host.set("register_item", register)?;
    host.set("register_texture", register_texture)?;
    host.set("register_block", register_block)?;
    host.set("register_tag", tag)?;
    host.set("register_item_icon", icon)?;
    host.set("register_anchored", anchored)?;
    host.set("register_storage", storage)?;
    host.set("register_creature", creature)?;
    host.set(
        "register_moving_entity",
        moving::declarer(
            lua,
            Rc::clone(&pending),
            &moving_namespace,
            Arc::clone(snapshot),
        )?,
    )?;
    host.set("register_machine", machine)?;
    host.set("register_generator", generation)?;
    host.set("register_action", action)?;
    host.set("register_committed_observer", observer)?;
    host.set("register_weather_observer", weather_observer)?;
    host.set("register_player_lifecycle", players)?;
    host.set("register_handler", handler)?;
    host.set("register_system", system)?;
    host.set("register_entity", entity)?;
    host.set("register_player_rules", player_rules)?;
    host.set("register_player_appearance", appearance)?;
    host.set_readonly(true);
    entry.call::<()>(host)?;
    let mut pending = pending.borrow_mut();
    if let Some(error) = pending.error {
        return Err(mlua::Error::RuntimeError(
            pending.diagnostic.clone().unwrap_or_else(|| error.into()),
        ));
    }
    Ok(std::mem::take(&mut *pending))
}

fn text(value: Value) -> Result<String, &'static str> {
    let Value::String(value) = value else {
        return Err("register_item expects three UTF-8 strings");
    };
    if value.as_bytes().is_empty() || value.as_bytes().len() > 255 {
        return Err("startup item strings must contain 1..=255 bytes");
    }
    value
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| "invalid UTF-8 in startup item")
}
