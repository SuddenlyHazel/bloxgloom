//! Canonical, client-safe package set, independent of filesystem paths. Versions 7–13 and 20–29
//! use an uncompressed little-endian length-prefixed format, not a save or network
//! protocol. No entry, executable server capabilities, local paths or original manifests are
//! exported. All package identities/direct exact dependencies remain present,
//! including empty server-only libraries, so dependency validation is complete.
//!
//! SHA-256 identifies exact canonical bytes. Verification requires an expected
//! key from a trusted session/manifest; a hash supplied by the same untrusted
//! sender as the bytes is integrity checking, not authentication or signing.
//! Texture payloads remain opaque PNG-classified bytes unless a verified startup
//! declaration binds one to a catalog texture; those PNGs are decoded and bounded
//! on preparation workers before publication.
//! Separately classified UI assets are validated/prepared by ui::authored before
//! publication. V8 adds a bounded size byte to each item record only when
//! a non-default drop size is declared; unchanged declarations keep exact V7 bytes.
//! V9 adds per-item drop animation; V10/V11 append selected player rules to
//! V7/V8 respectively, and V12 carries both player rules and drop animation.
//! V13 carries item size/animation, explicitly optional player rules, and a
//! required bounded player appearance record (including the model identity).
//! V20/V21 add per-item authoritative drop policies without/with player rules;
//! V22 combines those policies with the V13 appearance record and optional rules.
//! V23 adds one-state block face/material properties, while retaining optional
//! player rules/appearance and per-item metadata without changing earlier bytes.
//! V24 adds bounded item/block tag declarations; default packages retain V7–23.
//! V25 adds cutout texture flags and block geometry/material to the V24 grammar.
//! V26 adds explicit bounded property schemas and legal state lists to V25.
//! V27 adds opaque item component schema identity to V26.
//! V28 adds per-state package-owned face textures to V27.
//! V29 adds single-cell host-owned storage with negotiated inventory screens.
//! V30 adds bounded storage screen hints and labeled slot groups.
//! V31 adds server-authored mobile creature presentation and schema identity.
//! V32 adds host-owned process machines and inventory screens.
//! V33 adds bounded creature interaction requests and animation parameters.
//! V34 adds bounded process-machine recipe lists.
//! V35 adds bounded machine automation ports and transfer work metadata.
//! Absent selections preserve earlier bytes. No version changes wire framing or saves. Artifacts older than V7 are
//! rejected; there is no conversion or partial install.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::manifest::{SourceSide, identifier, valid_version};
use super::{MAX_ASSET_BYTES, MAX_ASSETS, MAX_MODULES, MAX_PACKAGES, MAX_SOURCE_BYTES};
use super::{MAX_TOTAL_BYTES, Package, ScriptError, error};

mod declarations;

