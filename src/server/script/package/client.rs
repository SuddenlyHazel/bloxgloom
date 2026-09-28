//! Canonical, client-safe package set, independent of filesystem paths. Version 3
//! is an uncompressed little-endian length-prefixed format, not a save or network
//! protocol. No entry, executable server capabilities, local paths or original manifests are
//! exported. All package identities/direct exact dependencies remain present,
//! including empty server-only libraries, so dependency validation is complete.
//!
//! SHA-256 identifies exact canonical bytes. Verification requires an expected
//! key from a trusted session/manifest; a hash supplied by the same untrusted
//! sender as the bytes is integrity checking, not authentication or signing.
//! Texture payloads are opaque PNG-classified bytes, NOT decoded/validated images.
//! A future image loader must impose its own decoded-dimension/memory limits.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::manifest::{SourceSide, identifier, valid_version};
use super::{MAX_ASSET_BYTES, MAX_ASSETS, MAX_MODULES, MAX_PACKAGES, MAX_SOURCE_BYTES};
use super::{MAX_TOTAL_BYTES, Package, ScriptError, error};

mod declarations;

const MAGIC: &[u8] = b"BGCLIENT\x03";
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

    /// Domain/version prefix avoids collisions with other future cache formats.
    pub fn cache_name(&self) -> String {
        use std::fmt::Write;
        let mut name = String::from("client-v3-sha256-");
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
    /// Only `texture` assets are currently supported. Keys are logical names,
    /// never extraction paths. No filesystem extraction API is provided.
    pub textures: BTreeMap<String, Vec<u8>>,
}

/// Immutable bytes and decoded view, published together only after validation.
#[derive(Debug)]
pub struct ClientBundle {
    bytes: Vec<u8>,
    key: CacheKey,
    packages: BTreeMap<String, ClientPackage>,
    declarations: Option<declarations::Startup>,
}

impl ClientBundle {
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
                writer.count(1)?; // texture classification
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
        if reader.take(MAGIC.len())? != MAGIC {
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
            for _ in 0..count {
                let key = reader.identifier()?;
                ordered(&textures, &key)?;
                if reader.count(1)? != 1 {
                    return Err(invalid());
                }
                let bytes = reader.field(MAX_ASSET_BYTES.min(MAX_TOTAL_BYTES - payload))?;
                payload += bytes.len();
                textures.insert(key, bytes.to_vec());
            }
            packages.insert(
                name,
                ClientPackage {
                    version,
                    dependencies,
                    sources,
                    textures,
                },
            );
        }
        let declarations = declarations::Startup::decode(&mut reader, &packages)?;
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
        Ok(Self {
            bytes: bytes.to_vec(),
            key: expected,
            packages,
            declarations,
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
