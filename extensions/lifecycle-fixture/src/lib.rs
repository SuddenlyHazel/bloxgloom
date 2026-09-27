//! Separately compiled extension. No dependency on engine internals.
use bloxgloom_host_api::{
    CubeBlock, Extension, FootprintCell, Registrar, RegistrationError, StorageBlockEntity,
};

pub const KEY: &str = "fixture:tall_store";
pub mod anchored;
pub mod content;
pub mod actions;
pub mod creature;
pub mod machine;
pub mod system;
pub struct Fixture;
impl Extension for Fixture {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        content::Content.register(r)?;
        TallStore.register(r)?;
        r.mobile_entity(creature::definition())?;
        machine::register(r)?;
        r.owner_system(system::definition())?;
        anchored::SignalPost.register(r)?;
        actions::register(r)
    }
}
pub struct TallStore;
impl Extension for TallStore {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.cube_block(CubeBlock {
            key: KEY.into(),
            name: "TALL STORE".into(),
            texture: "bloxgloom:chest_side".into(),
        })?;
        registrar.inventory_screen(bloxgloom_host_api::InventoryScreen::storage(
            KEY,
            KEY,
            "TALL STORE",
            9,
            9,
            vec![[0, 0, 0], [0, 1, 0]],
        ))?;
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
