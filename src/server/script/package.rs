//! Bounded, deterministic local package snapshots. No filesystem access occurs
//! after discovery, including on retries or imports.
//!
//! # Local package formats v1 and v2
//!
//! The root contains only immediate package directories, named by identity.
//! Each contains a UTF-8 `package.txt`, for example:
//!
//! ```text
//! format 1
//! package example
//! version 1.0.0
//! entry main
//! dependency arithmetic 1.2.0
//! module main scripts/main.luau
//! module helper scripts/helper.luau
//! requires bloxgloom:content/v1
//! ```
//!
//! Declarations are whitespace-separated, one per line; blank lines are allowed.
//! Unknown declarations, duplicates and comments are errors. The four singleton
//! declarations are required. Versions are exact three-component u32 triples,
//! without leading zeroes, ranges or prerelease suffixes. Dependencies must exist
//! at exactly the declared version; self-dependencies are forbidden. Discovery
//! reads packages/modules lexically, not in filesystem or manifest line order.
//!
//! Identifiers contain 1–64 lowercase ASCII letters, digits, `_` or `-`. Module
//! paths end in `.luau`, have at most eight components and 240 bytes, and use only
//! ASCII letters/digits, `_`, `-`, `.`, and `/`. Empty, `.` and `..` components
//! are forbidden. Unlisted package files are ignored, not recursively scanned.
//! Bounds: 64 packages, 32 dependencies and 64 modules per package, 256 modules
//! total, 16 KiB per manifest, 64 KiB per source, and 4 MiB aggregate file bytes.
//! Up to 32 distinct `requires` capability strings (255 bytes each) are allowed;
//! startup, not integer execution, validates which capabilities are supported.
//! Root paths have at most 4096 bytes/64 components and no parent traversal.
//!
//! Format 1 is unchanged: every module is server-only and nothing but package
//! identity/version/direct dependencies enters the client bundle. Format 2 uses
//! the same singleton, dependency and capability declarations, but replaces
//! module lines with explicit sides and optionally declares texture assets:
//!
//! ```text
//! format 2
//! package example
//! version 1.0.0
//! entry main
//! module server main server/main.luau
//! module client ui client/ui.luau
//! module shared common shared/common.luau
//! asset texture icon assets/textures/icon.png
//! ```
//!
//! Format 2 requires each source path to start with its side directory; the
//! entry must be server or shared. Client modules cannot be imported/executed on
//! the server. Texture paths must start with `assets/textures/` and end in `.png`.
//! All format 2 paths obey the existing depth/length/character bounds and forbid
//! dot-prefixed components. Assets are limited to 64/package, 256 total, and
//! 256 KiB/file, sharing the 4 MiB aggregate discovery budget with manifests and
//! all sources. No wildcard, directory asset, arbitrary data or save file type
//! is supported. PNG is a classification, not image validity/decoded-size proof.
//! Authors explicitly approve everything in client/shared sources and assets;
//! this cannot detect secrets deliberately copied/renamed into approved files.
//!
//! Discovery also builds a canonical `client::ClientBundle`, containing only
//! client/shared sources and declared asset bytes, plus package identities and
//! exact dependencies. Startup may bind a declared PNG to a package-owned
//! catalog texture. Local paths, entries, capabilities and original manifests
//! are never exported. SHA-256 keys and bounded decode/verify protect the
//! streamed artifact; the client never executes server startup scripts.
//!
//! The secure filesystem backend currently supports Unix only; other platforms
//! fail closed. Every path component, including root ancestors, is opened with
//! NOFOLLOW. Source/manifest files must be regular files. The bounded reader
//! rejects symlinks and special files without blocking on FIFOs. On macOS use
//! `/private/var/...`, not the `/var` symlink. Authors must finish edits before
//! discovery: this is not an atomic multi-file filesystem transaction. Hard
//! links/privileged concurrent filesystem changes are not an authenticity
//! boundary. All accepted source bytes are immutable after discovery returns.
//!
//! # Execution contract
//!
//! Retain the result in an `Arc` and call `ScriptWorker::execute_package` with
//! that snapshot, package identity and integer tick/seed input. A module uses
//! `import("arithmetic:operations")` to obtain a declared module's non-nil
//! export (table/function/scalar), never OS require or a filesystem path. Only
//! its own package and direct dependencies are visible. Imports retain their
//! defining module's authority even inside functions exported to other packages.
//! Dependencies can intentionally delegate capabilities through their exports;
//! packages share a VM and are not mutually untrusted security compartments.
//!
//! The entry module returns a function accepting `{tick, seed}` and returning an
//! integer. Modules are initialized lazily once per invocation. Successful
//! exports and initialization failures are cached. Cyclic initialization fails;
//! import nesting is limited to 32 modules. All imports share the invocation's
//! interrupt/time/memory limits. Its source limit also applies per loaded module.
//! Each invocation/retry gets a fresh VM, globals and export cache. Reusing the
//! same snapshot is deterministic with respect to local file changes.
//!
//! Discovery errors name the package directory and validated version/module
//! where available. Initialization errors name `package@version:module`. Later
//! exported-function errors retain named Luau source tracebacks, with the entry
//! identity in the host error. Initialization interrupts name the active module.
//!
//! Explicit development startup also runs entries with a bounded registration
//! host instead of integer inputs (see the sibling `startup` module). Semantic
//! actions and decision events bind the public gameplay Context and existing
//! host transactions. The frozen client artifact is delivered by the
//! join transport. Texture/material rendering uses the existing catalog and
//! voxel paths; hot reload remains separate. This local manifest format does
//! not alter world/save data.
//! Generation modules can also be registered at startup; their frozen sources
//! run in fresh VMs on loader threads through the public Contributor contract.

