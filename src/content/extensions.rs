//! Host assignment for public declarations; extensions never choose wire IDs.
use super::*;
use bloxgloom_host_api::{CubeBlock, RegistrationError as ApiError, StorageBlockEntity};

impl Catalog {
    pub(crate) fn extension_cube(&mut self, definition: &CubeBlock) -> Result<(), ApiError> {
        let texture = self
            .textures
            .iter()
            .position(|t| t.key == definition.texture)
            .map(|i| TextureId(i as u32))
            .ok_or_else(|| ApiError(format!("missing texture {}", definition.texture)))?;
        let block = BlockTypeId(self.blocks.len() as u32);
        let state = BlockStateId(self.states.len() as u32);
        let item = ItemId(self.items.len() as u32);
        let error = |e: RegistrationError| ApiError(format!("{}: {e:?}", definition.key));
        self.register_block(BlockDef {
            id: block,
            key: definition.key.clone().into(),
            name: definition.name.clone().into(),
            swatch: [0.55, 0.33, 0.14, 1.0],
            textures: BlockTextures {
                top: texture,
                side: texture,
                bottom: texture,
            },
            solid: true,
            opaque: true,
            cutout: false,
            plant: false,
            replaceable: false,
            supports_plant: false,
            flammable: false,
            emission: 0,
            reflectance: [140, 90, 45],
            properties: vec![],
        })
        .map_err(error)?;
        self.register_state(state, block, vec![], None)
            .map_err(error)?;
        self.register_item(ItemDef {
            id: item,
            key: definition.key.clone().into(),
            name: definition.name.clone().into(),
            swatch: [0.55, 0.33, 0.14, 1.0],
            texture,
            placeable: Some(state),
            sprite: false,
        })
        .map_err(error)
    }

    pub(crate) fn extension_storage(
        &mut self,
        definition: &StorageBlockEntity,
    ) -> Result<(), ApiError> {
        // Changes to lifecycle semantics must not reinterpret stored entities.
        let mut hash = 0xcbf29ce484222325u64;
        let mut field = |bytes: &[u8]| {
            for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        for key in [
            "storage-v2",
            &definition.entity,
            &definition.block,
            &definition.placement_item,
            &definition.anchor_state,
        ] {
            field(key.as_bytes());
        }
        field(&(definition.slots as u64).to_le_bytes());
        for cell in &definition.footprint {
            for coordinate in cell.offset {
                field(&coordinate.to_le_bytes());
            }
            field(cell.state.as_bytes());
        }
        self.register_entity_type(EntityTypeDef {
            id: EntityTypeId(self.entities.len() as u32),
            key: definition.entity.clone().into(),
            schema_version: 2,
            schema_fingerprint: hash,
        })
        .map_err(|e| ApiError(format!("{}: {e:?}", definition.entity)))
    }

    pub(crate) fn state_by_key(&self, key: &str) -> Option<BlockStateId> {
        self.state_by_key.get(key).copied()
    }
    pub(crate) fn block_by_key(&self, key: &str) -> Option<BlockTypeId> {
        self.blocks
            .iter()
            .flatten()
            .find(|b| b.key == key)
            .map(|b| b.id)
    }
}
