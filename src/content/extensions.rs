//! Host assignment for public declarations; extensions never choose wire IDs.
use super::*;
use bloxgloom_host_api::{CubeBlock, RegistrationError as ApiError, StorageBlockEntity};

impl Catalog {
    pub(crate) fn extension_cube(&mut self, definition: &CubeBlock) -> Result<(), ApiError> {
        use bloxgloom_host_api::content::*;
        self.public_block(&Block {
            key: definition.key.clone(),
            name: definition.name.clone(),
            swatch: [0.55, 0.33, 0.14, 1.0],
            textures: FaceTextures::uniform(&definition.texture),
            solid: true,
            material: Material::Opaque,
            geometry: Geometry::Cube,
            replaceable: false,
            supports_plant: false,
            flammable: false,
            emission: 0,
            reflectance: [140, 90, 45],
            properties: vec![],
            states: vec![BlockState::default()],
        })?;
        self.public_item(&Item {
            key: definition.key.clone(),
            name: definition.name.clone(),
            swatch: [0.55, 0.33, 0.14, 1.0],
            texture: definition.texture.clone(),
            placeable: Some(definition.key.clone()),
            sprite: false,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            drop_policy: Default::default(),
            components: Components::Unstructured,
        })
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