pub mod client;
mod files;
pub(super) mod manifest;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::{ScriptError, ScriptFailure};
use manifest::Manifest;

pub const MAX_PACKAGES: usize = 64;
pub const MAX_MODULES: usize = 256;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ASSETS: usize = 256;
pub const MAX_ASSET_BYTES: usize = 256 * 1024;
const MAX_MANIFEST_BYTES: usize = 16 * 1024;

/// Private fields prevent mutation or bypassing validation after discovery.
pub struct PackageSnapshot {
    packages: BTreeMap<String, Package>,
    client: Arc<client::ClientBundle>,
    /// Installed once after all startup entries finish, before the catalog is
    /// published. Tick workers only read these exact registered templates.
    creature_initials: OnceLock<BTreeMap<String, bloxgloom_host_api::entity::Payload>>,
}

struct Package {
    manifest: Manifest,
    sources: BTreeMap<String, String>,
    assets: BTreeMap<String, Vec<u8>>,
}

impl PackageSnapshot {
    pub(super) fn install_creature_initials(
        &self,
        creatures: &[bloxgloom_host_api::entity::MobileEntity],
    ) -> Result<(), ScriptError> {
        let initials = creatures
            .iter()
            .map(|creature| (creature.key.clone(), creature.behavior.initial()))
            .collect();
        self.creature_initials
            .set(initials)
            .map_err(|_| error("<creatures>", "creature templates already installed"))
    }

    pub(super) fn creature_initial(
        &self,
        key: &str,
    ) -> Option<bloxgloom_host_api::entity::Payload> {
        self.creature_initials.get()?.get(key).cloned()
    }

    /// Startup uses the existing public composition contract v1. Source semver
    /// dependencies are checked exactly by discovery, not squeezed into a u32.
    pub(super) fn startup_packages(
        &self,
    ) -> Result<Vec<bloxgloom_host_api::composition::Package>, ScriptError> {
        use bloxgloom_host_api::composition::{
            ACTIONS, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, MACHINES, MOBILE_ENTITIES,
            OWNER_SYSTEMS, Package, STORAGE,
        };
        self.packages
            .iter()
            .map(|(name, package)| {
                if name == "bloxgloom"
                    || package.manifest.requires.iter().any(|c| {
                        c != CONTENT
                            && c != GENERATION
                            && c != ACTIONS
                            && c != OWNER_SYSTEMS
                            && c != STORAGE
                            && c != INVENTORY_SCREENS
                            && c != MOBILE_ENTITIES
                            && c != MACHINES
                    })
                {
                    return Err(error(
                        &self.identity(&self.entry(name)?),
                        "reserved namespace or unsupported startup capability",
                    ));
                }
                Ok(Package {
                    key: format!("{name}:package"),
                    version: 1,
                    dependencies: package
                        .manifest
                        .dependencies
                        .keys()
                        .map(|name| Dependency {
                            package: format!("{name}:package"),
                            version: 1,
                        })
                        .collect(),
                    requires: package.manifest.requires.iter().cloned().collect(),
                })
            })
            .collect()
    }

