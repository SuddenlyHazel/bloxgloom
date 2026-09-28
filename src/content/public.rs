//! Compilation of public declarations into the existing hot-path catalog.
use super::*;
use bloxgloom_host_api::{RegistrationError as Error, content as api};
#[cfg(test)]
mod tests;

impl Catalog {
    pub(crate) fn public_texture(&mut self, definition: &api::Texture) -> Result<(), Error> {
        self.register_texture(texture_definition(definition))
            .map(|_| ())
            .map_err(|e| error(&definition.key, e))
    }

    /// Embedded images are decoded by the material tests; compiling their public
    /// metadata must not decode 25 MiB again for each builtin catalog construction.
    pub(super) fn embedded_texture(&mut self, definition: &api::Texture) {
        let texture = texture_definition(definition);
        assert!(valid_key(&texture.key) && self.texture_keys.insert(texture.key.to_string()));
        self.texture_fingerprints
            .push(fingerprint_texture(&texture));
        self.textures.push(texture);
    }

    fn texture_key(&self, key: &str) -> Result<TextureId, Error> {
        self.textures
            .iter()
            .position(|t| t.key == key)
            .map(|i| TextureId(i as u32))
            .ok_or_else(|| Error(format!("missing texture {key}")))
    }

    fn public_faces(&self, faces: &api::FaceTextures) -> Result<BlockTextures, Error> {
        Ok(BlockTextures {
            top: self.texture_key(&faces.top)?,
            side: self.texture_key(&faces.side)?,
            bottom: self.texture_key(&faces.bottom)?,
        })
    }

    pub(crate) fn public_block_type_at(
        &mut self,
        id: BlockTypeId,
        b: &api::Block,
    ) -> Result<(), Error> {
        validate_display(&b.name, b.swatch)?;
        let mut properties = b
            .properties
            .iter()
            .map(|p| PropertyDef {
                name: p.name.clone().into(),
                values: p.values.iter().cloned().map(Into::into).collect(),
            })
            .collect::<Vec<_>>();
        properties.sort_by(|a, b| a.name.cmp(&b.name));
        for property in &mut properties {
            property.values.sort();
        }
        self.register_block(BlockDef {
            id,
            key: b.key.clone().into(),
            name: b.name.clone().into(),
            swatch: b.swatch,
            textures: self.public_faces(&b.textures)?,
            solid: b.solid,
            opaque: b.material == api::Material::Opaque,
            cutout: b.material == api::Material::Cutout,
            plant: b.geometry != api::Geometry::Cube,
            replaceable: b.replaceable,
            supports_plant: b.supports_plant,
            flammable: b.flammable,
            emission: b.emission,
            reflectance: b.reflectance,
            properties,
        })
        .map_err(|e| error(&b.key, e))?;
        if b.geometry == api::Geometry::NarrowCrossedPlant {
            self.narrow_plants.insert(b.key.clone());
        }
        Ok(())
    }

    pub(crate) fn public_block(&mut self, b: &api::Block) -> Result<(), Error> {
        if b.states.is_empty() || b.states.len() > 4096 {
            return Err(Error(format!(
                "{}: expected 1–4096 explicit legal states",
                b.key
            )));
        }
        let id = BlockTypeId(self.blocks.len() as u32);
        self.public_block_type_at(id, b)?;
        // Canonical property ordering makes assignment independent of declaration order.
        let mut states = b.states.clone();
        for state in &mut states {
            state.properties.sort();
        }
        states.sort_by(|a, b| a.properties.cmp(&b.properties));
        for state in states {
            let textures = state
                .textures
                .as_ref()
                .map(|t| self.public_faces(t))
                .transpose()?;
            self.register_state_with_emission(
                BlockStateId(self.states.len() as u32),
                id,
                state.properties,
                textures,
                state.emission,
            )
            .map_err(|e| error(&b.key, e))?;
        }
        Ok(())
    }

