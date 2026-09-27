//! Separately compiled extension. No dependency on engine internals.
use bloxgloom_host_api::{
    CubeBlock, Extension, FootprintCell, Registrar, RegistrationError, StorageBlockEntity,
};

pub const KEY: &str = "fixture:tall_store";
pub struct TallStore;
impl Extension for TallStore {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.cube_block(CubeBlock {
            key: KEY.into(),
            name: "TALL STORE".into(),
            texture: "bloxgloom:chest_side".into(),
        })?;
        registrar.storage_block_entity(StorageBlockEntity {
            entity: KEY.into(),
            block: KEY.into(),
            placement_item: KEY.into(),
            anchor_state: KEY.into(),
            slots: 9,
            footprint: vec![
                FootprintCell {
                    offset: [0, 0, 0],
                    state: KEY.into(),
                },
                FootprintCell {
                    offset: [0, 1, 0],
                    state: KEY.into(),
                },
            ],
        })
    }
}
