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
//! Each package may declare 32 bounded PNG-backed textures by local asset name
//! and 32 items with builtin or own registered textures. `register_block(key,
//! name, texture)` additionally declares up to 32 uniform opaque, solid cubes
//! with one default state and a same-key placeable item. Cubes use white swatches,
//! no properties, no emission, no flammability or plant support, and the ordinary
//! 128 item stack cap. No arbitrary geometry or material flags are exposed.
//! Keys must belong to the entry package; imported helpers may receive the
//! callback but gain only that entry's authority. No undeclared assets or grants
//! are registered. Texture/item and entity schema metadata are exported
//! as inert client metadata. Handler/owner-system compatibility hashes do not
//! export callbacks, owner seeds, codecs, or execution authority; generators
//! export key/revision only and remain server-executed.
//! Every dependency entry runs too; library packages can
//! return a no-op entry and export helpers from other modules.
//! With `requires bloxgloom:generation/v1`, an entry may additionally declare one
//! `host.register_generator("demo:terrain", 1, "demo:terrain")`. The named own
//! module returns a function receiving a chunk context (see `generation`).
//! `bloxgloom:actions/v1` permits one `register_action` declaration (see `gameplay`).
//! It also permits 32 exact `register_handler` gameplay decision owners.
//! It permits 32 `register_entity` fixed-byte schemas (see `entities`) as well.
//! `bloxgloom:owner_systems/v1` permits one persistent chunk `register_system`
//! declaration (see `system`); plans run on existing owner workers, not this worker.
//!
//! The bundle records identity:package with public contract version 1. Source
//! semver is checked exactly during discovery, not persisted or converted to
//! the public u32 version. Retry/restart rediscovers sources and uses fresh VMs;
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

const MAX_ITEMS_PER_PACKAGE: usize = 32;
const MAX_TEXTURES_PER_PACKAGE: usize = 32;
const MAX_BLOCKS_PER_PACKAGE: usize = 32;
mod block;
pub(in crate::server::script) use block::cube;
pub(in crate::server::script) use block::extended as extended_block;
mod appearance;
mod item;
mod player;

pub(super) struct PackageTexture {
    pub(super) definition: Texture,
    pub(super) asset: String,
}

pub(in crate::server) struct Declarations {
    pub(in crate::server) appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    pub(in crate::server) player_rules: Option<crate::content::player::Selection>,
    pub(in crate::server) client_bundle: Arc<super::package::client::ClientBundle>,
    pub(super) packages: Vec<bloxgloom_host_api::composition::Package>,
    pub(super) items: Vec<Item>,
    pub(super) blocks: Vec<Block>,
    pub(super) textures: Vec<PackageTexture>,
    pub(super) generation: Vec<bloxgloom_host_api::generation::Registration>,
    pub(super) actions: Vec<super::gameplay::Registration>,
    pub(super) handlers: Vec<bloxgloom_host_api::gameplay::HandlerRegistration>,
    pub(super) entities: Vec<bloxgloom_host_api::gameplay::EntityDefinition>,
    pub(super) systems: Vec<bloxgloom_host_api::system::System>,
}

