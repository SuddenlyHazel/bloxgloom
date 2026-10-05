//! GPU packing is independent of stable catalog texture identities.
use crate::content::{Catalog, TextureId};
use std::collections::{HashMap, HashSet};

pub(crate) struct Layers {
    pub definitions: Vec<usize>,
    pub by_texture: Vec<u32>,
    /// Auxiliary sampling of a packed companion uses its original image,
    /// rather than silently substituting the base albedo.
    pub auxiliary_flags: Vec<u32>,
}

#[cfg(test)]
mod tests;

impl Layers {
    pub fn new(catalog: &Catalog) -> Self {
        let definitions = catalog.textures();
        let by_key: HashMap<_, _> = definitions
            .iter()
            .enumerate()
            .map(|(id, definition)| (definition.key.as_ref(), id))
            .collect();
        let used: HashSet<TextureId> = catalog
            .states()
            .flat_map(|state| state.face_textures)
            .chain(catalog.items().map(|item| item.texture))
            .collect();
        let companions: Vec<_> = definitions
            .iter()
            .enumerate()
            .map(|(id, definition)| {
                if definition.alpha_cutout
                    || used.contains(&TextureId::new(id as u32))
                    || by_key.contains_key(format!("{}_n", definition.key).as_str())
                    || by_key.contains_key(format!("{}_s", definition.key).as_str())
                {
                    return None;
                }
                definition
                    .key
                    .strip_suffix("_n")
                    .or_else(|| definition.key.strip_suffix("_s"))
                    .and_then(|base| by_key.get(base).copied())
            })
            .collect();
        let mut packed = Vec::new();
        let mut by_texture = vec![0; definitions.len()];
        let mut auxiliary_flags = vec![0; definitions.len()];
        for (id, companion) in companions.iter().enumerate() {
            if companion.is_none() {
                by_texture[id] = packed.len() as u32;
                packed.push(id);
            }
        }
        for (id, companion) in companions.iter().enumerate() {
            if let Some(mut base) = *companion {
                // Nested names are legal. Each step removes a suffix, so this
                // cannot cycle even when maps register before their albedo.
                while let Some(parent) = companions[base] {
                    base = parent;
                }
                by_texture[id] = by_texture[base];
                let kind = if definitions[id].key.ends_with("_n") {
                    1
                } else {
                    2
                };
                auxiliary_flags[id] =
                    (kind << 24) | (u32::from(definitions[id].alpha_cutout) << 26);
            }
        }
        Self {
            definitions: packed,
            by_texture,
            auxiliary_flags,
        }
    }
}
