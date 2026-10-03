//! Saved contracts and exact terrain algorithms have different identities.
//! Schemas describe interpretation of durable state; source digests describe
//! generation of chunks that have never been saved.
use super::{PackageSnapshot, manifest::SourceSide};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

impl PackageSnapshot {
    /// Dynamic imports can select any module exposed by a declared dependency.
    /// Protect the full transitive package closure rather than guessing which
    /// imports happened in one invocation. Cyclic dependency metadata is bounded
    /// and visited only once.
    pub(super) fn dependency_closure(&self, entry: &str) -> BTreeSet<String> {
        let mut closure = BTreeSet::new();
        let mut pending = vec![entry.split(':').next().unwrap_or(entry).to_owned()];
        while let Some(name) = pending.pop() {
            if let Some(package) = self.packages.get(&name)
                && closure.insert(name)
            {
                pending.extend(package.manifest.dependencies.keys().cloned());
            }
        }
        closure
    }

    /// Source-independent compatibility for handlers/codecs. Explicit package
    /// versions, dependency versions, capabilities and entry/layout/revision
    /// fields remain fences. Helpers and client code can evolve without changing
    /// an unrelated durable representation. This is not an authenticity hash.
    pub(super) fn execution_identity(&self, domain: &[u8], entry: &str, revision: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut field = |bytes: &[u8]| {
            for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        field(b"luau-explicit-persisted-contract-v2");
        field(domain);
        field(entry.as_bytes());
        field(revision);
        self.contract_fields(entry, &mut field);
        hash
    }

    /// Exact algorithm identity, including every server/shared source in the
    /// generation package and its transitive dependency closure. Client-only
    /// sources and visual assets cannot be imported by generation and are
    /// deliberately excluded. Even an unchanged declared revision cannot permit
    /// a source change to mix old and newly generated terrain after restart.
    pub(in crate::server::script) fn generation_source_identity(&self, module: &str) -> [u8; 32] {
        let mut hash = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(b"luau-generation-source-v1");
        field(module.as_bytes());
        self.contract_fields(module, &mut field);
        for name in self.dependency_closure(module) {
            let package = &self.packages[&name];
            field(name.as_bytes());
            let server_sources: Vec<_> = package
                .sources
                .iter()
                .filter(|(module, _)| package.manifest.sides[*module] != SourceSide::Client)
                .collect();
            field(&(server_sources.len() as u64).to_le_bytes());
            for (module, source) in server_sources {
                field(module.as_bytes());
                field(&[match package.manifest.sides[module] {
                    SourceSide::Server => 0,
                    SourceSide::Shared => 1,
                    SourceSide::Client => unreachable!("filtered client module"),
                }]);
                field(source.as_bytes());
            }
        }
        hash.finalize().into()
    }

    fn contract_fields(&self, entry: &str, field: &mut impl FnMut(&[u8])) {
        let closure = self.dependency_closure(entry);
        field(&(closure.len() as u64).to_le_bytes());
        for name in closure {
            let manifest = &self.packages[&name].manifest;
            field(name.as_bytes());
            field(manifest.version.as_bytes());
            field(manifest.entry.as_bytes());
            field(&(manifest.dependencies.len() as u64).to_le_bytes());
            for (dependency, version) in &manifest.dependencies {
                field(dependency.as_bytes());
                field(version.as_bytes());
            }
            field(&(manifest.requires.len() as u64).to_le_bytes());
            for capability in &manifest.requires {
                field(capability.as_bytes());
            }
        }
    }
}