impl Declarations {
    pub(in crate::server) fn discover(root: &Path) -> std::io::Result<Self> {
        let snapshot = Arc::new(PackageSnapshot::discover(root).map_err(std::io::Error::other)?);
        let packages = snapshot.startup_packages().map_err(std::io::Error::other)?;
        let worker = ScriptWorker::spawn(super::Limits::default())?;
        let mut items = Vec::new();
        let mut blocks = Vec::new();
        let mut textures = Vec::new();
        let mut generation = Vec::new();
        let mut actions = Vec::new();
        let mut handlers = Vec::new();
        let mut entities = Vec::new();
        let mut systems = Vec::new();
        let mut player_rules = None;
        let mut appearance = None;
        // At most 64 packages * 32 items, in lexical package order. Each entry
        // gets a fresh VM; completion timing cannot affect assignment order.
        for package in &packages {
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
            if let Some(selection) = declarations.player_rules
                && player_rules.replace(selection).is_some()
            {
                return Err(std::io::Error::other(
                    "duplicate player rules selection across packages",
                ));
            }
            items.extend(declarations.items);
            if let Some(selection) = declarations.appearance
                && appearance.replace(selection).is_some()
            {
                return Err(std::io::Error::other(
                    "duplicate appearance across packages",
                ));
            }
            blocks.extend(declarations.blocks);
            textures.extend(declarations.textures);
            handlers.extend(declarations.handlers);
            entities.extend(declarations.entities);
            if let Some(system) = declarations.system {
                systems.push(system);
            }
            if let Some(declaration) = declarations.action {
                actions.push(super::gameplay::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
            if let Some(declaration) = declarations.generation {
                generation.push(super::generation::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
        }
        let mut result = Self {
            appearance,
            player_rules,
            client_bundle: Arc::clone(snapshot.client_bundle()),
            packages,
            items,
            blocks,
            textures,
            generation,
            actions,
            handlers,
            entities,
            systems,
        };
        result.client_bundle = Arc::new(
            snapshot
                .client_bundle()
                .with_startup(&result)
                .map_err(std::io::Error::other)?,
        );
        Ok(result)
    }
}

impl Extension for Declarations {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
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
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Pending {
    appearance: Option<bloxgloom_host_api::appearance::Appearance>,
    player_rules: Option<crate::content::player::Selection>,
    items: Vec<Item>,
    blocks: Vec<Block>,
    textures: Vec<PackageTexture>,
    pub(super) generation: Option<super::generation::Declaration>,
    pub(super) action: Option<super::gameplay::Declaration>,
    pub(super) handlers: Vec<bloxgloom_host_api::gameplay::HandlerRegistration>,
    pub(super) entities: Vec<bloxgloom_host_api::gameplay::EntityDefinition>,
    pub(super) system: Option<bloxgloom_host_api::system::System>,
    pub(super) error: Option<&'static str>,
}

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    namespace: &str,
    snapshot: &Arc<PackageSnapshot>,
) -> mlua::Result<Pending> {
    let permits_content = snapshot.permits_content(namespace);
    let pending = Rc::new(RefCell::new(Pending::default()));
    let player_rules = player::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
    let appearance = appearance::declarer(lua, Rc::clone(&pending), namespace, permits_content)?;
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
    let namespace = namespace.to_owned();
    let texture_namespace = namespace.clone();
    let block_namespace = namespace.clone();
    let register = lua.create_function(
        move |_, (key, name, texture, options): (Value, Value, Value, Value)| {
            let mut pending = capture.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permits_content {
                    return Err("register_item requires bloxgloom:content/v1");
                }
                if pending.items.len() >= MAX_ITEMS_PER_PACKAGE {
                    return Err("startup item limit exceeded (32 per package)");
                }
                // Validate lengths before copying VM strings into host allocations.
                // Only the bounded item option parser traverses a table; do not
                // invoke metamethods while decoding declarations.
                let key = text(key)?;
                let name = text(name)?;
                let texture = text(texture)?;
                let (sprite, drop_size, drop_animation, drop_policy) = item::options(options)?;
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
                    components: bloxgloom_host_api::content::Components::None,
                });
                Ok(())
            })();
            result.map_err(|error| {
                // pcall cannot turn a rejected host declaration into partial success.
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )?;
    let register_texture = lua.create_function(move |_, (key, asset): (Value, Value)| {
        let mut pending = texture_capture.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !permits_content {
                return Err("register_texture requires bloxgloom:content/v1");
            }
            if pending.textures.len() >= MAX_TEXTURES_PER_PACKAGE {
                return Err("startup texture limit exceeded (32 per package)");
            }
            let key = text(key)?;
            let asset = text(asset)?;
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
            pending.textures.push(PackageTexture {
                definition: Texture {
                    key,
                    png: std::borrow::Cow::Owned(bytes.to_vec()),
                    stitch_edges: true,
                    stitch_vertical: true,
                    alpha_cutout: false,
                    emission_strength: 0.0,
                },
                asset,
            });
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })?;
    let register_block = lua.create_function(
        move |_, (key, name, texture, options): (Value, Value, Value, Value)| {
            let mut pending = block_capture.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !permits_content {
                    return Err("register_block requires bloxgloom:content/v1");
                }
                if pending.blocks.len() >= MAX_BLOCKS_PER_PACKAGE {
                    return Err("startup block limit exceeded (32 per package)");
                }
                if pending.items.len() >= MAX_ITEMS_PER_PACKAGE {
                    return Err("startup item limit exceeded (32 per package)");
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
                pending.blocks.push(block);
                pending.items.push(Item {
                    key: key.clone(),
                    name,
                    texture,
                    swatch: [1.0; 4],
                    placeable: Some(key),
                    sprite: false,
                    drop_size: bloxgloom_host_api::content::DropSize::Normal,
                    drop_animation: Default::default(),
                    drop_policy: Default::default(),
                    components: Components::None,
                });
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )?;
    let host = lua.create_table()?;
    host.set("register_item", register)?;
    host.set("register_texture", register_texture)?;
    host.set("register_block", register_block)?;
    host.set("register_generator", generation)?;
    host.set("register_action", action)?;
    host.set("register_handler", handler)?;
    host.set("register_system", system)?;
    host.set("register_entity", entity)?;
    host.set("register_player_rules", player_rules)?;
    host.set("register_player_appearance", appearance)?;
    host.set_readonly(true);
    entry.call::<()>(host)?;
    let mut pending = pending.borrow_mut();
    if let Some(error) = pending.error {
        return Err(mlua::Error::RuntimeError(error.into()));
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