    pub(crate) fn public_item_at(&mut self, id: ItemId, i: &api::Item) -> Result<(), Error> {
        validate_display(&i.name, i.swatch)?;
        if !i.drop_animation.valid() {
            return Err(Error(format!("{}: invalid drop animation", i.key)));
        }
        if let api::Components::Opaque {
            version, max_bytes, ..
        } = i.components
            && (version == 0 || max_bytes == 0 || max_bytes > 1024)
        {
            return Err(Error(format!("{}: invalid component schema", i.key)));
        }
        if i.placeable.is_some()
            && matches!(i.components, api::Components::Opaque { required: true, .. })
        {
            return Err(Error(format!(
                "{}: voxel placement/refunds require a valid component-free item",
                i.key
            )));
        }
        let placeable = i
            .placeable
            .as_ref()
            .map(|key| {
                self.state_by_key(key)
                    .ok_or_else(|| Error(format!("{}: missing placement state {key}", i.key)))
            })
            .transpose()?;
        self.register_item(ItemDef {
            id,
            key: i.key.clone().into(),
            name: i.name.clone().into(),
            swatch: i.swatch,
            texture: self.texture_key(&i.texture)?,
            placeable,
            sprite: i.sprite,
        })
        .map_err(|e| error(&i.key, e))?;
        if i.drop_size != api::DropSize::Normal {
            self.drop_sizes.insert(i.key.clone(), i.drop_size);
        }
        if i.drop_animation != api::DropAnimation::default() {
            self.drop_animations.insert(i.key.clone(), i.drop_animation);
        }
        if i.components != api::Components::Unstructured {
            self.item_components
                .insert(i.key.clone(), i.components.clone());
        }
        Ok(())
    }

    pub(crate) fn public_item(&mut self, i: &api::Item) -> Result<(), Error> {
        self.public_item_at(ItemId(self.items.len().max(1) as u32), i)
    }

    pub(crate) fn valid_item_components(
        &self,
        item: ItemId,
        payload: Option<(u16, &[u8])>,
    ) -> bool {
        let Some(item) = self.item(item) else {
            return false;
        };
        match self.item_components.get(item.key.as_ref()) {
            None | Some(api::Components::Unstructured) => true,
            Some(api::Components::None) => payload.is_none(),
            Some(api::Components::Opaque {
                version,
                max_bytes,
                required,
                ..
            }) => payload.map_or(!required, |(v, bytes)| {
                v == *version && !bytes.is_empty() && bytes.len() <= usize::from(*max_bytes)
            }),
        }
    }

    pub(crate) fn plant_selection_margin(&self, state: BlockStateId) -> f64 {
        if self
            .block(state)
            .is_some_and(|b| self.narrow_plants.contains(b.key.as_ref()))
        {
            0.35
        } else {
            0.22
        }
    }

    pub(super) fn component_fingerprint(&self, key: &str, hash: &mut u64) {
        match self.item_components.get(key) {
            None | Some(api::Components::Unstructured) => {}
            Some(api::Components::None) => hash_bytes(hash, b"components:none"),
            Some(api::Components::Opaque {
                version,
                fingerprint,
                max_bytes,
                required,
            }) => {
                hash_bytes(hash, b"components:opaque");
                hash_bytes(hash, &version.to_le_bytes());
                hash_bytes(hash, &fingerprint.to_le_bytes());
                hash_bytes(hash, &max_bytes.to_le_bytes());
                hash_bytes(hash, &[*required as u8]);
            }
        }
    }
}

fn validate_display(name: &str, swatch: [f32; 4]) -> Result<(), Error> {
    if name.is_empty()
        || name.len() > 255
        || name.chars().any(char::is_control)
        || swatch
            .iter()
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
    {
        return Err(Error("invalid display name or swatch".into()));
    }
    Ok(())
}

fn error(key: &str, error: RegistrationError) -> Error {
    Error(format!("{key}: {error:?}"))
}

fn texture_definition(definition: &api::Texture) -> TextureDef {
    TextureDef {
        key: definition.key.clone().into(),
        png: definition.png.clone(),
        stitch_edges: definition.stitch_edges,
        stitch_vertical: definition.stitch_vertical,
        alpha_cutout: definition.alpha_cutout,
        emission_strength: definition.emission_strength,
    }
}
