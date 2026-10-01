//! One estimated admission policy shared by native collection, Luau startup,
//! and delivered metadata. Estimates deliberately cover owned string/container
//! overhead, rather than treating compact serialized bytes as heap usage.
use bloxgloom_host_api::{
    RegistrationError,
    composition::Package,
    content::{Block, Tag},
};

pub(crate) const MAX_COUNT: usize = 4096;
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Budget {
    count: usize,
    bytes: usize,
}

impl Budget {
    pub(crate) fn reserve_budget(
        &mut self,
        other: &Self,
        key: &str,
    ) -> Result<(), RegistrationError> {
        self.reserve(other.count, other.bytes, key)
    }

    pub(crate) fn reserve(
        &mut self,
        count: usize,
        bytes: usize,
        key: &str,
    ) -> Result<(), RegistrationError> {
        let attempted_count = self.count.saturating_add(count);
        let attempted_bytes = self.bytes.saturating_add(bytes);
        if attempted_count > MAX_COUNT || attempted_bytes > MAX_BYTES {
            return Err(RegistrationError(format!(
                "{key} content declarations/installation: attempted {attempted_count}; maximum {MAX_COUNT}; estimated declaration bytes/installation: attempted {attempted_bytes}; maximum {MAX_BYTES}"
            )));
        }
        self.count = attempted_count;
        self.bytes = attempted_bytes;
        Ok(())
    }
}

pub(crate) fn block_bytes(block: &Block) -> usize {
    2048 + block
        .properties
        .iter()
        .map(|property| 256 + property.values.len() * 256)
        .sum::<usize>()
        + block
            .states
            .iter()
            .map(|state| 1024 + state.properties.len() * 512)
            .sum::<usize>()
}

pub(crate) fn tag_bytes(tag: &Tag) -> usize {
    256 + tag.members.len() * 256
}

pub(crate) fn package_bytes(package: &Package) -> usize {
    256 + (package.dependencies.len() + package.requires.len()) * 256
}

#[cfg(test)]
mod tests;