    pub(super) fn permits_content(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            p.manifest
                .requires
                .contains(bloxgloom_host_api::composition::CONTENT)
        })
    }

    pub(super) fn permits_creatures(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            [
                bloxgloom_host_api::composition::CONTENT,
                bloxgloom_host_api::composition::MOBILE_ENTITIES,
            ]
            .into_iter()
            .all(|capability| p.manifest.requires.contains(capability))
        })
    }

    pub(super) fn permits_machines(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            [
                bloxgloom_host_api::composition::CONTENT,
                bloxgloom_host_api::composition::MACHINES,
                bloxgloom_host_api::composition::INVENTORY_SCREENS,
            ]
            .into_iter()
            .all(|capability| p.manifest.requires.contains(capability))
        })
    }

    pub(super) fn permits_storage_screens(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            [
                bloxgloom_host_api::composition::CONTENT,
                bloxgloom_host_api::composition::STORAGE,
                bloxgloom_host_api::composition::INVENTORY_SCREENS,
            ]
            .into_iter()
            .all(|capability| p.manifest.requires.contains(capability))
        })
    }

    pub(super) fn permits_generation(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            p.manifest
                .requires
                .contains(bloxgloom_host_api::composition::GENERATION)
        })
    }

    pub(super) fn permits_actions(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            p.manifest
                .requires
                .contains(bloxgloom_host_api::composition::ACTIONS)
        })
    }

    pub(super) fn permits_systems(&self, package: &str) -> bool {
        self.packages.get(package).is_some_and(|p| {
            p.manifest
                .requires
                .contains(bloxgloom_host_api::composition::OWNER_SYSTEMS)
        })
    }

    /// Conservative installation identity, persisted as the public handler
    /// version. Includes every frozen source (including dependency helpers), not
    /// just the entry. Like catalog fingerprints this is not an authenticity hash.
    pub(super) fn gameplay_version(&self, entry: &str, revision: u16) -> u64 {
        self.execution_identity(b"luau-action-v1", entry, &revision.to_le_bytes())
    }

    pub(super) fn system_schema(&self, entry: &str, schema: u32, revision: u16) -> u64 {
        let mut identity = schema.to_le_bytes().to_vec();
        identity.extend(revision.to_le_bytes());
        self.execution_identity(b"luau-owner-system-v1", entry, &identity)
    }

    pub(super) fn creature_schema(&self, entry: &str, schema: u16, revision: u16) -> u64 {
        let mut identity = schema.to_le_bytes().to_vec();
        identity.extend(revision.to_le_bytes());
        self.execution_identity(b"luau-creature-v1", entry, &identity)
    }

    pub(super) fn machine_schema(&self, entry: &str, schema: u16, revision: u16) -> u64 {
        let mut identity = schema.to_le_bytes().to_vec();
        identity.extend(revision.to_le_bytes());
        self.execution_identity(b"luau-machine-v1", entry, &identity)
    }

    pub(super) fn entity_schema(
        &self,
        key: &str,
        schema: u16,
        state_bytes: u16,
        public_bytes: u16,
        delay: Option<u32>,
    ) -> u64 {
        let mut identity = schema.to_le_bytes().to_vec();
        identity.extend(state_bytes.to_le_bytes());
        identity.extend(public_bytes.to_le_bytes());
        // Zero is not a valid delay, so it unambiguously denotes suspension.
        identity.extend(delay.unwrap_or(0).to_le_bytes());
        self.execution_identity(b"luau-entity-fixed-bytes-v1", key, &identity)
    }

    fn execution_identity(&self, domain: &[u8], entry: &str, revision: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut field = |bytes: &[u8]| {
            for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        field(domain);
        field(entry.as_bytes());
        field(revision);
        field(&(self.packages.len() as u64).to_le_bytes());
        for (name, package) in &self.packages {
            field(name.as_bytes());
            field(package.manifest.version.to_string().as_bytes());
            field(package.manifest.entry.as_bytes());
            field(&(package.manifest.dependencies.len() as u64).to_le_bytes());
            for (name, version) in &package.manifest.dependencies {
                field(name.as_bytes());
                field(version.to_string().as_bytes());
            }
            field(&(package.manifest.requires.len() as u64).to_le_bytes());
            for capability in &package.manifest.requires {
                field(capability.as_bytes());
            }
            field(&(package.sources.len() as u64).to_le_bytes());
            for (module, source) in &package.sources {
                field(module.as_bytes());
                field(source.as_bytes());
            }
        }
        hash
    }

    /// Read immediate package directories in lexical order. The host must call
    /// this off the window thread. Failure never publishes a partial snapshot.
    pub fn discover(root: &Path) -> Result<Self, ScriptError> {
        let root_dir = files::Directory::root(root).map_err(|e| error("<packages>", e))?;
        let names = root_dir
            .names(MAX_PACKAGES)
            .map_err(|e| error("<packages>", e))?;
        let mut packages = BTreeMap::new();
        let mut module_count = 0;
        let mut asset_count = 0;
        let mut total_bytes = 0usize;
        for name in names {
            if !manifest::identifier(&name) {
                return Err(error(&name, "invalid package directory name"));
            }
            let directory = root_dir.child(&name).map_err(|e| error(&name, e))?;
            let text = directory
                .read(
                    "package.txt",
                    MAX_MANIFEST_BYTES.min(MAX_TOTAL_BYTES - total_bytes),
                )
                .map_err(|e| error(&name, e))?;
            total_bytes += text.len();
            let manifest = Manifest::parse(&name, &text)?;
            let owner = format!("{}@{}", name, manifest.version);
            module_count += manifest.modules.len();
            if module_count > MAX_MODULES {
                return Err(error(&owner, "too many modules in package set"));
            }
            let mut sources = BTreeMap::new();
            for (module, path) in &manifest.modules {
                let id = format!("{owner}:{module}");
                let remaining = MAX_TOTAL_BYTES
                    .checked_sub(total_bytes)
                    .ok_or_else(|| error(&id, "package set byte limit exceeded"))?;
                let source = directory
                    .read(path, MAX_SOURCE_BYTES.min(remaining))
                    .map_err(|e| error(&id, e))?;
                total_bytes += source.len();
                sources.insert(module.clone(), source);
            }
            asset_count += manifest.assets.len();
            if asset_count > MAX_ASSETS {
                return Err(error(&owner, "too many assets in package set"));
            }
            let mut assets = BTreeMap::new();
            for (asset, path) in &manifest.assets {
                let bytes = directory
                    .read_bytes(path, MAX_ASSET_BYTES.min(MAX_TOTAL_BYTES - total_bytes))
                    .map_err(|e| error(&owner, e))?;
                total_bytes += bytes.len();
                assets.insert(asset.clone(), bytes);
            }
            packages.insert(
                name,
                Package {
                    manifest,
                    sources,
                    assets,
                },
            );
        }
        for (name, package) in &packages {
            for (dependency, version) in &package.manifest.dependencies {
                if packages
                    .get(dependency)
                    .is_none_or(|p| &p.manifest.version != version)
                {
                    return Err(error(
                        &format!("{name}@{}", package.manifest.version),
                        format!("missing dependency {dependency}@{version}"),
                    ));
                }
            }
        }
        // Build only from the validated immutable snapshot, never reopen files or
        // serialize a local manifest (which contains server-only paths/entry).
        let client = Arc::new(client::ClientBundle::from_packages(&packages)?);
        Ok(Self {
            packages,
            client,
            creature_initials: OnceLock::new(),
        })
    }

    pub fn client_bundle(&self) -> &Arc<client::ClientBundle> {
        &self.client
    }

    pub(super) fn entry(&self, package: &str) -> Result<String, ScriptError> {
        let p = self
            .packages
            .get(package)
            .ok_or_else(|| error(package, "unknown package"))?;
        Ok(format!("{package}:{}", p.manifest.entry))
    }

    pub(super) fn identity(&self, key: &str) -> String {
        if let Some((name, module)) = key.split_once(':')
            && let Some(package) = self.packages.get(name)
        {
            return format!("{name}@{}:{module}", package.manifest.version);
        }
        key.to_owned()
    }

    pub(super) fn source(&self, key: &str) -> Option<&str> {
        let (package, module) = key.split_once(':')?;
        let package = self.packages.get(package)?;
        if package.manifest.sides.get(module) == Some(&manifest::SourceSide::Client) {
            return None;
        }
        package.sources.get(module).map(String::as_str)
    }

    /// Only the entry package's explicitly declared texture asset, never a
    /// filesystem path or another package's bytes, may back a startup texture.
    pub(super) fn texture_asset(&self, package: &str, asset: &str) -> Option<&[u8]> {
        let package = self.packages.get(package)?;
        (package.manifest.asset_kinds.get(asset) == Some(&1))
            .then(|| package.assets.get(asset).map(Vec::as_slice))?
    }

    /// Imports are `package:module`, never paths. Visibility is lexical to the
    /// importing module, not the entry point or the runtime call stack.
    pub(super) fn resolve(&self, caller: &str, import: &str) -> Result<String, ScriptError> {
        let reject = || {
            error(
                &self.identity(caller),
                "import must name a declared module in this package or a direct dependency",
            )
        };
        if import.len() > 129 {
            return Err(reject());
        }
        let (target, module) = import.split_once(':').ok_or_else(reject)?;
        if !manifest::identifier(target) || !manifest::identifier(module) {
            return Err(reject());
        }
        let (owner, _) = caller.split_once(':').ok_or_else(reject)?;
        let package = self.packages.get(owner).ok_or_else(reject)?;
        if (target != owner && !package.manifest.dependencies.contains_key(target))
            || self.source(import).is_none()
        {
            return Err(reject());
        }
        Ok(import.to_owned())
    }
}

pub(super) fn error(owner: &str, message: impl ToString) -> ScriptError {
    ScriptError {
        module: owner.to_owned(),
        failure: ScriptFailure::Package(message.to_string()),
    }
}

#[cfg(test)]
mod tests;
