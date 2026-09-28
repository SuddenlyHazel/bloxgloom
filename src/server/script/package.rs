//! Bounded, deterministic local package snapshots. No filesystem access occurs
//! after discovery, including on retries or imports.
//!
//! # Local package format v1
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
//! host instead of integer inputs (see the sibling `startup` module). Gameplay
//! and generation bindings, handles, transaction admission, scheduling and durable
//! receipts remain unbound. Assets, compatibility negotiation, server/client
//! declarations, distribution hashes, network/UI and hot reload are not added.
//! No world/save/wire format changes accompany this local manifest format.

mod files;
pub(super) mod manifest;

use std::collections::BTreeMap;
use std::path::Path;

use super::{ScriptError, ScriptFailure};
use manifest::Manifest;

pub const MAX_PACKAGES: usize = 64;
pub const MAX_MODULES: usize = 256;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 16 * 1024;

/// Private fields prevent mutation or bypassing validation after discovery.
pub struct PackageSnapshot {
    packages: BTreeMap<String, Package>,
}

struct Package {
    manifest: Manifest,
    sources: BTreeMap<String, String>,
}

impl PackageSnapshot {
    /// Startup uses the existing public composition contract v1. Source semver
    /// dependencies are checked exactly by discovery, not squeezed into a u32.
    pub(super) fn startup_packages(
        &self,
    ) -> Result<Vec<bloxgloom_host_api::composition::Package>, ScriptError> {
        use bloxgloom_host_api::composition::{CONTENT, Dependency, Package};
        self.packages
            .iter()
            .map(|(name, package)| {
                if name == "bloxgloom" || package.manifest.requires.iter().any(|c| c != CONTENT) {
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

    /// Read immediate package directories in lexical order. The host must call
    /// this off the window thread. Failure never publishes a partial snapshot.
    pub fn discover(root: &Path) -> Result<Self, ScriptError> {
        let root_dir = files::Directory::root(root).map_err(|e| error("<packages>", e))?;
        let names = root_dir
            .names(MAX_PACKAGES)
            .map_err(|e| error("<packages>", e))?;
        let mut packages = BTreeMap::new();
        let mut module_count = 0;
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
            packages.insert(name, Package { manifest, sources });
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
        Ok(Self { packages })
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
        self.packages
            .get(package)?
            .sources
            .get(module)
            .map(String::as_str)
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