const MAGIC: &[u8] = b"BGCLIENT\x07";
const SIZED_MAGIC: &[u8] = b"BGCLIENT\x08";
const ANIMATED_MAGIC: &[u8] = b"BGCLIENT\x09";
// V10/V11 append a single complete player selection to V7/V8 startup data.
// Undeclared builtin rules retain the exact earlier bundle and catalog identity.
const PLAYER_MAGIC: &[u8] = b"BGCLIENT\x0a";
const PLAYER_SIZED_MAGIC: &[u8] = b"BGCLIENT\x0b";
const PLAYER_ANIMATED_MAGIC: &[u8] = b"BGCLIENT\x0c";
// V13 always includes item size/animation, an optional-player marker, and one
// required appearance selection. Earlier headers retain their exact grammar.
const APPEARANCE_MAGIC: &[u8] = b"BGCLIENT\x0d";
const POLICY_MAGIC: &[u8] = b"BGCLIENT\x14";
const PLAYER_POLICY_MAGIC: &[u8] = b"BGCLIENT\x15";
const APPEARANCE_POLICY_MAGIC: &[u8] = b"BGCLIENT\x16";
const BLOCK_OPTIONS_MAGIC: &[u8] = b"BGCLIENT\x17";
const TAGS_MAGIC: &[u8] = b"BGCLIENT\x18";
const VISUAL_BLOCKS_MAGIC: &[u8] = b"BGCLIENT\x19";
const BLOCK_STATES_MAGIC: &[u8] = b"BGCLIENT\x1a";
const COMPONENTS_MAGIC: &[u8] = b"BGCLIENT\x1b";
const STATE_TEXTURES_MAGIC: &[u8] = b"BGCLIENT\x1c";
const STORAGE_MAGIC: &[u8] = b"BGCLIENT\x1d";
const SCREEN_LAYOUT_MAGIC: &[u8] = b"BGCLIENT\x1e";
const CREATURE_MAGIC: &[u8] = b"BGCLIENT\x1f";
const MACHINE_MAGIC: &[u8] = b"BGCLIENT\x20";
const CREATURE_OPTIONS_MAGIC: &[u8] = b"BGCLIENT\x21";
const MACHINE_RECIPES_MAGIC: &[u8] = b"BGCLIENT\x22";
const MACHINE_PORTS_MAGIC: &[u8] = b"BGCLIENT\x23";
/// Payloads share the 4 MiB discovery budget. An extra MiB bounds all identity,
/// dependency and record framing overhead (64 packages, 256 modules/256 assets).
/// Two further MiB bound declarative startup metadata. Every record category
/// also has per-package count/string bounds; runtime seeds/state are excluded.
pub const MAX_BUNDLE_BYTES: usize = MAX_TOTAL_BYTES + 3 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Cache namespace, independent of the artifact's version header. The hash
    /// includes that header, so distinct bundle versions cannot collide.
    pub fn cache_name(&self) -> String {
        use std::fmt::Write;
        let mut name = String::from("client-v7-sha256-");
        for byte in self.0 {
            write!(name, "{byte:02x}").expect("write String");
        }
        name
    }
}

#[derive(Debug)]
pub struct ClientSource {
    pub side: ClientSide,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientSide {
    Client,
    Shared,
}

#[derive(Debug)]
pub struct ClientPackage {
    pub version: String,
    pub dependencies: BTreeMap<String, String>,
    pub sources: BTreeMap<String, ClientSource>,
    /// World textures remain opaque. Keys are logical names, never extraction
    /// paths. Separately typed UI assets do not enter this map.
    pub textures: BTreeMap<String, Vec<u8>>,
    pub(crate) ui_assets: BTreeMap<String, (u32, Vec<u8>)>,
    pub(crate) effect_assets: BTreeMap<String, (u32, Vec<u8>)>,
    pub(crate) material_assets: BTreeMap<String, (u32, Vec<u8>)>,
}

/// Immutable bytes and decoded view, published together only after validation.
#[derive(Debug)]
pub struct ClientBundle {
    bytes: Vec<u8>,
    key: CacheKey,
    packages: BTreeMap<String, ClientPackage>,
    declarations: Option<declarations::Startup>,
    ui: Option<std::sync::Arc<crate::ui::authored::Resources>>,
    effect: Option<std::sync::Arc<crate::render::effects::Prepared>>,
    material: Option<std::sync::Arc<crate::render::custom::Source>>,
}

impl ClientBundle {
    pub(crate) fn effect(&self) -> Option<&std::sync::Arc<crate::render::effects::Prepared>> {
        self.effect.as_ref()
    }
    pub(crate) fn material(&self) -> Option<&std::sync::Arc<crate::render::custom::Source>> {
        self.material.as_ref()
    }
    pub(crate) fn ui(&self) -> Option<&std::sync::Arc<crate::ui::authored::Resources>> {
        self.ui.as_ref()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn cache_key(&self) -> CacheKey {
        self.key
    }

    pub fn packages(&self) -> &BTreeMap<String, ClientPackage> {
        &self.packages
    }

    pub(super) fn from_packages(packages: &BTreeMap<String, Package>) -> Result<Self, ScriptError> {
        let mut writer = Writer(MAGIC.to_vec());
        writer.count(packages.len())?;
        for (name, package) in packages {
            writer.field(name.as_bytes())?;
            writer.field(package.manifest.version.as_bytes())?;
            writer.count(package.manifest.dependencies.len())?;
            for (name, version) in &package.manifest.dependencies {
                writer.field(name.as_bytes())?;
                writer.field(version.as_bytes())?;
            }
            writer.count(
                package
                    .manifest
                    .sides
                    .values()
                    .filter(|s| **s != SourceSide::Server)
                    .count(),
            )?;
            for (name, source) in &package.sources {
                let side = match package.manifest.sides[name] {
                    SourceSide::Server => continue,
                    SourceSide::Client => 1,
                    SourceSide::Shared => 2,
                };
                writer.field(name.as_bytes())?;
                writer.count(side)?;
                writer.field(source.as_bytes())?;
            }
            writer.count(package.assets.len())?;
            for (name, bytes) in &package.assets {
                writer.field(name.as_bytes())?;
                writer.count(package.manifest.asset_kinds[name] as usize)?;
                writer.field(bytes)?;
            }
        }
        writer.count(0)?; // Discovery alone has not executed startup declarations.
        let key = CacheKey(Sha256::digest(&writer.0).into());
        // Use the same bounded canonical validator for local publication and
        // later cache/stream decoding. Failure never publishes a partial set.
        Self::decode_verify(&writer.0, key)
    }

    pub fn decode_verify(bytes: &[u8], expected: CacheKey) -> Result<Self, ScriptError> {
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(invalid());
        }
        if CacheKey(Sha256::digest(bytes).into()) != expected {
            return Err(error("<client-bundle>", "SHA-256 integrity mismatch"));
        }
        let mut reader = Reader(bytes);
        let version = reader.take(MAGIC.len())?;
        if ![
            MAGIC,
            SIZED_MAGIC,
            ANIMATED_MAGIC,
            PLAYER_MAGIC,
            PLAYER_SIZED_MAGIC,
            PLAYER_ANIMATED_MAGIC,
            APPEARANCE_MAGIC,
            POLICY_MAGIC,
            PLAYER_POLICY_MAGIC,
            APPEARANCE_POLICY_MAGIC,
            BLOCK_OPTIONS_MAGIC,
            TAGS_MAGIC,
            VISUAL_BLOCKS_MAGIC,
            BLOCK_STATES_MAGIC,
            COMPONENTS_MAGIC,
            STATE_TEXTURES_MAGIC,
            STORAGE_MAGIC,
            SCREEN_LAYOUT_MAGIC,
            CREATURE_MAGIC,
            MACHINE_MAGIC,
            CREATURE_OPTIONS_MAGIC,
            MACHINE_RECIPES_MAGIC,
            MACHINE_PORTS_MAGIC,
        ]
        .contains(&version)
        {
            return Err(invalid());
        }
        let mut packages = BTreeMap::new();
        let mut modules = 0;
        let mut assets = 0;
        let mut payload = 0;
        for _ in 0..reader.count(MAX_PACKAGES)? {
            let name = reader.identifier()?;
            ordered(&packages, &name)?;
            let version = reader.version()?;
            let mut dependencies = BTreeMap::new();
            for _ in 0..reader.count(32)? {
                let dependency = reader.identifier()?;
                ordered(&dependencies, &dependency)?;
                if dependency == name {
                    return Err(invalid());
                }
                dependencies.insert(dependency, reader.version()?);
            }
            let count = reader.count(64.min(MAX_MODULES - modules))?;
            modules += count;
            let mut sources = BTreeMap::new();
            for _ in 0..count {
                let key = reader.identifier()?;
                ordered(&sources, &key)?;
                let side = match reader.count(2)? {
                    1 => ClientSide::Client,
                    2 => ClientSide::Shared,
                    _ => return Err(invalid()),
                };
                let source = reader.text(MAX_SOURCE_BYTES.min(MAX_TOTAL_BYTES - payload))?;
                payload += source.len();
                sources.insert(key, ClientSource { side, source });
            }
            let count = reader.count(64.min(MAX_ASSETS - assets))?;
            assets += count;
            let mut textures = BTreeMap::new();
            let mut ui_assets = BTreeMap::new();
            let mut effect_assets = BTreeMap::new();
            let mut material_assets = BTreeMap::new();
            let mut previous = String::new();
            for _ in 0..count {
                let key = reader.identifier()?;
                if key <= previous {
                    return Err(invalid());
                }
                previous.clone_from(&key);
                let kind = reader.count(9)? as u32;
                if kind == 0 {
                    return Err(invalid());
                }
                let asset_limit = match kind {
                    6 => crate::render::effects::MAX_SHADER_BYTES,
                    7 => 1024,
                    8 => 1024,
                    9 => crate::render::custom::MAX_SHADER_BYTES,
                    _ => MAX_ASSET_BYTES,
                };
                let bytes = reader
                    .field(asset_limit.min(MAX_TOTAL_BYTES - payload))
                    .map_err(|_| {
                        error(&name, format!("asset {key}: invalid or oversized payload"))
                    })?;
                payload += bytes.len();
                if kind == 1 {
                    textures.insert(key, bytes.to_vec());
                } else if kind <= 5 {
                    ui_assets.insert(key, (kind, bytes.to_vec()));
                } else if kind <= 7 {
                    effect_assets.insert(key, (kind, bytes.to_vec()));
                } else {
                    material_assets.insert(key, (kind, bytes.to_vec()));
                }
            }
            packages.insert(
                name,
                ClientPackage {
                    version,
                    dependencies,
                    sources,
                    textures,
                    ui_assets,
                    effect_assets,
                    material_assets,
                },
            );
        }
        let declarations = declarations::Startup::decode(
            &mut reader,
            &packages,
            declarations::Format {
                sized: version == SIZED_MAGIC
                    || version == ANIMATED_MAGIC
                    || version == PLAYER_SIZED_MAGIC
                    || version == PLAYER_ANIMATED_MAGIC
                    || version == APPEARANCE_MAGIC
                    || version == POLICY_MAGIC
                    || version == PLAYER_POLICY_MAGIC
                    || version == APPEARANCE_POLICY_MAGIC
                    || version == BLOCK_OPTIONS_MAGIC
                    || version == TAGS_MAGIC
                    || version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                animated: version == ANIMATED_MAGIC
                    || version == PLAYER_ANIMATED_MAGIC
                    || version == APPEARANCE_MAGIC
                    || version == POLICY_MAGIC
                    || version == PLAYER_POLICY_MAGIC
                    || version == APPEARANCE_POLICY_MAGIC
                    || version == BLOCK_OPTIONS_MAGIC
                    || version == TAGS_MAGIC
                    || version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                player: version == PLAYER_MAGIC
                    || version == PLAYER_SIZED_MAGIC
                    || version == PLAYER_ANIMATED_MAGIC
                    || version == PLAYER_POLICY_MAGIC,
                appearance: version == APPEARANCE_MAGIC || version == APPEARANCE_POLICY_MAGIC,
                policy: version == POLICY_MAGIC
                    || version == PLAYER_POLICY_MAGIC
                    || version == APPEARANCE_POLICY_MAGIC
                    || version == BLOCK_OPTIONS_MAGIC
                    || version == TAGS_MAGIC
                    || version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                extended_blocks: version == BLOCK_OPTIONS_MAGIC
                    || version == TAGS_MAGIC
                    || version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                tags: version == TAGS_MAGIC
                    || version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                visual_blocks: version == VISUAL_BLOCKS_MAGIC
                    || version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                block_states: version == BLOCK_STATES_MAGIC
                    || version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                components: version == COMPONENTS_MAGIC
                    || version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                state_textures: version == STATE_TEXTURES_MAGIC
                    || version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                storage: version == STORAGE_MAGIC
                    || version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                screen_layout: version == SCREEN_LAYOUT_MAGIC
                    || version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                creatures: version == CREATURE_MAGIC
                    || version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                creature_options: version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                machines: version == MACHINE_MAGIC
                    || version == CREATURE_OPTIONS_MAGIC
                    || version == MACHINE_RECIPES_MAGIC
                    || version == MACHINE_PORTS_MAGIC,
                machine_recipes: version == MACHINE_RECIPES_MAGIC || version == MACHINE_PORTS_MAGIC,
                machine_ports: version == MACHINE_PORTS_MAGIC,
            },
        )?;
        if !reader.0.is_empty() {
            return Err(invalid());
        }
        for package in packages.values() {
            for (name, version) in &package.dependencies {
                if packages.get(name).is_none_or(|p| p.version != *version) {
                    return Err(error("<client-bundle>", "missing exact dependency"));
                }
            }
        }
        let ui = crate::ui::authored::Resources::compile(&packages)
            .map_err(|message| error("<client-ui>", message))?
            .map(std::sync::Arc::new);
        let effect = crate::render::effects::prepare(&packages)
            .map_err(|message| error("<client-effect>", message))?
            .map(std::sync::Arc::new);
        let material = crate::render::custom::prepare_assets(&packages)
            .map_err(|message| error("<client-material>", message))?
            .map(std::sync::Arc::new);
        Ok(Self {
            bytes: bytes.to_vec(),
            key: expected,
            packages,
            declarations,
            ui,
            effect,
            material,
        })
    }
}

fn ordered<T>(map: &BTreeMap<String, T>, next: &str) -> Result<(), ScriptError> {
    if map
        .last_key_value()
        .is_some_and(|(last, _)| last.as_str() >= next)
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> ScriptError {
    error(
        "<client-bundle>",
        "invalid, noncanonical or oversized client bundle",
    )
}

struct Writer(Vec<u8>);

impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Result<(), ScriptError> {
        if bytes.len() > MAX_BUNDLE_BYTES - self.0.len() {
            return Err(invalid());
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }

    fn count(&mut self, count: usize) -> Result<(), ScriptError> {
        self.put(&u32::try_from(count).map_err(|_| invalid())?.to_le_bytes())
    }

    fn field(&mut self, bytes: &[u8]) -> Result<(), ScriptError> {
        self.count(bytes.len())?;
        self.put(bytes)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ScriptError> {
        let (bytes, rest) = self.0.split_at_checked(count).ok_or_else(invalid)?;
        self.0 = rest;
        Ok(bytes)
    }

    fn count(&mut self, max: usize) -> Result<usize, ScriptError> {
        let count = u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")) as usize;
        if count > max {
            return Err(invalid());
        }
        Ok(count)
    }

    fn field(&mut self, max: usize) -> Result<&'a [u8], ScriptError> {
        let len = self.count(max)?;
        self.take(len)
    }

    fn text(&mut self, max: usize) -> Result<String, ScriptError> {
        Ok(std::str::from_utf8(self.field(max)?)
            .map_err(|_| invalid())?
            .to_owned())
    }

    fn identifier(&mut self) -> Result<String, ScriptError> {
        let text = self.text(64)?;
        if !identifier(&text) {
            return Err(invalid());
        }
        Ok(text)
    }

    fn version(&mut self) -> Result<String, ScriptError> {
        let text = self.text(32)?;
        if !valid_version(&text) {
            return Err(invalid());
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests;
